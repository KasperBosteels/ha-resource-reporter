# resource-reporter Windows installer
# Installs the agent and registers it to run at startup as a hidden background task.
#
# Usage (in PowerShell, from the folder containing this script + the exe):
#   powershell -ExecutionPolicy Bypass -File install-windows.ps1 -MqttHost <broker-host> -MqttPass <password> -NodeName laptop
#
# Notes:
#   - On Windows the MQTT broker hostname must be one the laptop can resolve.
#     Use a resolvable name/IP for your broker (e.g. its Tailscale MagicDNS name
#     like homeassistant.<your-tailnet>.ts.net, or a plain LAN IP). A bare
#     single-label name like "homeassistant" often won't resolve on Windows.

param(
    [string]$MqttHost = "homeassistant",
    [string]$MqttPort = "1883",
    [string]$MqttUser = "mqtt",
    [string]$MqttPass = "mqtt",
    [string]$NodeName = $env:COMPUTERNAME,
    [string]$Interval = "60"
)

$ErrorActionPreference = "Stop"
$AppName    = "resource-reporter"
$InstallDir = Join-Path $env:LOCALAPPDATA $AppName
$ExeSource  = Join-Path $PSScriptRoot "resource-reporter.exe"
$ExeDest    = Join-Path $InstallDir "resource-reporter.exe"
$ConfDest   = Join-Path $InstallDir "resource-reporter.conf"
$TaskName   = "ResourceReporter"

Write-Host "Installing $AppName ..." -ForegroundColor Cyan

# 1. Create install dir + copy exe
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
if (-not (Test-Path $ExeSource)) {
    Write-Error "resource-reporter.exe not found next to this script. Keep them together."
    exit 1
}
Copy-Item -Force $ExeSource $ExeDest

# 2. Write the config file the exe reads on startup (sits next to the exe)
@"
MQTT_HOST=$MqttHost
MQTT_PORT=$MqttPort
MQTT_USER=$MqttUser
MQTT_PASS=$MqttPass
NODE_NAME=$NodeName
REPORT_INTERVAL=$Interval
"@ | Set-Content -Encoding ASCII $ConfDest

# 3. Register a Scheduled Task that runs the EXE DIRECTLY (no launcher wrapper).
#    Running the exe directly is what keeps Windows from reaping it. The release
#    build is compiled windowless, so no console flashes.
$Action = New-ScheduledTaskAction -Execute $ExeDest -WorkingDirectory $InstallDir
$Trigger = New-ScheduledTaskTrigger -AtLogOn
# Keep it alive: restart if it ever exits, run indefinitely, don't auto-stop.
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries -StartWhenAvailable `
    -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit (New-TimeSpan -Seconds 0)
$Principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited

Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
Register-ScheduledTask -TaskName $TaskName -Action $Action -Trigger $Trigger `
    -Settings $Settings -Principal $Principal | Out-Null

# 4. Start it now
Start-ScheduledTask -TaskName $TaskName

Write-Host ""
Write-Host "Installed and started." -ForegroundColor Green
Write-Host "  Node name : $NodeName"
Write-Host "  Broker    : $MqttHost`:$MqttPort"
Write-Host "  Task      : $TaskName (runs at every logon, hidden, auto-restarts)"
Write-Host ""
Write-Host "It should appear in Home Assistant as device '$NodeName' within ~1 min."
Write-Host "To uninstall: run uninstall-windows.ps1"
