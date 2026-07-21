param(
    [string]$CommandDir = $(if ($env:REPOGRAMMAR_COMMAND_DIR) { $env:REPOGRAMMAR_COMMAND_DIR } else { Join-Path $env:USERPROFILE ".local\bin" }),
    [string]$InstallDir = $(if ($env:REPOGRAMMAR_INSTALL_DIR) { $env:REPOGRAMMAR_INSTALL_DIR } elseif ($env:XDG_DATA_HOME) { Join-Path $env:XDG_DATA_HOME "repogrammar" } else { Join-Path $env:USERPROFILE ".local\share\repogrammar" }),
    [string]$WorkerRoot = $(if ($env:REPOGRAMMAR_WORKER_ROOT) { $env:REPOGRAMMAR_WORKER_ROOT } else { Join-Path $InstallDir "workers" }),
    [string]$SourceBinary = $(if ($env:REPOGRAMMAR_SOURCE_BINARY) { $env:REPOGRAMMAR_SOURCE_BINARY } else { "" }),
    [string]$Target = "all",
    [string]$Scope = "global",
    [switch]$FromSource,
    [switch]$InstallCliOnly,
    [switch]$InstallAndConfigure,
    [switch]$DisconnectAgents,
    [switch]$UninstallCommand,
    [switch]$Verify,
    [switch]$Prune,
    [switch]$Purge,
    [switch]$DryRun,
    [string]$Project = "",
    [switch]$Yes,
    [switch]$ReplaceUnmanagedCommand,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$TargetWasSpecified = $PSBoundParameters.ContainsKey("Target")
$ScopeWasSpecified = $PSBoundParameters.ContainsKey("Scope")
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $ScriptDir "..\.."))

