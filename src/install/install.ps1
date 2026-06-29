param(
    [string]$Version = $(if ($env:REPOGRAMMAR_VERSION) { $env:REPOGRAMMAR_VERSION } else { "latest" }),
    [string]$Repo = $(if ($env:REPOGRAMMAR_REPO) { $env:REPOGRAMMAR_REPO } else { "SioYooo/RepoGrammar" }),
    [string]$CommandDir = $(if ($env:REPOGRAMMAR_COMMAND_DIR) { $env:REPOGRAMMAR_COMMAND_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\RepoGrammar\bin" }),
    [string]$InstallDir = $(if ($env:REPOGRAMMAR_INSTALL_DIR) { $env:REPOGRAMMAR_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "RepoGrammar" }),
    [string]$WorkerRoot = $(if ($env:REPOGRAMMAR_WORKER_ROOT) { $env:REPOGRAMMAR_WORKER_ROOT } else { Join-Path $InstallDir "workers" }),
    [string]$SourceBinary = $(if ($env:REPOGRAMMAR_SOURCE_BINARY) { $env:REPOGRAMMAR_SOURCE_BINARY } else { "" }),
    [string]$Target = "all",
    [string]$Scope = "global",
    [switch]$FromSource,
    [switch]$InstallCliOnly,
    [switch]$InstallAndConfigure,
    [switch]$UninstallCommand,
    [switch]$Yes,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$PreviewVersionHint = $(if ($env:REPOGRAMMAR_PREVIEW_VERSION_HINT) { $env:REPOGRAMMAR_PREVIEW_VERSION_HINT } else { "v0.2.0-preview.0" })
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $ScriptDir "..\.."))
$UseSource = [bool]$FromSource

function Show-Usage {
    Write-Output @"
RepoGrammar Windows installer preview

Usage:
  powershell -ExecutionPolicy Bypass -File install.ps1
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallCliOnly
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallCliOnly -FromSource -Yes
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallAndConfigure
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallAndConfigure -FromSource -Yes -Target all
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallAndConfigure -Target "codex,claude-code" -Scope global
  powershell -ExecutionPolicy Bypass -File install.ps1 -UninstallCommand -Yes

By default, the script downloads a prebuilt Windows x64 release artifact,
verifies its checksum, installs repogrammar.exe into a user-writable command
directory, and can launch repogrammar install for Codex / Claude Code MCP
wiring. In a source checkout, use -FromSource to build or copy a local
target\release\repogrammar.exe before running agent setup.

For local artifact tests, set REPOGRAMMAR_RELEASE_DIR to a directory containing
the Windows zip and matching .sha256 file. For source dogfood tests,
REPOGRAMMAR_SOURCE_BINARY or -SourceBinary may point at an already built
repogrammar.exe and skips the default cargo build.
"@
}

function Test-SourceCheckout {
    return (Test-Path (Join-Path $RepoRoot "Cargo.toml")) -and
        (Test-Path (Join-Path $RepoRoot "src\rust"))
}

function Confirm-DefaultNo([string]$Prompt) {
    if ($Yes) {
        return $true
    }
    $reply = Read-Host "$Prompt [y/N]"
    return $reply -match '^(?i)y(es)?$'
}

function Get-ReleaseBase {
    if ($env:REPOGRAMMAR_RELEASE_BASE) {
        return $env:REPOGRAMMAR_RELEASE_BASE.TrimEnd("/")
    }
    if ($Version -eq "latest") {
        return "https://github.com/$Repo/releases/latest/download"
    }
    return "https://github.com/$Repo/releases/download/$Version"
}

function Copy-ReleaseAsset([string]$Name, [string]$Destination) {
    if ($env:REPOGRAMMAR_RELEASE_DIR) {
        $localAsset = Join-Path $env:REPOGRAMMAR_RELEASE_DIR $Name
        if (!(Test-Path $localAsset)) {
            throw "release artifact not found in REPOGRAMMAR_RELEASE_DIR: $Name"
        }
        Copy-Item $localAsset $Destination
        return
    }
    $base = Get-ReleaseBase
    $url = "$base/$Name"
    try {
        Invoke-WebRequest -Uri $url -OutFile $Destination -ErrorAction Stop
    } catch {
        throw "release artifact was not found: $url. For preview prereleases, rerun with -Version <preview-tag> (for example: -Version $PreviewVersionHint). For local artifact testing, set REPOGRAMMAR_RELEASE_DIR to a directory containing the zip and .sha256 file."
    }
}

function Normalize-ArchiveEntry([string]$Entry) {
    $normalized = $Entry.Trim()
    if ($normalized.StartsWith("./")) {
        $normalized = $normalized.Substring(2)
    }
    while ($normalized.EndsWith("/")) {
        $normalized = $normalized.Substring(0, $normalized.Length - 1)
    }
    return $normalized
}

function Assert-SafeArchiveEntries([string]$Archive) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $allowed = @(
        "repogrammar.exe",
        "workers",
        "workers/python",
        "workers/python/worker.py"
    )
    $hasBinary = $false
    $hasWorker = $false
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        foreach ($entry in $zip.Entries) {
            $name = Normalize-ArchiveEntry ($entry.FullName)
            if ([string]::IsNullOrWhiteSpace($name) -or
                $name.StartsWith("/") -or
                $name.Contains("\") -or
                $name.Contains("://") -or
                $name -match "^[A-Za-z]:" -or
                ($name -split "/") -contains "." -or
                ($name -split "/") -contains ".." -or
                !($allowed -contains $name)) {
                throw "release artifact contains unsafe or unexpected path: $($entry.FullName)"
            }
            if ($name -eq "repogrammar.exe") {
                $hasBinary = $true
            }
            if ($name -eq "workers/python/worker.py") {
                $hasWorker = $true
            }
        }
    } finally {
        $zip.Dispose()
    }
    if (!$hasBinary) {
        throw "release artifact did not contain repogrammar.exe"
    }
    if (!$hasWorker) {
        throw "release artifact did not contain bundled Python worker at workers/python/worker.py"
    }
}

