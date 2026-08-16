// Compile as a windowless GUI-subsystem app on Windows release builds so no
// console window flashes when launched as a background scheduled task.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use rumqttc::{Client, LastWill, MqttOptions, QoS};
use serde_json::json;
use std::collections::HashSet;
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::{Components, Disks, Networks, System};

/// How often to sample and publish, in seconds. Override with REPORT_INTERVAL.
const DEFAULT_INTERVAL: u64 = 60;

/// Load KEY=VALUE lines from a `resource-reporter.conf` sitting next to the
/// executable, setting each as an env var (only if not already set). This lets
/// the Windows agent run directly as a scheduled task with no launcher wrapper.
fn load_config_file() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let conf = dir.join("resource-reporter.conf");
            if let Ok(text) = std::fs::read_to_string(&conf) {
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        let k = k.trim();
                        let v = v.trim();
                        if std::env::var(k).is_err() {
                            std::env::set_var(k, v);
                        }
                    }
                }
            }
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Lowercase alnum, everything else -> underscore. Used for MQTT topic / entity ids.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Parse a docker mem string like "67.07MiB / 15.48GiB" -> MB (first value only).
fn parse_mem_mb(s: &str) -> Option<f64> {
    let first = s.split('/').next()?.trim();
    let (num, unit): (String, String) = first
        .chars()
        .partition(|c| c.is_ascii_digit() || *c == '.');
    let v: f64 = num.parse().ok()?;
    let mb = match unit.trim() {
        "B" => v / 1_048_576.0,
        "KiB" | "kB" | "KB" => v / 1024.0,
        "MiB" | "MB" => v,
        "GiB" | "GB" => v * 1024.0,
        "TiB" | "TB" => v * 1_048_576.0,
        _ => return None,
    };
    Some(mb)
}

struct ContainerStat {
    name: String,
    cpu: f64,
    mem_mb: f64,
}

/// Query docker for per-container stats. Returns None if docker isn't usable.
fn docker_stats() -> Option<Vec<ContainerStat>> {
    let out = Command::new("docker")
        .args([
            "stats",
            "--no-stream",
            "--format",
            "{{.Name}};{{.CPUPerc}};{{.MemUsage}}",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut stats = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0].trim().to_string();
        let cpu = parts[1].trim().trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
        let mem_mb = parse_mem_mb(parts[2]).unwrap_or(0.0);
        if !name.is_empty() {
            stats.push(ContainerStat { name, cpu, mem_mb });
        }
    }
    Some(stats)
}