function Show-Usage {
    Write-Output @"
RepoGrammar Windows contributor installer

Usage:
  powershell -ExecutionPolicy Bypass -File install.ps1 -FromSource
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallCliOnly -FromSource -Yes
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallAndConfigure -FromSource -Yes -Target all
  powershell -ExecutionPolicy Bypass -File install.ps1 -DisconnectAgents -Target all -Yes
  powershell -ExecutionPolicy Bypass -File install.ps1 -UninstallCommand -Yes # deprecated guidance
  powershell -ExecutionPolicy Bypass -File install.ps1 -Verify
  powershell -ExecutionPolicy Bypass -File install.ps1 -Prune -Yes
  powershell -ExecutionPolicy Bypass -File install.ps1 -Purge -Yes

-Verify reports, by SHA256, whether the repogrammar copies on PATH, the
configured agent MCP servers, and any running serve processes match the managed
authority binary. -Prune additionally removes PATH copies whose hash differs
from the authority (add -Yes to skip the confirmation). Install/update actions
run the same stale PATH cleanup after refreshing the managed command.
-DisconnectAgents delegates receipt-owned coding-agent removal to
repogrammar disconnect. -Purge delegates complete managed product removal to
repogrammar uninstall; the Rust CLI owns all path validation and cleanup.
-DryRun previews -Purge. Repository-local .repogrammar state is always
preserved. If -Project is supplied, the wrapper prints the separate explicit
repogrammar uninit --project <path> --yes command but does not run it.
-UninstallCommand is deprecated because command-only deletion leaves managed
installation state behind.

RepoGrammar does not publish or support a Windows release artifact.
Installation is fail-closed unless -FromSource is passed explicitly from a
RepoGrammar source checkout. -FromSource builds or copies a local
target\release\repogrammar.exe before optional agent setup. -SourceBinary or
REPOGRAMMAR_SOURCE_BINARY may point at an already built local executable and
skip the cargo build, but neither option enables a release download.

If the command directory already holds a repogrammar.exe that RepoGrammar did
not install, the installer refuses to replace it unless you also pass
-ReplaceUnmanagedCommand, which backs the existing file up first. -Yes alone
does not replace an unmanaged command.
Custom -WorkerRoot/REPOGRAMMAR_WORKER_ROOT locations are rejected for managed
installs because the product receipt covers only deterministic first-party
worker paths.

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

function Install-WorkerAsset([string]$WorkerSource) {
    if (!(Test-Path $WorkerSource)) {
        throw "source checkout did not contain bundled Python worker at src/workers/python/worker.py"
    }
    $workerRoots = @($WorkerRoot)
    $deterministicWorkerRoot = Join-Path $InstallDir "workers"
    if ([System.IO.Path]::GetFullPath($WorkerRoot) -eq [System.IO.Path]::GetFullPath($deterministicWorkerRoot)) {
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

function Install-TempFileReplacing([string]$TempPath, [string]$Destination, [string]$Label) {
    if (!(Test-Path -LiteralPath $TempPath)) {
        throw "temporary $Label was not created: $TempPath"
    }
    if (Test-Path -LiteralPath $Destination -PathType Container) {
        Remove-Item -LiteralPath $TempPath -Force -ErrorAction SilentlyContinue
        throw "$Label path is a directory and cannot be replaced automatically: $Destination"
    }

    $backup = $null
    if (Test-Path -LiteralPath $Destination) {
        $backup = "$Destination.replace-backup.$PID.$([guid]::NewGuid().ToString('N'))"
        try {
            Move-Item -LiteralPath $Destination -Destination $backup -ErrorAction Stop
        } catch {
            Remove-Item -LiteralPath $TempPath -Force -ErrorAction SilentlyContinue
            throw "failed to remove previous $Label at ${Destination}: $($_.Exception.Message). Exit any running coding agent sessions that use RepoGrammar MCP, then rerun the install or build command."
        }
    }

    try {
        Move-Item -LiteralPath $TempPath -Destination $Destination -ErrorAction Stop
    } catch {
        if ($backup -and (Test-Path -LiteralPath $backup)) {
            Move-Item -LiteralPath $backup -Destination $Destination -ErrorAction SilentlyContinue
        }
        Remove-Item -LiteralPath $TempPath -Force -ErrorAction SilentlyContinue
        throw "failed to install $Label at ${Destination}: $($_.Exception.Message)"
    }

    if ($backup -and (Test-Path -LiteralPath $backup)) {
        try {
            Remove-Item -LiteralPath $backup -Force -ErrorAction Stop
        } catch {
            throw "failed to delete previous $Label at ${backup}: $($_.Exception.Message). Exit any running coding agent sessions that use RepoGrammar MCP, then rerun the install or build command."
        }
    }
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
            throw "repogrammar command path already exists and is not managed by RepoGrammar; move it aside, choose a different -CommandDir, or pass -ReplaceUnmanagedCommand to back it up and replace it"
        }
    }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $installedBinary) | Out-Null
    $tmpInstalled = "$installedBinary.tmp.$PID"
    Copy-Item -LiteralPath $Binary -Destination $tmpInstalled -Force
    Install-TempFileReplacing $tmpInstalled $installedBinary "managed repogrammar executable"
    New-Item -ItemType Directory -Force -Path $CommandDir | Out-Null
    $tmpCommand = "$commandPath.tmp.$PID"
    Copy-Item -LiteralPath $installedBinary -Destination $tmpCommand -Force
    Install-TempFileReplacing $tmpCommand $commandPath "repogrammar command"
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
    Install-ManagedCliBinary $binary ([bool]$ReplaceUnmanagedCommand)
    $sourceLabel = $(if ($sourceBinaryProvided) { "provided source binary" } else { "source build" })
    Write-Output "Installed $CommandDir\repogrammar.exe from $sourceLabel"
}

function Assert-WindowsSourceInstallEnabled {
    if (!$FromSource) {
        throw "Windows has no supported RepoGrammar release artifact; installation requires explicit -FromSource from a RepoGrammar source checkout"
    }
}