function Install-WorkerAsset([string]$WorkerSource) {
    if (!(Test-Path $WorkerSource)) {
        throw "source checkout did not contain bundled Python worker at src/workers/python/worker.py"
    }
    $workerRoots = @($WorkerRoot)
    if (!$env:REPOGRAMMAR_WORKER_ROOT) {
        $workerRoots += (Join-Path $CommandDir "repogrammar-workers")
    }
    foreach ($root in ($workerRoots | Select-Object -Unique)) {
        $workerDest = Join-Path $root "python"
        New-Item -ItemType Directory -Force -Path $workerDest | Out-Null
        Copy-Item $WorkerSource (Join-Path $workerDest "worker.py") -Force
    }
}

function Backup-UnmanagedCommand([string]$CommandPath) {
    if (!(Test-Path $CommandPath)) {
        return
    }
    $item = Get-Item $CommandPath
    if ($item.PSIsContainer) {
        throw "repogrammar command path is a directory and cannot be replaced automatically; choose a different CommandDir"
    }
    $backup = "$CommandPath.unmanaged-backup"
    if (Test-Path $backup) {
        $backup = "$backup.$((Get-Date).ToString('yyyyMMddHHmmss')).$PID"
    }
    Move-Item -LiteralPath $CommandPath -Destination $backup
    Write-Output "Backed up existing unmanaged repogrammar command to $backup"
}

function Install-ManagedCliBinary([string]$Binary, [bool]$AllowUnmanagedBackup) {
    if (!(Test-Path $Binary)) {
        throw "repogrammar source binary not found: $Binary"
    }
    $installedBinary = Join-Path (Join-Path $InstallDir "bin") "repogrammar.exe"
    $commandPath = Join-Path $CommandDir "repogrammar.exe"
    if ((Test-Path $commandPath) -and !(Test-ManagedCommandPath $commandPath $installedBinary)) {
        if ($AllowUnmanagedBackup) {
            Backup-UnmanagedCommand $commandPath
        } else {
            throw "repogrammar command path already exists and is not managed by RepoGrammar; move it aside or choose a different CommandDir"
        }
    }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $installedBinary) | Out-Null
    $tmpInstalled = "$installedBinary.tmp.$PID"
    Copy-Item $Binary $tmpInstalled -Force
    Move-Item $tmpInstalled $installedBinary -Force
    New-Item -ItemType Directory -Force -Path $CommandDir | Out-Null
    $tmpCommand = "$commandPath.tmp.$PID"
    Copy-Item $installedBinary $tmpCommand -Force
    Move-Item $tmpCommand $commandPath -Force
    Write-Output "Installed $commandPath"
}

