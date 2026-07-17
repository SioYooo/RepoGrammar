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
    [switch]$UninstallCommand,
    [switch]$Verify,
    [switch]$Prune,
    [switch]$Purge,
    [string]$Project = "",
    [switch]$Yes,
    [switch]$ReplaceUnmanagedCommand,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $ScriptDir "..\.."))

function Show-Usage {
    Write-Output @"
RepoGrammar Windows contributor installer

Usage:
  powershell -ExecutionPolicy Bypass -File install.ps1 -FromSource
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallCliOnly -FromSource -Yes
  powershell -ExecutionPolicy Bypass -File install.ps1 -InstallAndConfigure -FromSource -Yes -Target all
  powershell -ExecutionPolicy Bypass -File install.ps1 -UninstallCommand -Yes
  powershell -ExecutionPolicy Bypass -File install.ps1 -Verify
  powershell -ExecutionPolicy Bypass -File install.ps1 -Prune -Yes
  powershell -ExecutionPolicy Bypass -File install.ps1 -Purge -Project . -Yes

-Verify reports, by SHA256, whether the repogrammar copies on PATH, the
configured agent MCP servers, and any running serve processes match the managed
authority binary. -Prune additionally removes PATH copies whose hash differs
from the authority (add -Yes to skip the confirmation). Install/update actions
run the same stale PATH cleanup after refreshing the managed command.
-Purge fully removes RepoGrammar: it prints a plan, then stops repogrammar
processes, runs uninstall (agent MCP entries and receipts), optionally runs
uninit on -Project (the .repogrammar state), and deletes every repogrammar
binary, worker asset, and the managed data directory. Add -Yes to skip the
confirmation prompt.

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
    Install-CliFromSource
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

function Resolve-AnyRepogrammar {
    $candidates = @((Join-Path $CommandDir "repogrammar.exe"), (Get-AuthorityBinary))
    $candidates += Get-RepogrammarPathCopies
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            return $candidate
        }
    }
    return $null
}

function Test-PurgeOwnedPath([string]$Candidate, $FileTargets) {
    if ([string]::IsNullOrWhiteSpace($Candidate)) {
        return $false
    }
    $lower = $Candidate.ToLowerInvariant()
    foreach ($file in $FileTargets) {
        if ($lower -eq $file.ToLowerInvariant()) {
            return $true
        }
    }
    foreach ($root in @($InstallDir, $CommandDir)) {
        if ($root -and $lower.StartsWith($root.ToLowerInvariant().TrimEnd('\') + '\')) {
            return $true
        }
    }
    return $false
}

function Invoke-Purge {
    $commandBin = Join-Path $CommandDir "repogrammar.exe"
    $commandWorkers = Join-Path $CommandDir "repogrammar-workers"
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin\repogrammar.exe"
    $cargoGuard = Join-Path $env:USERPROFILE ".cargo\bin\repo-guard.exe"

    $fileTargets = @()
    foreach ($file in (@($commandBin, $cargoBin, $cargoGuard) + (Get-RepogrammarPathCopies))) {
        if ($file -and (Test-Path -LiteralPath $file -PathType Leaf)) {
            $fileTargets += (Resolve-Path -LiteralPath $file).Path
        }
    }
    $fileTargets = @($fileTargets | Select-Object -Unique)

    $dirTargets = @()
    foreach ($dir in @($InstallDir, $commandWorkers)) {
        if ($dir -and (Test-Path -LiteralPath $dir)) {
            $dirTargets += $dir
        }
    }
    $dirTargets = @($dirTargets | Select-Object -Unique)

    Write-Output "RepoGrammar purge plan:"
    Write-Output "  - Stop repogrammar processes that run the binaries listed below"
    Write-Output "  - repogrammar uninstall --target all --scope global (remove agent MCP entries and receipts)"
    if ($Project) {
        Write-Output "  - repogrammar uninit --project $Project --yes (remove .repogrammar state)"
    }
    foreach ($dir in $dirTargets) { Write-Output "  - remove directory $dir" }
    foreach ($file in $fileTargets) { Write-Output "  - remove file $file" }
    Write-Output ""

    if (!$Yes -and !(Confirm-DefaultNo "Proceed with purge? This permanently deletes the items above")) {
        Write-Output "Cancelled. Nothing was removed."
        return
    }

    try {
        $running = Get-CimInstance Win32_Process -Filter "name='repogrammar.exe'" -ErrorAction SilentlyContinue
    } catch {
        $running = @()
    }
    foreach ($proc in $running) {
        if (Test-PurgeOwnedPath $proc.ExecutablePath $fileTargets) {
            try { Stop-Process -Id $proc.ProcessId -Force -ErrorAction SilentlyContinue } catch {}
        }
    }

    $rg = Resolve-AnyRepogrammar
    if ($rg) {
        $previousEap = $ErrorActionPreference
        $ErrorActionPreference = "SilentlyContinue"
        try {
            Invoke-WithInstallEnv {
                & $rg uninstall --target all --scope global --yes 2>&1 | Out-Null
                if ($Project -and (Test-Path -LiteralPath (Join-Path $Project ".repogrammar"))) {
                    & $rg uninit --project $Project --yes 2>&1 | Out-Null
                }
            }
        } finally {
            $ErrorActionPreference = $previousEap
        }
    } else {
        Write-Output "No repogrammar binary found to run uninstall/uninit; removing files only."
    }

    foreach ($file in $fileTargets) {
        try {
            Remove-Item -LiteralPath $file -Force -ErrorAction Stop
            Write-Output "Removed $file"
        } catch {
            Write-Output "Failed to remove ${file}: $($_.Exception.Message)"
        }
    }
    foreach ($dir in $dirTargets) {
        try {
            Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction Stop
            Write-Output "Removed $dir"
        } catch {
            Write-Output "Failed to remove ${dir}: $($_.Exception.Message)"
        }
    }
    Write-Output "Purge complete. Re-run with -InstallAndConfigure -FromSource -Yes to reinstall."
}

if ($Help) {
    Show-Usage
    exit 0
}

if ($Verify -or $Prune) {
    Invoke-VerifyInstall ([bool]$Prune)
    exit 0
}

if ($Purge) {
    Invoke-Purge
    exit 0
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

if ($UninstallCommand) {
    Remove-Command
    exit 0
}

Assert-WindowsSourceInstallEnabled
Write-Output "RepoGrammar Windows contributor setup"
Write-Output ""
Write-Output "1 = build/install from this source checkout and configure coding agents"
Write-Output "2 = build/install command from this source checkout only"
Write-Output "3 = configure coding agents only"
Write-Output "4 = uninstall repogrammar command only"
Write-Output "q = cancel"
$choice = Read-Host "Selection [1]"
switch ($choice) {
    "" { Install-Cli; Run-AgentInstall; Invoke-VerifyInstall $true; break }
    "1" { Install-Cli; Run-AgentInstall; Invoke-VerifyInstall $true; break }
    "2" { Install-Cli; Invoke-VerifyInstall $true; break }
    "3" { Run-AgentInstall; break }
    "4" { Remove-Command; break }
    "q" { Write-Output "Cancelled. No changes made."; break }
    "Q" { Write-Output "Cancelled. No changes made."; break }
    default { throw "invalid selection: $choice" }
}
