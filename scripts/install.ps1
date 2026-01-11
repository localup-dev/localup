# Localup - Windows Installation Script
# Download and install latest release for Windows
# Usage: irm https://raw.githubusercontent.com/localup-dev/localup/main/scripts/install.ps1 | iex

param(
    [string]$InstallPath = "$env:LocalAppData\localup"
)

# Enable error handling
$ErrorActionPreference = "Stop"

# Colors
$Green = [Console]::ForegroundColor = "Green"
$Yellow = [Console]::ForegroundColor = "Yellow"
$Red = [Console]::ForegroundColor = "Red"
$Blue = [Console]::ForegroundColor = "Blue"
$Default = [Console]::ResetColor

# Detect architecture
function Get-Architecture {
    $arch = [Environment]::ProcessorCount
    $osArch = [Environment]::Is64BitOperatingSystem

    if ([System.Environment]::Is64BitOperatingSystem) {
        return "amd64"
    } elseif ([Environment]::ProcessorCount -match "ARM64") {
        return "arm64"
    } else {
        return "amd64"  # Default
    }
}

Write-Host ""
Write-Host "═══════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "   Localup - Download Latest Release" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

# Detect platform and architecture
$Platform = "windows"
$Arch = Get-Architecture
$Ext = "zip"

Write-Host "📋 Detected platform:" -ForegroundColor Yellow
Write-Host "  OS: $Platform"
Write-Host "  Architecture: $Arch"
Write-Host ""

# Get latest release version
Write-Host "🔍 Fetching latest release..." -ForegroundColor Yellow

try {
    # Try to get from GitHub API (includes pre-releases)
    $releaseJson = Invoke-RestMethod -Uri "https://api.github.com/repos/localup-dev/localup/releases?per_page=1" -ErrorAction SilentlyContinue
    $LatestVersion = $releaseJson[0].tag_name

    if ([string]::IsNullOrEmpty($LatestVersion) -or $LatestVersion -eq "null") {
        Write-Host "❌ Could not fetch latest release version" -ForegroundColor Red
        Write-Host "ℹ️  GitHub API error or rate limit reached" -ForegroundColor Yellow
        Write-Host "Try again later or download manually from:" -ForegroundColor Yellow
        Write-Host "  https://github.com/localup-dev/localup/releases" -ForegroundColor Blue
        exit 1
    }

    # Validate version format
    if (-not ($LatestVersion -match '^v[0-9]')) {
        Write-Host "❌ Invalid version format: $LatestVersion" -ForegroundColor Red
        Write-Host "Expected format like: v0.0.1, v1.0.0, etc." -ForegroundColor Yellow
        exit 1
    }
} catch {
    Write-Host "❌ Error fetching release: $_" -ForegroundColor Red
    exit 1
}

Write-Host "  Latest version: $LatestVersion" -ForegroundColor Green
Write-Host ""

# Construct download URLs
$TunnelFile = "localup-${Platform}-${Arch}.${Ext}"
$RelayFile = "localup-relay-${Platform}-${Arch}.${Ext}"
$AgentFile = "localup-agent-server-${Platform}-${Arch}.${Ext}"
$ChecksumsFile = "checksums-${Platform}-${Arch}.txt"

$BaseUrl = "https://github.com/localup-dev/localup/releases/download/${LatestVersion}"

$TunnelUrl = "${BaseUrl}/${TunnelFile}"
$RelayUrl = "${BaseUrl}/${RelayFile}"
$AgentUrl = "${BaseUrl}/${AgentFile}"
$ChecksumsUrl = "${BaseUrl}/${ChecksumsFile}"

# Create download directory
$DownloadDir = "localup-${LatestVersion}"
if (Test-Path $DownloadDir) {
    Remove-Item -Path $DownloadDir -Recurse -Force
}
$null = New-Item -ItemType Directory -Force -Path $DownloadDir
Set-Location $DownloadDir

Write-Host "📥 Downloading binaries..." -ForegroundColor Yellow

# Download tunnel CLI
Write-Host "  Downloading tunnel CLI..."
try {
    Invoke-WebRequest -Uri $TunnelUrl -OutFile $TunnelFile -UseBasicParsing
    Write-Host "  ✓ Downloaded $TunnelFile" -ForegroundColor Green
} catch {
    Write-Host "  ✗ Failed to download $TunnelFile" -ForegroundColor Red
    Write-Host "  Error: $_" -ForegroundColor Red
    exit 1
}