function Install-Cli {
    Assert-WindowsSourceInstallEnabled
    $deterministicWorkerRoot = [System.IO.Path]::GetFullPath((Join-Path $InstallDir "workers"))
    if ([System.IO.Path]::GetFullPath($WorkerRoot) -ne $deterministicWorkerRoot) {
        throw "custom -WorkerRoot/REPOGRAMMAR_WORKER_ROOT is not supported for first-party managed installs; the product ownership receipt only covers $deterministicWorkerRoot and the managed command worker root"
    }
    Install-CliFromSource
    Record-CliInstallReceipt
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

function Record-CliInstallReceipt {
    $command = Join-Path $CommandDir "repogrammar.exe"
    if (!(Test-Path -LiteralPath $command -PathType Leaf)) {
        throw "installed repogrammar command is unavailable for receipt creation"
    }
    Invoke-WithInstallEnv {
        & $command install --target none --scope global --yes --no-telemetry
        if ($LASTEXITCODE -ne 0) {
            throw "repogrammar product receipt creation failed with exit code $LASTEXITCODE"
        }
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

function Write-DeprecatedUninstallCommandGuidance {
    throw "-UninstallCommand is deprecated because command-only removal leaves managed installation state behind. Use -Purge (or repogrammar uninstall --yes) for the managed product installation; use -DisconnectAgents (or repogrammar disconnect --target all --yes) for coding-agent integrations only."
}

function Get-AuthorityBinary {
    return Join-Path (Join-Path $InstallDir "bin") "repogrammar.exe"
}

function Get-Sha256OrNull([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    }
    return $null
}

function Get-RepogrammarPathCopies {
    $copies = @()
    $seen = @{}
    foreach ($dir in ($env:PATH -split ';')) {
        if ([string]::IsNullOrWhiteSpace($dir)) {
            continue
        }
        $candidate = Join-Path $dir "repogrammar.exe"
        if (!(Test-Path -LiteralPath $candidate -PathType Leaf)) {
            continue
        }
        try {
            $resolved = (Resolve-Path -LiteralPath $candidate).Path
        } catch {
            $resolved = $candidate
        }
        $key = $resolved.ToLowerInvariant()
        if ($seen.ContainsKey($key)) {
            continue
        }
        $seen[$key] = $true
        $copies += $resolved
    }
    return $copies
}

function Get-CodexMcpCommand {
    $cfg = Join-Path $env:USERPROFILE ".codex\config.toml"
    if (!(Test-Path -LiteralPath $cfg)) {
        return $null
    }
    $inSection = $false
    foreach ($line in (Get-Content -LiteralPath $cfg)) {
        if ($line -match '^\s*\[mcp_servers\.repogrammar\]\s*$') {
            $inSection = $true
            continue
        }
        if ($inSection -and $line -match '^\s*\[') {
            break
        }
        if ($inSection -and $line -match "^\s*command\s*=\s*['""](.+?)['""]\s*$") {
            return $matches[1]
        }
    }
    return $null
}

function Get-ClaudeMcpCommand {
    $cfg = Join-Path $env:USERPROFILE ".claude.json"
    if (!(Test-Path -LiteralPath $cfg)) {
        return $null
    }
    try {
        $json = Get-Content -LiteralPath $cfg -Raw | ConvertFrom-Json
    } catch {
        return $null
    }
    if (($json.PSObject.Properties.Name -contains 'mcpServers') -and $json.mcpServers -and
        ($json.mcpServers.PSObject.Properties.Name -contains 'repogrammar')) {
        return $json.mcpServers.repogrammar.command
    }
    return $null
}

function Get-RepogrammarProcesses {
    $procs = @()
    try {
        $list = Get-CimInstance Win32_Process -Filter "name='repogrammar.exe'" -ErrorAction SilentlyContinue
    } catch {
        return $procs
    }
    foreach ($entry in $list) {
        $procs += [PSCustomObject]@{
            ProcessId = $entry.ProcessId
            Path = $entry.ExecutablePath
            CommandLine = $entry.CommandLine
        }
    }
    return $procs
}

function Get-ServeProcessExecutables {
    $procs = @()
    foreach ($entry in (Get-RepogrammarProcesses)) {
        if ($entry.CommandLine -and $entry.CommandLine -match '\bserve\b') {
            $procs += [PSCustomObject]@{ ProcessId = $entry.ProcessId; Path = $entry.Path }
        }
    }
    return $procs
}

function Get-RepogrammarProcessesUsingPath([string]$Path) {
    $matches = @()
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $matches
    }
    try {
        $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    } catch {
        $resolvedPath = $Path
    }
    foreach ($entry in (Get-RepogrammarProcesses)) {
        if ([string]::IsNullOrWhiteSpace($entry.Path)) {
            continue
        }
        try {
            $entryPath = [System.IO.Path]::GetFullPath($entry.Path)
        } catch {
            $entryPath = $entry.Path
        }
        if ($entryPath.ToLowerInvariant() -eq $resolvedPath.ToLowerInvariant()) {
            $matches += $entry
        }
    }
    return $matches
}

function Write-StaleRemovalFailure([string]$Entry, [string]$Message) {
    Write-Output "Failed to remove ${Entry}: $Message"
    $usingProcesses = Get-RepogrammarProcessesUsingPath $Entry
    if ($usingProcesses.Count -gt 0) {
        $pids = ($usingProcesses | ForEach-Object { $_.ProcessId }) -join ", "
        Write-Output "  Running repogrammar process(es) using it: PID $pids"
    }
    Write-Output "  Exit any process using it, then retry. If this copy came from cargo, run: cargo uninstall repogrammar"
}

function Format-HashTag([string]$Hash, [string]$AuthorityHash) {
    if ([string]::IsNullOrWhiteSpace($Hash)) {
        return "[missing]"
    }
    $short = $Hash.Substring(0, [Math]::Min(12, $Hash.Length))
    if ($AuthorityHash -and ($Hash -eq $AuthorityHash)) {
        return "[$short matches authority]"
    }
    return "[$short DIFFERENT build]"
}

function Invoke-VerifyInstall([bool]$DoPrune) {
    $authority = Get-AuthorityBinary
    $authorityHash = Get-Sha256OrNull $authority
    Write-Output "RepoGrammar install verification"
    Write-Output ""
    if ($null -eq $authorityHash) {
        Write-Output "Authority (managed binary): $authority [NOT INSTALLED]"
        Write-Output "Install first, for example: install.ps1 -InstallAndConfigure -FromSource -Yes"
        return
    }
    Write-Output "Authority (managed binary): $authority $(Format-HashTag $authorityHash $authorityHash)"
    Write-Output ""

    Write-Output "repogrammar on PATH:"
    $copies = Get-RepogrammarPathCopies
    $stale = @()
    if ($copies.Count -eq 0) {
        Write-Output "  (none)"
    }
    foreach ($copy in $copies) {
        $copyHash = Get-Sha256OrNull $copy
        Write-Output "  $copy $(Format-HashTag $copyHash $authorityHash)"
        if ($copyHash -ne $authorityHash) {
            $stale += $copy
        }
    }
    Write-Output ""

    Write-Output "Agent MCP servers point at:"
    $codexCmd = Get-CodexMcpCommand
    $claudeCmd = Get-ClaudeMcpCommand
    if ($codexCmd) {
        Write-Output "  codex  -> $codexCmd $(Format-HashTag (Get-Sha256OrNull $codexCmd) $authorityHash)"
    } else {
        Write-Output "  codex  -> (no RepoGrammar MCP entry)"
    }
    if ($claudeCmd) {
        Write-Output "  claude -> $claudeCmd $(Format-HashTag (Get-Sha256OrNull $claudeCmd) $authorityHash)"
    } else {
        Write-Output "  claude -> (no RepoGrammar MCP entry)"
    }
    Write-Output ""

    Write-Output "Running serve processes:"
    $serves = Get-ServeProcessExecutables
    if ($serves.Count -eq 0) {
        Write-Output "  (none running)"
    }
    foreach ($entry in $serves) {
        Write-Output "  PID $($entry.ProcessId) -> $($entry.Path) $(Format-HashTag (Get-Sha256OrNull $entry.Path) $authorityHash)"
    }
    Write-Output ""

    $agentsConsistent = $true
    foreach ($cmd in @($codexCmd, $claudeCmd)) {
        if ($cmd -and ((Get-Sha256OrNull $cmd) -ne $authorityHash)) {
            $agentsConsistent = $false
        }
    }
    if ($agentsConsistent) {
        Write-Output "OK: configured agents use the same build as the managed authority."
    } else {
        Write-Output "WARNING: an agent MCP entry points at a different build than the authority."
        Write-Output "  Re-run: install.ps1 -InstallAndConfigure -FromSource -Yes to repoint agents."
    }
    if ($stale.Count -gt 0) {
        Write-Output ""
        Write-Output "Stale PATH copies (different build than the authority): $($stale.Count)"
        foreach ($entry in $stale) {
            Write-Output "  $entry"
        }
        if ($DoPrune) {
            if (!$Yes -and !(Confirm-DefaultNo "Remove the stale PATH copies listed above?")) {
                Write-Output "Cancelled. No copies removed."
            } else {
                $pruneFailures = @()
                foreach ($entry in $stale) {
                    try {
                        Remove-Item -LiteralPath $entry -Force -ErrorAction Stop
                        Write-Output "Removed $entry"
                    } catch {
                        $pruneFailures += $entry
                        Write-StaleRemovalFailure $entry $_.Exception.Message
                    }
                }
                if ($pruneFailures.Count -gt 0) {
                    throw "failed to remove $($pruneFailures.Count) stale PATH copy/copies; see messages above."
                }
            }
        } else {
            Write-Output "Run with -Prune (add -Yes to skip the prompt) to remove these stale copies."
        }
    }
}

function Resolve-ManagedRepogrammar {
    $command = Join-Path $CommandDir "repogrammar.exe"
    $authority = Get-AuthorityBinary
    if ((Test-Path -LiteralPath $command -PathType Leaf) -and
        (Test-Path -LiteralPath $authority -PathType Leaf) -and
        (Test-ManagedCommandPath $command $authority)) {
        return $command
    }
    if (Test-Path -LiteralPath $authority -PathType Leaf) {
        return $authority
    }
    return $null
}

function Invoke-AgentDisconnect {
    $rg = Resolve-ManagedRepogrammar
    if (!$rg) {
        throw "managed repogrammar command is unavailable; reinstall once before disconnecting coding-agent integrations"
    }
    if (!$Yes -and !(Confirm-DefaultNo "Remove RepoGrammar-owned $Target coding-agent integrations?")) {
        Write-Output "Cancelled. No coding-agent integrations were removed."
        return
    }
    Invoke-WithInstallEnv {
        & $rg disconnect --target $Target --scope $Scope --yes
        if ($LASTEXITCODE -ne 0) {
            throw "repogrammar disconnect failed with exit code $LASTEXITCODE"
        }
    }
}

function Invoke-Purge {
    if ($TargetWasSpecified -or $ScopeWasSpecified) {
        throw "-Target/-Scope select coding-agent integrations, not product files. Use -DisconnectAgents -Target <agents> -Scope <scope> -Yes."
    }
    $rg = Resolve-ManagedRepogrammar
    if (!$rg) {
        throw "managed repogrammar command is unavailable; reinstall once to create an ownership receipt before uninstalling"
    }
    if ($Project) {
        Write-Output "Repository state is preserved: $Project\.repogrammar"
        Write-Output "Remove it separately only if intended: repogrammar uninit --project `"$Project`" --yes"
    }
    if (!$DryRun -and !$Yes -and !(Confirm-DefaultNo "Remove the RepoGrammar-managed product installation? Repository-local .repogrammar state will be preserved")) {
        Write-Output "Cancelled. The managed RepoGrammar installation was not removed."
        return
    }
    Invoke-WithInstallEnv {
        $arguments = @("uninstall", "--yes")
        if ($DryRun) {
            $arguments += "--dry-run"
        }
        & $rg @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "repogrammar uninstall failed with exit code $LASTEXITCODE"
        }
    }
}

if ($Help) {
    Show-Usage
    exit 0
}

if ($Verify -or $Prune) {
    if ($DryRun) {
        throw "-DryRun is only valid with -Purge"
    }
    Invoke-VerifyInstall ([bool]$Prune)
    exit 0
}

if ($Purge) {
    Invoke-Purge
    exit 0
}

if ($DryRun) {
    throw "-DryRun is only valid with -Purge"
}

if ($InstallCliOnly) {
    Install-Cli
    Invoke-VerifyInstall $true
    exit 0
}

if ($InstallAndConfigure) {
    Install-Cli
    Run-AgentInstall
    Invoke-VerifyInstall $true
    exit 0
}

if ($DisconnectAgents) {
    Invoke-AgentDisconnect
    exit 0
}

if ($UninstallCommand) {
    Write-DeprecatedUninstallCommandGuidance
    exit 0
}

Assert-WindowsSourceInstallEnabled
Write-Output "RepoGrammar Windows contributor setup"
Write-Output ""
Write-Output "1 = build/install from this source checkout and configure coding agents"
Write-Output "2 = build/install command from this source checkout only"
Write-Output "3 = configure coding agents only"
Write-Output "4 = show command-only uninstall migration guidance"
Write-Output "5 = disconnect RepoGrammar-owned coding-agent integrations"
Write-Output "6 = uninstall the RepoGrammar-managed product installation"
Write-Output "q = cancel"
$choice = Read-Host "Selection [1]"
switch ($choice) {
    "" { Install-Cli; Run-AgentInstall; Invoke-VerifyInstall $true; break }
    "1" { Install-Cli; Run-AgentInstall; Invoke-VerifyInstall $true; break }
    "2" { Install-Cli; Invoke-VerifyInstall $true; break }
    "3" { Run-AgentInstall; break }
    "4" { Write-DeprecatedUninstallCommandGuidance; break }
    "5" { Invoke-AgentDisconnect; break }
    "6" { Invoke-Purge; break }
    "q" { Write-Output "Cancelled. No changes made."; break }
    "Q" { Write-Output "Cancelled. No changes made."; break }
    default { throw "invalid selection: $choice" }
}