fn main() {
    // Load config from resource-reporter.conf next to the exe (Windows path);
    // env vars from systemd EnvironmentFile still take precedence on Linux.
    load_config_file();

    // ---- Configuration ----
    let host = env_or("MQTT_HOST", "homeassistant");
    let port: u16 = env_or("MQTT_PORT", "1883").parse().unwrap_or(1883);
    let user = env_or("MQTT_USER", "mqtt");
    let pass = env_or("MQTT_PASS", "");
    let interval: u64 = env_or("REPORT_INTERVAL", &DEFAULT_INTERVAL.to_string())
        .parse()
        .unwrap_or(DEFAULT_INTERVAL);

    let raw_node = std::env::var("NODE_NAME")
        .ok()
        .or_else(System::host_name)
        .unwrap_or_else(|| "unknown".to_string());
    let node = sanitize(&raw_node);
    let friendly = raw_node.clone();

    let discovery_prefix = env_or("DISCOVERY_PREFIX", "homeassistant");
    let state_topic = format!("resource-reporter/{node}/state");
    let avail_topic = format!("resource-reporter/{node}/availability");
    let client_id = format!("resource-reporter-{node}");

    // Sensors go unavailable in HA if no update arrives within this window.
    let expire_after = interval * 3 + 30;

    // ---- MQTT connection ----
    let mut opts = MqttOptions::new(&client_id, &host, port);
    opts.set_keep_alive(Duration::from_secs(interval.max(15) + 15));
    if !user.is_empty() {
        opts.set_credentials(&user, &pass);
    }
    opts.set_last_will(LastWill::new(&avail_topic, "offline", QoS::AtLeastOnce, true));

    let (client, mut connection) = Client::new(opts, 50);
    std::thread::spawn(move || {
        for _ in connection.iter() {}
    });
    std::thread::sleep(Duration::from_secs(2));

    // ---- Platform capability probe ----
    let have_temp = !Components::new_with_refreshed_list().list().is_empty();
    let have_load = !cfg!(windows);
    let have_docker = docker_stats().is_some();

    let device = json!({
        "identifiers": [format!("resource_reporter_{node}")],
        "name": friendly,
        "model": "resource-reporter",
        "manufacturer": "Auroboros",
    });

    // Helper closure to publish one sensor discovery config.
    let publish_sensor = |key: &str,
                          name: &str,
                          unit: &str,
                          device_class: Option<&str>,
                          icon: &str| {
        let cfg_topic = format!("{discovery_prefix}/sensor/{node}_{key}/config");
        let mut cfg = json!({
            "name": name,
            "unique_id": format!("resource_reporter_{node}_{key}"),
            "state_topic": state_topic,
            "value_template": format!("{{{{ value_json.{key} }}}}"),
            "availability_topic": avail_topic,
            "icon": icon,
            "state_class": "measurement",
            "expire_after": expire_after,
            "device": device,
        });
        if !unit.is_empty() {
            cfg["unit_of_measurement"] = json!(unit);
        }
        if let Some(dc) = device_class {
            cfg["device_class"] = json!(dc);
        }
        let _ = client.publish(&cfg_topic, QoS::AtLeastOnce, true, cfg.to_string());
    };

    // ---- Fixed host sensors ----
    publish_sensor("cpu", "CPU Usage", "%", None, "mdi:cpu-64-bit");
    publish_sensor("mem", "Memory Usage", "%", None, "mdi:memory");
    publish_sensor("mem_used", "Memory Used", "GB", Some("data_size"), "mdi:memory");
    publish_sensor("swap", "Swap Usage", "%", None, "mdi:harddisk-plus");
    publish_sensor("disk", "Disk Usage", "%", None, "mdi:harddisk");
    publish_sensor("uptime", "Uptime", "h", Some("duration"), "mdi:timer-outline");
    publish_sensor("net_rx", "Network Down", "kB/s", Some("data_rate"), "mdi:download");
    publish_sensor("net_tx", "Network Up", "kB/s", Some("data_rate"), "mdi:upload");
    if have_temp {
        publish_sensor("temp", "CPU Temperature", "\u{00b0}C", Some("temperature"), "mdi:thermometer");
    }
    if have_load {
        publish_sensor("load1", "Load Average 1m", "", None, "mdi:chart-line");
    }
    if have_docker {
        publish_sensor("docker_running", "Docker Containers", "", None, "mdi:docker");
        publish_sensor("docker_cpu", "Docker CPU", "%", None, "mdi:docker");
        publish_sensor("docker_mem", "Docker Memory", "MB", Some("data_size"), "mdi:docker");
    }

    let _ = client.publish(&avail_topic, QoS::AtLeastOnce, true, "online");

    // ---- Sampling loop ----
    let mut sys = System::new_all();
    sys.refresh_cpu_all();
    let mut networks = Networks::new_with_refreshed_list();
    let mut last_net = Instant::now();
    std::thread::sleep(Duration::from_millis(500));

    // Track which per-container discovery configs we've published, to clean up.
    let mut known_containers: HashSet<String> = HashSet::new();

    loop {
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let cpu = sys.global_cpu_usage();
        let mem_total = sys.total_memory() as f64;
        let mem_used = sys.used_memory() as f64;
        let mem_pct = if mem_total > 0.0 { mem_used / mem_total * 100.0 } else { 0.0 };
        let mem_used_gb = mem_used / 1_073_741_824.0;

        let swap_total = sys.total_swap() as f64;
        let swap_pct = if swap_total > 0.0 {
            sys.used_swap() as f64 / swap_total * 100.0
        } else {
            0.0
        };

        // Disk usage for root (Linux) or largest disk (Windows).
        let disks = Disks::new_with_refreshed_list();
        let (mut d_total, mut d_avail) = (0u64, 0u64);
        for disk in disks.list() {
            let mp = disk.mount_point();
            if mp == std::path::Path::new("/") {
                d_total = disk.total_space();
                d_avail = disk.available_space();
                break;
            }
            // Fallback: pick the biggest disk (Windows has no "/").
            if disk.total_space() > d_total {
                d_total = disk.total_space();
                d_avail = disk.available_space();
            }
        }
        let disk_pct = if d_total > 0 {
            (d_total - d_avail) as f64 / d_total as f64 * 100.0
        } else {
            0.0
        };

        // Network throughput (kB/s) since last sample.
        networks.refresh();
        let elapsed = last_net.elapsed().as_secs_f64().max(0.001);
        last_net = Instant::now();
        let (mut rx, mut tx) = (0f64, 0f64);
        for (_, data) in networks.iter() {
            rx += data.received() as f64;
            tx += data.transmitted() as f64;
        }
        let net_rx = rx / elapsed / 1024.0;
        let net_tx = tx / elapsed / 1024.0;

        // Temperature.
        let components = Components::new_with_refreshed_list();
        let mut temp: Option<f32> = None;
        for c in components.list() {
            let label = c.label().to_lowercase();
            if label.contains("package") || label.contains("tctl") || label.contains("cpu") {
                temp = Some(c.temperature());
                break;
            }
        }
        if temp.is_none() {
            temp = components.list().first().map(|c| c.temperature());
        }

        let uptime_h = System::uptime() as f64 / 3600.0;
        let load1 = System::load_average().one;

        // ---- Docker ----
        let mut payload = json!({
            "cpu": format!("{:.1}", cpu),
            "mem": format!("{:.1}", mem_pct),
            "mem_used": format!("{:.2}", mem_used_gb),
            "swap": format!("{:.1}", swap_pct),
            "disk": format!("{:.1}", disk_pct),
            "uptime": format!("{:.1}", uptime_h),
            "net_rx": format!("{:.1}", net_rx),
            "net_tx": format!("{:.1}", net_tx),
            "temp": temp.map(|t| format!("{:.1}", t)),
            "load1": format!("{:.2}", load1),
        });

        if have_docker {
            if let Some(containers) = docker_stats() {
                let count = containers.len();
                let total_cpu: f64 = containers.iter().map(|c| c.cpu).sum();
                let total_mem: f64 = containers.iter().map(|c| c.mem_mb).sum();
                payload["docker_running"] = json!(count);
                payload["docker_cpu"] = json!(format!("{:.1}", total_cpu));
                payload["docker_mem"] = json!(format!("{:.0}", total_mem));

                let mut current: HashSet<String> = HashSet::new();
                for c in &containers {
                    let ckey = sanitize(&c.name);
                    current.insert(ckey.clone());

                    // Publish discovery for newly-seen containers.
                    if !known_containers.contains(&ckey) {
                        let cpu_key = format!("ct_{ckey}_cpu");
                        let mem_key = format!("ct_{ckey}_mem");
                        publish_sensor(
                            &cpu_key,
                            &format!("[C] {} CPU", c.name),
                            "%",
                            None,
                            "mdi:cube-outline",
                        );
                        publish_sensor(
                            &mem_key,
                            &format!("[C] {} Memory", c.name),
                            "MB",
                            Some("data_size"),
                            "mdi:cube-outline",
                        );
                        known_containers.insert(ckey.clone());
                    }
                    payload[format!("ct_{ckey}_cpu")] = json!(format!("{:.1}", c.cpu));
                    payload[format!("ct_{ckey}_mem")] = json!(format!("{:.0}", c.mem_mb));
                }

                // Clean up discovery for containers that disappeared.
                let gone: Vec<String> =
                    known_containers.difference(&current).cloned().collect();
                for ckey in gone {
                    for suffix in ["cpu", "mem"] {
                        let topic = format!(
                            "{discovery_prefix}/sensor/{node}_ct_{ckey}_{suffix}/config"
                        );
                        // Empty retained payload removes the entity from HA.
                        let _ = client.publish(&topic, QoS::AtLeastOnce, true, "");
                    }
                    known_containers.remove(&ckey);
                }
            }
        }

        let _ = client.publish(&state_topic, QoS::AtLeastOnce, false, payload.to_string());
        let _ = client.publish(&avail_topic, QoS::AtLeastOnce, true, "online");

        std::thread::sleep(Duration::from_secs(interval));
    }
}
