param(
    [string]$Version = $(if ($env:REPOGRAMMAR_VERSION) { $env:REPOGRAMMAR_VERSION } else { "latest" }),
    [string]$Repo = $(if ($env:REPOGRAMMAR_REPO) { $env:REPOGRAMMAR_REPO } else { "SioYooo/RepoGrammar" }),
    [string]$CommandDir = $(if ($env:REPOGRAMMAR_COMMAND_DIR) { $env:REPOGRAMMAR_COMMAND_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\RepoGrammar\bin" }),
    [string]$WorkerRoot = $(if ($env:REPOGRAMMAR_WORKER_ROOT) { $env:REPOGRAMMAR_WORKER_ROOT } else { Join-Path (Split-Path -Parent $CommandDir) "share\repogrammar\workers" }),
    [switch]$InstallCliOnly,
    [switch]$InstallAndConfigure,
    [switch]$UninstallCommand,
    [switch]$Yes,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

function Show-Usage {
    Write-Output @"
RepoGrammar Windows installer preview

Usage:
  powershell -ExecutionPolicy Bypass -File install.ps1
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallCliOnly
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallAndConfigure
  powershell -ExecutionPolicy Bypass -File install.ps1 -UninstallCommand -Yes

The script downloads a prebuilt Windows x64 release artifact, verifies its
checksum, installs repogrammar.exe into a user-writable command directory, and
can launch repogrammar install for Codex / Claude Code MCP wiring.
"@
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

function Install-Cli {
    $artifact = "repogrammar-x86_64-pc-windows-msvc.zip"
    $temp = New-Item -ItemType Directory -Path ([System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "repogrammar-install-$([System.Guid]::NewGuid())"))
    try {
        $archive = Join-Path $temp.FullName $artifact
        $checksum = Join-Path $temp.FullName "$artifact.sha256"
        if ($env:REPOGRAMMAR_RELEASE_DIR) {
            Copy-Item (Join-Path $env:REPOGRAMMAR_RELEASE_DIR $artifact) $archive
            Copy-Item (Join-Path $env:REPOGRAMMAR_RELEASE_DIR "$artifact.sha256") $checksum
        } else {
            $base = Get-ReleaseBase
            Invoke-WebRequest -Uri "$base/$artifact" -OutFile $archive
            Invoke-WebRequest -Uri "$base/$artifact.sha256" -OutFile $checksum
        }
        $expected = ((Get-Content $checksum -Raw) -split "\s+")[0].ToLowerInvariant()
        $actual = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
        if ($expected -ne $actual) {
            throw "checksum verification failed for $artifact"
        }
        Expand-Archive -Path $archive -DestinationPath $temp.FullName -Force
        $binary = Join-Path $temp.FullName "repogrammar.exe"
        if (!(Test-Path $binary)) {
            throw "release artifact did not contain repogrammar.exe"
        }
        New-Item -ItemType Directory -Force -Path $CommandDir | Out-Null
        Copy-Item $binary (Join-Path $CommandDir "repogrammar.exe") -Force
        $worker = Join-Path $temp.FullName "workers\python\worker.py"
        if (Test-Path $worker) {
            $workerDest = Join-Path $WorkerRoot "python"
            New-Item -ItemType Directory -Force -Path $workerDest | Out-Null
            Copy-Item $worker (Join-Path $workerDest "worker.py") -Force
        }
        Write-Output "Installed $(Join-Path $CommandDir "repogrammar.exe")"
    } finally {
        Remove-Item -Recurse -Force $temp.FullName -ErrorAction SilentlyContinue
    }
}

function Run-AgentInstall {
    $command = Join-Path $CommandDir "repogrammar.exe"
    if (!(Test-Path $command)) {
        throw "repogrammar.exe is not installed; install the CLI first"
    }
    if ($Yes) {
        & $command install --target all --scope global --yes --no-telemetry
    } else {
        & $command install
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
Write-Output "1 = install or update repogrammar and configure coding agents"
Write-Output "2 = install or update repogrammar command only"
Write-Output "3 = configure coding agents only"
Write-Output "4 = uninstall repogrammar command only"
Write-Output "q = cancel"
$choice = Read-Host "Selection [1]"
switch ($choice) {
    "" { Install-Cli; Run-AgentInstall }
    "1" { Install-Cli; Run-AgentInstall }
    "2" { Install-Cli }
    "3" { Run-AgentInstall }
    "4" { Remove-Command }
    "q" { Write-Output "Cancelled. No changes made." }
    "Q" { Write-Output "Cancelled. No changes made." }
    default { throw "invalid selection: $choice" }
}