function Get-SourceBinaryPath {
    if (![string]::IsNullOrWhiteSpace($SourceBinary)) {
        return [System.IO.Path]::GetFullPath($SourceBinary)
    }
    return Join-Path (Join-Path $RepoRoot "target\release") "repogrammar.exe"
}

function Build-SourceBinary([string]$Binary) {
    if (!(Test-SourceCheckout)) {
        throw "source build requires running this script from a RepoGrammar source checkout"
    }
    if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo is required for -FromSource unless -SourceBinary or REPOGRAMMAR_SOURCE_BINARY points at an already built binary"
    }
    Write-Output "Building repogrammar.exe with cargo build --release"
    Push-Location $RepoRoot
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
    if (!(Test-Path $Binary)) {
        throw "cargo build completed but did not create expected binary: $Binary"
    }
}

function Install-CliFromSource {
    if (!(Test-SourceCheckout)) {
        throw "source install requires running this script from a RepoGrammar source checkout"
    }
    $binary = Get-SourceBinaryPath
    $sourceBinaryProvided = ![string]::IsNullOrWhiteSpace($SourceBinary)
    if ($sourceBinaryProvided) {
        if (!(Test-Path $binary)) {
            throw "repogrammar source binary not found: $binary"
        }
    } else {
        Build-SourceBinary $binary
    }
    Install-WorkerAsset (Join-Path $RepoRoot "src\workers\python\worker.py")
    Install-ManagedCliBinary $binary $true
    $sourceLabel = $(if ($sourceBinaryProvided) { "provided source binary" } else { "source build" })
    Write-Output "Installed $CommandDir\repogrammar.exe from $sourceLabel"
}

function Install-CliFromRelease {
    $artifact = "repogrammar-x86_64-pc-windows-msvc.zip"
    $installedBinary = Join-Path (Join-Path $InstallDir "bin") "repogrammar.exe"
    $commandPath = Join-Path $CommandDir "repogrammar.exe"
    if ((Test-Path $commandPath) -and !(Test-ManagedCommandPath $commandPath $installedBinary)) {
        throw "repogrammar command path already exists and is not managed by RepoGrammar; move it aside or choose a different CommandDir"
    }
    $temp = New-Item -ItemType Directory -Path ([System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "repogrammar-install-$([System.Guid]::NewGuid())"))
    try {
        $archive = Join-Path $temp.FullName $artifact
        $checksum = Join-Path $temp.FullName "$artifact.sha256"
        Copy-ReleaseAsset $artifact $archive
        Copy-ReleaseAsset "$artifact.sha256" $checksum
        $expected = ((Get-Content $checksum -Raw) -split "\s+")[0].ToLowerInvariant()
        $actual = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
        if ($expected -ne $actual) {
            throw "checksum verification failed for $artifact"
        }
        Assert-SafeArchiveEntries $archive
        Expand-Archive -Path $archive -DestinationPath $temp.FullName -Force
        $binary = Join-Path $temp.FullName "repogrammar.exe"
        if (!(Test-Path $binary)) {
            throw "release artifact did not contain repogrammar.exe"
        }
        $worker = Join-Path $temp.FullName "workers\python\worker.py"
        if (!(Test-Path $worker)) {
            throw "release artifact did not contain bundled Python worker at workers/python/worker.py"
        }
        Install-WorkerAsset $worker
        Install-ManagedCliBinary $binary $false
    } finally {
        Remove-Item -Recurse -Force $temp.FullName -ErrorAction SilentlyContinue
    }
}

function Install-Cli {
    if ($script:UseSource) {
        Install-CliFromSource
    } else {
        Install-CliFromRelease
    }
}

