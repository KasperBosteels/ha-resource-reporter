# AGENTS.md — resource-reporter

Context for AI agents (and humans) working in this repo. Keep it current: if you
change the architecture, build, or conventions, update this file in the same change.

## What this is

A tiny cross-platform **Rust** agent that samples host resources (CPU, RAM, swap,
disk, temperature, uptime, network, load) plus per-container Docker stats and
publishes them to **Home Assistant** over **MQTT** using [MQTT discovery](https://www.home-assistant.io/integrations/mqtt/#mqtt-discovery).
Each machine appears in HA automatically as a device with its own sensors — no
YAML editing in HA.

Single binary, no runtime deps beyond an MQTT broker. Runs as a systemd user
service on Linux and a scheduled task on Windows.

## Architecture (all in `src/main.rs`, ~370 lines, no modules)

Execution flow:
1. `load_config_file()` — reads `KEY=VALUE` lines from `resource-reporter.conf`
   next to the exe (Windows path). Does not override already-set env vars, so the
   systemd `EnvironmentFile` still wins on Linux.
2. Read config from env (`env_or` helper) — see table below.
3. Connect MQTT (`rumqttc`), set a retained **Last Will** on the availability
   topic so HA marks the device `offline` if the process dies.
4. **Capability probe** — decide which optional sensors exist on this platform:
   - `have_temp`: any temperature component present
   - `have_load`: false on Windows (no load average)
   - `have_docker`: `docker stats` runs successfully
5. Publish **retained discovery configs** for every sensor via `publish_sensor`.
6. **Sampling loop** every `REPORT_INTERVAL` seconds: sample host + Docker, build
   one JSON payload, publish to the state topic, re-assert `online`.

Key building blocks:
- `publish_sensor(key, name, unit, device_class, icon)` — publishes one retained
  discovery config. This is the single place sensor entities are defined.
- `docker_stats()` -> `Option<Vec<ContainerStat>>` — shells out to
  `docker stats --no-stream --format '{{.Name}};{{.CPUPerc}};{{.MemUsage}}'`.
  Returns `None` if docker isn't usable (that's how `have_docker` is decided).
- `parse_mem_mb(s)` — parses docker mem strings like `"67.07MiB / 15.48GiB"` to MB.
- `sanitize(s)` — lowercase alnum, everything else -> `_`. Used for MQTT topic
  and entity id fragments. **All node/container names must pass through this.**
- Per-container sensors are published dynamically as containers appear, and their
  retained discovery configs are cleared (empty payload) when a container vanishes.

## MQTT topic / entity layout

- State:        `resource-reporter/<node>/state`   (JSON, all metrics)
- Availability: `resource-reporter/<node>/availability`  (`online`/`offline`, retained)
- Discovery:    `<DISCOVERY_PREFIX>/sensor/<node>_<key>/config`  (retained)
- Entity id pattern: `sensor.<node>_<key>`, containers `sensor.<node>_ct_<container>_<cpu|mem>`

`<node>` is `sanitize(NODE_NAME or hostname)`.

## Configuration (environment variables)

| Var                | Default         | Meaning                             |
|--------------------|-----------------|-------------------------------------|
| `MQTT_HOST`        | `homeassistant` | MQTT broker hostname                |
| `MQTT_PORT`        | `1883`          | MQTT broker port                    |
| `MQTT_USER`        | `mqtt`          | MQTT username                       |
| `MQTT_PASS`        | (empty)         | MQTT password                       |
| `NODE_NAME`        | system hostname | Device name shown in HA             |
| `REPORT_INTERVAL`  | `60`            | Seconds between samples             |
| `DISCOVERY_PREFIX` | `homeassistant` | HA MQTT discovery prefix            |

## Build / test / run

```sh
cargo build --release          # release binary -> target/release/resource-reporter
cargo build                    # debug
cargo clippy --all-targets     # lint (fix warnings before committing)
cargo fmt                      # format (run before committing)
cargo test                     # (no tests yet — see "Testing" below)
```

Cross-compile for the Raspberry Pi (aarch64) — build natively on the Pi, or:
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

Run locally against a broker:
```sh
MQTT_HOST=homeassistant MQTT_USER=mqtt MQTT_PASS=... NODE_NAME=test-box \
  cargo run --release
```

## Deploy

- **Linux**: user systemd service in `systemd/resource-reporter.service`, config in
  `~/.config/resource-reporter/env` (chmod 600), `loginctl enable-linger` to survive
  logout/reboot.
- **Windows**: `windows/install-windows.ps1` registers a scheduled task at logon.
  The task must run the exe **directly** (not detached) or Windows reaps it.
  `windows/uninstall-windows.ps1` removes it.

## Conventions

- Everything lives in one file by design — keep it that way unless a change is big
  enough to genuinely warrant a module. Prefer small helper fns over new files.
- Failures are **soft**: a metric that can't be read is skipped, never panics the
  loop. Preserve this — wrap fallible platform calls, default sensibly.
- Any string used in a topic or entity id goes through `sanitize()`.
- New sensors: add a `publish_sensor(...)` call in the discovery section AND a
  matching field in the `payload` json in the loop — the `key` must match on both
  sides (the discovery `value_template` reads `value_json.<key>`).
- `panic = "abort"` and aggressive size opts are set in `[profile.release]`; don't
  rely on unwinding.
- Do not hardcode hostnames, tailnet names, credentials, or personal paths — this
  repo is public. Config comes from env / conf file only.

## Testing

There are no automated tests yet. The pure helpers are the low-hanging fruit and
the right place to start for any TDD/SDD work:
- `parse_mem_mb` — unit conversions and the `"x / y"` split (test B/KiB/MiB/GiB/TiB
  and malformed input -> `None`).
- `sanitize` — alnum passthrough, lowercasing, separator replacement.
- `env_or` — default vs override.

Put unit tests in a `#[cfg(test)] mod tests` block at the bottom of `main.rs`
(single-file crate). Run with `cargo test`. When adding tests, verify RED before
GREEN per the test-driven-development skill.

## SDD notes (for subagent-driven-development runs)

- This file IS the project context — provide task specifics in the subagent's
  `context`, but you don't need to re-explain the architecture; point at this file.
- Tasks touching `src/main.rs` are serial (single file) — never dispatch two
  implementer subagents editing it in parallel.
- Verification gate for any change: `cargo fmt --check && cargo clippy --all-targets
  && cargo build --release` must be clean, plus `cargo test` once tests exist.
- CI runs `.github/workflows/rust.yml` on a self-hosted runner.

## Repo map

```
src/main.rs        all program logic
Cargo.toml         crate manifest + release profile
dashboard.yaml     example Home Assistant dashboard for these sensors
systemd/           Linux user service unit
windows/           Windows install/uninstall PowerShell scripts
.github/workflows/ CI (rust.yml, self-hosted runner)
dist/              prebuilt artifacts (gitignored)
```
