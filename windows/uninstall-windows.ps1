# resource-reporter Windows uninstaller
# Stops and removes the scheduled task and installed files.
$ErrorActionPreference = "SilentlyContinue"
$AppName = "resource-reporter"
$InstallDir = Join-Path $env:LOCALAPPDATA $AppName
$TaskName = "ResourceReporter"

Write-Host "Uninstalling $AppName ..." -ForegroundColor Cyan

Stop-ScheduledTask -TaskName $TaskName
Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
Get-Process resource-reporter -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-Item -Recurse -Force $InstallDir

Write-Host "Removed task, process, and files." -ForegroundColor Green
Write-Host "Note: the 'laptop' device may linger in Home Assistant."
Write-Host "Delete it via Settings > Devices & Services > MQTT if you want it gone."