function Test-ManagedCommandPath([string]$CommandPath, [string]$InstalledBinary) {
    if (!(Test-Path $CommandPath)) {
        return $true
    }
    if (!(Test-Path $InstalledBinary)) {
        return $false
    }
    $commandHash = (Get-FileHash -Algorithm SHA256 $CommandPath).Hash.ToLowerInvariant()
    $installedHash = (Get-FileHash -Algorithm SHA256 $InstalledBinary).Hash.ToLowerInvariant()
    return $commandHash -eq $installedHash
}

function Invoke-WithInstallEnv([scriptblock]$Script) {
    $previousInstallDir = $env:REPOGRAMMAR_INSTALL_DIR
    $previousCommandDir = $env:REPOGRAMMAR_COMMAND_DIR
    $previousExecutable = $env:REPOGRAMMAR_EXECUTABLE
    try {
        $env:REPOGRAMMAR_INSTALL_DIR = $InstallDir
        $env:REPOGRAMMAR_COMMAND_DIR = $CommandDir
        $env:REPOGRAMMAR_EXECUTABLE = Join-Path (Join-Path $InstallDir "bin") "repogrammar.exe"
        & $Script
    } finally {
        $env:REPOGRAMMAR_INSTALL_DIR = $previousInstallDir
        $env:REPOGRAMMAR_COMMAND_DIR = $previousCommandDir
        $env:REPOGRAMMAR_EXECUTABLE = $previousExecutable
    }
}

function Run-AgentInstall {
    $command = Join-Path $CommandDir "repogrammar.exe"
    if (!(Test-Path $command)) {
        throw "repogrammar.exe is not installed; install the CLI first"
    }
    Invoke-WithInstallEnv {
        if ($Yes) {
            & $command install --target $Target --scope $Scope --yes --no-telemetry
        } else {
            & $command install
        }
        if ($LASTEXITCODE -ne 0) {
            throw "repogrammar install failed with exit code $LASTEXITCODE"
        }
    }
}

function Remove-Command {
    $command = Join-Path $CommandDir "repogrammar.exe"
    if (!(Test-Path $command)) {
        Write-Output "No repogrammar command found at $command"
        return
    }
    if (!(Confirm-DefaultNo "Remove repogrammar command at $command?")) {
        Write-Output "Cancelled. The repogrammar command was not removed."
        return
    }
    Remove-Item $command -Force
    Write-Output "Removed $command"
}

if ($Help) {
    Show-Usage
    exit 0
}

if ($InstallCliOnly) {
    Install-Cli
    exit 0
}

if ($InstallAndConfigure) {
    Install-Cli
    Run-AgentInstall
    exit 0
}

if ($UninstallCommand) {
    Remove-Command
    exit 0
}

Write-Output "RepoGrammar setup"
Write-Output ""
if (Test-SourceCheckout) {
    Write-Output "1 = build/install from this source checkout and configure coding agents"
    Write-Output "2 = build/install command from this source checkout only"
} else {
    Write-Output "1 = install or update repogrammar and configure coding agents"
    Write-Output "2 = install or update repogrammar command only"
}
Write-Output "3 = configure coding agents only"
Write-Output "4 = uninstall repogrammar command only"
if (Test-SourceCheckout) {
    Write-Output "5 = install or update from release artifact instead"
}
Write-Output "q = cancel"
$choice = Read-Host "Selection [1]"
switch ($choice) {
    "" { if (Test-SourceCheckout) { $script:UseSource = $true }; Install-Cli; Run-AgentInstall; break }
    "1" { if (Test-SourceCheckout) { $script:UseSource = $true }; Install-Cli; Run-AgentInstall; break }
    "2" { if (Test-SourceCheckout) { $script:UseSource = $true }; Install-Cli; break }
    "3" { Run-AgentInstall; break }
    "4" { Remove-Command; break }
    "5" { if (Test-SourceCheckout) { $script:UseSource = $false; Install-Cli } else { throw "invalid selection: $choice" }; break }
    "q" { Write-Output "Cancelled. No changes made."; break }
    "Q" { Write-Output "Cancelled. No changes made."; break }
    default { throw "invalid selection: $choice" }
}
