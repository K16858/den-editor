#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$BinaryName  = "den"
$InstallDir  = Join-Path $env:USERPROFILE ".local\bin"
$ConfigDir   = Join-Path $env:APPDATA $BinaryName
$ScriptDir   = $PSScriptRoot
$DefaultCfg  = Join-Path $ScriptDir "docs\examples\default"

# ── 前提確認 ────────────────────────────────────────────────────────────────

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo が見つかりません。https://rustup.rs でインストールしてください。"
    exit 1
}

# ── ビルド ───────────────────────────────────────────────────────────────────

Write-Host "Building $BinaryName..."
cargo build --release --manifest-path (Join-Path $ScriptDir "Cargo.toml")

# ── バイナリのインストール ───────────────────────────────────────────────────

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$BinaryPath = Join-Path $InstallDir "$BinaryName.exe"
Copy-Item (Join-Path $ScriptDir "target\release\$BinaryName.exe") $BinaryPath -Force
Write-Host "Installed: $BinaryPath"

# ── PATH への追記 ────────────────────────────────────────────────────────────

$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable(
        "Path",
        "$InstallDir;$currentPath",
        "User"
    )
    Write-Host "PATH に追記しました: $InstallDir"
    Write-Host "※ 新しいターミナルを開くと有効になります。"
} else {
    Write-Host "PATH は既に設定済みです。"
}

# ── 設定ファイルの配置 ───────────────────────────────────────────────────────

New-Item -ItemType Directory -Force -Path (Join-Path $ConfigDir "languages") | Out-Null

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

# ── 完了 ─────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "Installation complete!"
Write-Host "  Binary : $BinaryPath"
Write-Host "  Config : $ConfigDir"
