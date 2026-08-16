# resource-reporter

A tiny cross-platform Rust agent that samples host resources (CPU, RAM, disk,
temperature, uptime, load) and publishes them to **Home Assistant** over MQTT
using [MQTT discovery](https://www.home-assistant.io/integrations/mqtt/#mqtt-discovery),
so each machine shows up automatically as a device with its own sensors.

## How it works

- Every `REPORT_INTERVAL` seconds it samples the host with [`sysinfo`](https://crates.io/crates/sysinfo).
- On startup it publishes retained MQTT **discovery configs** so Home Assistant
  creates the entities without any YAML.
- State is published to `resource-reporter/<node>/state` as JSON.
- An MQTT **Last Will** marks the device `offline` if the process dies, so HA
  shows availability correctly.
- Sensors that a platform can't provide are skipped automatically
  (e.g. no load-average on Windows; CPU temp only if the OS exposes it).

## Configuration (environment variables)

| Var               | Meaning                                  |
|-------------------|------------------------------------------|
| `MQTT_HOST`       | MQTT broker hostname                     |
| `MQTT_PORT`       | MQTT broker port                         |
| `MQTT_USER`       | MQTT username                            |
| `MQTT_PASS`       | MQTT password                            |
| `NODE_NAME`       | Device name shown in Home Assistant      |
| `REPORT_INTERVAL` | Seconds between samples                  |
| `DISCOVERY_PREFIX`| HA MQTT discovery prefix                 |

## Build

```sh
cargo build --release
```

Cross-compile for the Raspberry Pi (aarch64) — build natively on the Pi, or in a
container on an arm64 host:

```sh
docker run --rm -v "$PWD":/build -w /build rust:1-slim cargo build --release
```

Cross-compile for Windows from Linux:

```sh
docker run --rm --platform linux/amd64 -v "$PWD":/build -w /build rust:1-slim sh -c \
  'apt-get update && apt-get install -y mingw-w64 && \
   rustup target add x86_64-pc-windows-gnu && \
   cargo build --release --target x86_64-pc-windows-gnu'
```

## Deploy

### Linux (systemd user service)

```sh
install -m755 target/release/resource-reporter ~/.local/bin/resource-reporter
mkdir -p ~/.config/resource-reporter
cat > ~/.config/resource-reporter/env <<EOF
MQTT_HOST=homeassistant
MQTT_USER=mqtt
MQTT_PASS=yourpassword
NODE_NAME=my-server
EOF
chmod 600 ~/.config/resource-reporter/env
cp systemd/resource-reporter.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now resource-reporter
loginctl enable-linger "$USER"   # keep running after logout / across reboots
```

### Windows (scheduled task at logon)

Copy the `windows/` folder plus `resource-reporter.exe` to the laptop, then:

```powershell
powershell -ExecutionPolicy Bypass -File install-windows.ps1 -MqttPass yourpassword -NodeName laptop
```

Uninstall with `uninstall-windows.ps1`.
