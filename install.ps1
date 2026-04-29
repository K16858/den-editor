#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$BinaryName  = "den"
$InstallDir  = Join-Path $env:USERPROFILE ".local\bin"
$ConfigDir   = Join-Path $env:APPDATA $BinaryName
$ScriptDir   = $PSScriptRoot
$DefaultCfg  = Join-Path $ScriptDir "docs\examples\default"

# ── Prerequisites ────────────────────────────────────────────────────────────

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo not found. Install it from https://rustup.rs"
    exit 1
}

# ── Build ────────────────────────────────────────────────────────────────────

Write-Host "Building $BinaryName..."
cargo build --release --manifest-path (Join-Path $ScriptDir "Cargo.toml")

# ── Install binary ───────────────────────────────────────────────────────────

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$BinaryPath = Join-Path $InstallDir "$BinaryName.exe"
Copy-Item (Join-Path $ScriptDir "target\release\$BinaryName.exe") $BinaryPath -Force
Write-Host "Installed: $BinaryPath"

# ── Update PATH ──────────────────────────────────────────────────────────────

$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable(
        "Path",
        "$InstallDir;$currentPath",
        "User"
    )
    Write-Host "Added to PATH: $InstallDir"
    Write-Host "Restart your terminal for the change to take effect."
} else {
    Write-Host "PATH already contains: $InstallDir"
}

# ── Install config files ─────────────────────────────────────────────────────

New-Item -ItemType Directory -Force -Path (Join-Path $ConfigDir "languages") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $ConfigDir "debuggers") | Out-Null

function Copy-IfMissing {
    param([string]$Src, [string]$Dst)
    if (-not (Test-Path $Dst)) {
        Copy-Item $Src $Dst
        Write-Host "Created: $Dst"
    }
}

Copy-IfMissing (Join-Path $DefaultCfg "colors.toml") (Join-Path $ConfigDir "colors.toml")

Get-ChildItem (Join-Path $DefaultCfg "languages\*.toml") | ForEach-Object {
    Copy-IfMissing $_.FullName (Join-Path $ConfigDir "languages\$($_.Name)")
}

Get-ChildItem (Join-Path $DefaultCfg "debuggers\*.toml") | ForEach-Object {
    Copy-IfMissing $_.FullName (Join-Path $ConfigDir "debuggers\$($_.Name)")
}

# ── Done ─────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "Installation complete!"
Write-Host "  Binary : $BinaryPath"
Write-Host "  Config : $ConfigDir"