# Download relay server
Write-Host "  Downloading relay server..."
try {
    Invoke-WebRequest -Uri $RelayUrl -OutFile $RelayFile -UseBasicParsing
    Write-Host "  ✓ Downloaded $RelayFile" -ForegroundColor Green
} catch {
    Write-Host "  ✗ Failed to download $RelayFile" -ForegroundColor Red
    Write-Host "  Error: $_" -ForegroundColor Red
    exit 1
}

# Download agent server
Write-Host "  Downloading agent server..."
try {
    Invoke-WebRequest -Uri $AgentUrl -OutFile $AgentFile -UseBasicParsing
    Write-Host "  ✓ Downloaded $AgentFile" -ForegroundColor Green
} catch {
    Write-Host "  ⚠️  Agent server may not be available in this release" -ForegroundColor Yellow
}

# Download checksums
Write-Host "  Downloading checksums..."
try {
    Invoke-WebRequest -Uri $ChecksumsUrl -OutFile $ChecksumsFile -UseBasicParsing
    Write-Host "  ✓ Downloaded $ChecksumsFile" -ForegroundColor Green
} catch {
    Write-Host "  ⚠️  Checksums file not available" -ForegroundColor Yellow
}

Write-Host ""

# Extract archives
Write-Host "📦 Extracting archives..." -ForegroundColor Yellow

if (Test-Path $TunnelFile) {
    Expand-Archive -Path $TunnelFile -DestinationPath . -Force
}

if (Test-Path $RelayFile) {
    Expand-Archive -Path $RelayFile -DestinationPath . -Force
}

if (Test-Path $AgentFile) {
    Expand-Archive -Path $AgentFile -DestinationPath . -Force
}

Write-Host "  ✓ Extracted binaries" -ForegroundColor Green
Write-Host ""

# Verify binaries
Write-Host "✅ Download complete!" -ForegroundColor Green
Write-Host ""

$BinariesPath = (Get-Location).Path
Write-Host "📂 Files extracted to:" -ForegroundColor Yellow
Write-Host "  $BinariesPath"
Write-Host ""

Write-Host "📋 Next steps:" -ForegroundColor Yellow
Write-Host ""
Write-Host "  Create destination directory:" -ForegroundColor Blue
Write-Host "  mkdir ""$InstallPath"" -Force"
Write-Host ""
Write-Host "  Copy binaries:" -ForegroundColor Blue
Write-Host "  Copy-Item localup.exe, localup-relay.exe -Destination ""$InstallPath"""
if (Test-Path "localup-agent-server.exe") {
    Write-Host "  Copy-Item localup-agent-server.exe -Destination ""$InstallPath"""
}
Write-Host ""
Write-Host "  Add to PATH permanently:" -ForegroundColor Blue
Write-Host "  `$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')"
Write-Host "  if (`$userPath -notcontains '$InstallPath') {"
Write-Host "    [Environment]::SetEnvironmentVariable('Path', \""$userPath;$InstallPath"", 'User')"
Write-Host "  }"
Write-Host ""
Write-Host "  Unblock executables (if needed):" -ForegroundColor Blue
Write-Host "  Unblock-File -Path ""$InstallPath\localup.exe"""
Write-Host "  Unblock-File -Path ""$InstallPath\localup-relay.exe"""
if (Test-Path "localup-agent-server.exe") {
    Write-Host "  Unblock-File -Path ""$InstallPath\localup-agent-server.exe"""
}
Write-Host ""
Write-Host "  Restart PowerShell and verify:" -ForegroundColor Blue
Write-Host "  localup --version"
Write-Host "  localup-relay --version"
if (Test-Path "localup-agent-server.exe") {
    Write-Host "  localup-agent-server --version"
}
Write-Host ""
Write-Host "🚀 Quick start:" -ForegroundColor Yellow
Write-Host "  # Start relay server" -ForegroundColor Blue
Write-Host "  localup-relay"
Write-Host ""
Write-Host "  # Create tunnel (in another terminal)" -ForegroundColor Blue
Write-Host "  localup http --port 3000 --relay localhost:4443"
Write-Host ""
