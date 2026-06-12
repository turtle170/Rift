# install.ps1 - Automated installer for Rift

$ErrorActionPreference = "Stop"

Write-Host "Installing Rift Desktop Pet..." -ForegroundColor Cyan

# 1. Fetch latest release info
$repo = "turtle170/Rift"
$apiUrl = "https://api.github.com/repos/$repo/releases/latest"

Write-Host "Fetching latest release info from GitHub..."
try {
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "rift-installer" }
} catch {
    Write-Error "Failed to fetch latest release from GitHub. Ensure you have an internet connection and GitHub is accessible."
    exit 1
}

$version = $release.tag_name
$asset = $release.assets | Where-Object { $_.name -eq "rift.exe" }

if (-not $asset) {
    Write-Error "Could not find rift.exe in release $version."
    exit 1
}

# 2. Setup Directories
$localAppData = [System.Environment]::GetFolderPath('LocalApplicationData')
$installDir = Join-Path $localAppData "rift\bin"

if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
}

$exePath = Join-Path $installDir "rift.exe"

# 3. Download the executable
Write-Host "Downloading Rift $version..."
try {
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $exePath -Headers @{ "User-Agent" = "rift-installer" }
} catch {
    Write-Error "Failed to download rift.exe."
    exit 1
}

Write-Host "Downloaded successfully to $exePath" -ForegroundColor Green

# 4. Add to PATH if missing
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$sysPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")

if (($userPath -split ';') -notcontains $installDir -and ($sysPath -split ';') -notcontains $installDir) {
    Write-Host "Adding $installDir to your User PATH..."
    $newPath = "$userPath;$installDir"
    [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
    
    # Update current session PATH so it works immediately in this window
    $env:PATH = "$env:PATH;$installDir"
    Write-Host "PATH updated!" -ForegroundColor Green
}

Write-Host "`nInstallation Complete! 🎉" -ForegroundColor Green
Write-Host "Run the following command to hatch your pet:" -ForegroundColor Cyan
Write-Host "  rift hatch" -ForegroundColor White
Write-Host "`n(Note: If 'rift' is not recognized, please restart your terminal or open a new one)" -ForegroundColor DarkGray
