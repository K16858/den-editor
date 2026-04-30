#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Repo = "K16858/redit"
$BinaryName = "den"
$AssetName = "den-windows-x86_64.exe"
$InstallDir = Join-Path $env:USERPROFILE ".local\bin"
$ConfigDir = Join-Path $env:APPDATA $BinaryName
$ApiLatest = "https://api.github.com/repos/$Repo/releases/latest"

$release = Invoke-RestMethod -Uri $ApiLatest
$tag = $release.tag_name
if ([string]::IsNullOrWhiteSpace($tag)) {
    throw "Failed to resolve latest release tag."
}

$binUrl = "https://github.com/$Repo/releases/download/$tag/$AssetName"
$checksumUrl = "https://github.com/$Repo/releases/download/$tag/sha256sums.txt"
$rawBase = "https://raw.githubusercontent.com/$Repo/$tag/docs/examples/default"

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("den-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

try {
    $tmpBin = Join-Path $tmpDir $AssetName
    $tmpChecksum = Join-Path $tmpDir "sha256sums.txt"

    Write-Host "Downloading $AssetName ($tag)..."
    Invoke-WebRequest -Uri $binUrl -OutFile $tmpBin
    Invoke-WebRequest -Uri $checksumUrl -OutFile $tmpChecksum

    $expectedLine = Select-String -Path $tmpChecksum -Pattern (" " + [regex]::Escape($AssetName) + "$") | Select-Object -First 1
    if (-not $expectedLine) {
        throw "Checksum entry not found for $AssetName."
    }
    $expected = ($expectedLine.Line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $tmpBin).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
        throw "Checksum mismatch for $AssetName."
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $binaryPath = Join-Path $InstallDir "$BinaryName.exe"
    Copy-Item $tmpBin $binaryPath -Force

    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($null -eq $currentPath) { $currentPath = "" }
    if ($currentPath -notlike "*$InstallDir*") {
        $newPath = if ([string]::IsNullOrWhiteSpace($currentPath)) { $InstallDir } else { "$InstallDir;$currentPath" }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "Added to PATH: $InstallDir"
        Write-Host "Restart your terminal for the change to take effect."
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $ConfigDir "languages") | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $ConfigDir "debuggers") | Out-Null

    function Download-IfMissing {
        param([string]$Url, [string]$Dst)
        if (-not (Test-Path $Dst)) {
            Invoke-WebRequest -Uri $Url -OutFile $Dst
            Write-Host "Created: $Dst"
        }
    }

    Download-IfMissing "$rawBase/colors.toml" (Join-Path $ConfigDir "colors.toml")
    foreach ($name in @("c", "go", "javascript", "markdown", "python", "rust")) {
        Download-IfMissing "$rawBase/languages/$name.toml" (Join-Path $ConfigDir "languages\$name.toml")
    }
    foreach ($name in @("go", "python", "rust")) {
        Download-IfMissing "$rawBase/debuggers/$name.toml" (Join-Path $ConfigDir "debuggers\$name.toml")
    }

    Write-Host ""
    Write-Host "Installation complete!"
    Write-Host "  Version: $tag"
    Write-Host "  Binary : $binaryPath"
    Write-Host "  Config : $ConfigDir"
}
finally {
    if (Test-Path $tmpDir) {
        Remove-Item -Path $tmpDir -Recurse -Force
    }
}
