$ErrorActionPreference = "Stop"

$Repo = "MaNiSh-9211/envy"
$Version = if ($env:ENVY_VERSION) { $env:ENVY_VERSION } else { "latest" }

$cpuMap = @{ "AMD64" = "amd64"; "ARM64" = "arm64" }
$cpu = $cpuMap[$env:PROCESSOR_ARCHITECTURE]
if (-not $cpu) {
    Write-Error "envy: unsupported architecture '$($env:PROCESSOR_ARCHITECTURE)'"
}

$asset = "envy-windows-$cpu.exe"
$segment = if ($Version -eq "latest") { "latest/download" } else { "download/$Version" }
$url = "https://github.com/$Repo/releases/$segment/$asset"

$destDir = if ($env:ENVY_INSTALL_DIR) { $env:ENVY_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\envy" }
New-Item -ItemType Directory -Path $destDir -Force | Out-Null
$dest = Join-Path $destDir "envy.exe"

Write-Host "envy: installing $asset -> $dest"
Invoke-WebRequest -Uri $url -OutFile $dest -UserAgent "envy-installer"

Write-Host "envy installed."
if (($env:Path -split ";") -notcontains $destDir) {
    Write-Host "note: add $destDir to your PATH to use 'envy' anywhere:"
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', `$env:Path + ';$destDir', 'User')"
}
