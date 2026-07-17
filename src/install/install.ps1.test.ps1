$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $ScriptDir "..\.."))
$Installer = Join-Path $ScriptDir "install.ps1"
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("repogrammar-install-ps-test-" + [guid]::NewGuid().ToString())
$PowerShellExe = (Get-Command powershell).Source
$CargoExe = (Get-Command cargo).Source
$SavedGlobalUserProfile = $env:USERPROFILE
$SavedGlobalHome = $env:HOME

function Assert-PathExists([string]$Path, [string]$Message) {
    if (!(Test-Path $Path)) {
        throw $Message
    }
}

function Assert-Contains([string]$Path, [string]$Expected) {
    $contents = Get-Content -Raw $Path
    if (!$contents.Contains($Expected)) {
        throw "expected $Path to contain: $Expected"
    }
}

function Invoke-InstallerWithPath([string]$PathValue, [scriptblock]$Body) {
    $PreviousPath = $env:PATH
    try {
        $env:PATH = $PathValue
        & $Body
    } finally {
        $env:PATH = $PreviousPath
    }
}

try {
    New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null

    # Windows release acquisition is intentionally source-only. A
    # default install must fail before reading a local fake release directory,
    # downloading anything, or creating command/install state.
    $UnsupportedCommandDir = Join-Path $TempRoot "unsupported-bin"
    $UnsupportedInstallDir = Join-Path $TempRoot "unsupported-data"
    $UnsupportedReleaseDir = Join-Path $TempRoot "unsupported-release"
    $UnsupportedOut = Join-Path $TempRoot "unsupported.out"
    New-Item -ItemType Directory -Force -Path $UnsupportedReleaseDir | Out-Null
    Set-Content -Path (Join-Path $UnsupportedReleaseDir "must-not-be-read") -Value "sentinel"
    $SavedReleaseDir = $env:REPOGRAMMAR_RELEASE_DIR
    $SavedErrorActionPreference = $ErrorActionPreference
    try {
        $env:REPOGRAMMAR_RELEASE_DIR = $UnsupportedReleaseDir
        $ErrorActionPreference = "Continue"
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -CommandDir $UnsupportedCommandDir `
            -InstallDir $UnsupportedInstallDir `
            -Yes *> $UnsupportedOut
        $UnsupportedStatus = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $SavedErrorActionPreference
        if ($null -ne $SavedReleaseDir) { $env:REPOGRAMMAR_RELEASE_DIR = $SavedReleaseDir } else { Remove-Item Env:REPOGRAMMAR_RELEASE_DIR -ErrorAction SilentlyContinue }
    }
    if ($UnsupportedStatus -eq 0) {
        throw "Windows default release install unexpectedly succeeded"
    }
    Assert-Contains $UnsupportedOut "Windows has no supported RepoGrammar release artifact"
    Assert-Contains $UnsupportedOut "installation requires explicit -FromSource"
    if ((Test-Path $UnsupportedCommandDir) -or (Test-Path $UnsupportedInstallDir)) {
        throw "refused Windows release install created command or install state"
    }

    Push-Location $RepoRoot
    try {
        & $CargoExe build --quiet --bin repogrammar
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    $SourceBinary = Join-Path $RepoRoot "target\debug\repogrammar.exe"
    Assert-PathExists $SourceBinary "source binary was not built"

    $TestHome = Join-Path $TempRoot "home"
    New-Item -ItemType Directory -Force -Path $TestHome | Out-Null
    $env:USERPROFILE = $TestHome
    $env:HOME = $TestHome

    $CommandDir = Join-Path $TempRoot "bin"
    $InstallDir = Join-Path $TempRoot "data"
    Invoke-InstallerWithPath $CommandDir {
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir $CommandDir `
            -InstallDir $InstallDir `
            -Yes
    }
    if ($LASTEXITCODE -ne 0) {
        throw "source install failed with exit code $LASTEXITCODE"
    }
    Assert-PathExists (Join-Path $CommandDir "repogrammar.exe") "command was not installed"
    Assert-PathExists (Join-Path $InstallDir "bin\repogrammar.exe") "managed binary was not installed"
    Assert-PathExists (Join-Path $InstallDir "workers\python\worker.py") "worker asset was not installed"
    Assert-PathExists (Join-Path $CommandDir "repogrammar-workers\python\worker.py") "command worker asset was not installed"
    & (Join-Path $CommandDir "repogrammar.exe") version | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "installed repogrammar version check failed"
    }
    Invoke-InstallerWithPath $CommandDir {
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir $CommandDir `
            -InstallDir $InstallDir `
            -Yes
    }
    if ($LASTEXITCODE -ne 0) {
        throw "source reinstall over existing managed files failed with exit code $LASTEXITCODE"
    }
    & (Join-Path $CommandDir "repogrammar.exe") version | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "reinstalled repogrammar version check failed"
    }

    # Default layout (no -CommandDir/-InstallDir) must resolve to the unified
    # authority: ~/.local/share/repogrammar/bin and ~/.local/bin (ADR-0014).
    $DefaultHome = Join-Path $TempRoot "default-home"
    New-Item -ItemType Directory -Force -Path $DefaultHome | Out-Null
    $SavedUserProfile = $env:USERPROFILE
    $SavedHome = $env:HOME
    $SavedXdg = $env:XDG_DATA_HOME
    $SavedEnvCommandDir = $env:REPOGRAMMAR_COMMAND_DIR
    $SavedEnvInstallDir = $env:REPOGRAMMAR_INSTALL_DIR
    $SavedEnvWorkerRoot = $env:REPOGRAMMAR_WORKER_ROOT
    try {
        $env:USERPROFILE = $DefaultHome
        $env:HOME = $DefaultHome
        Remove-Item Env:XDG_DATA_HOME -ErrorAction SilentlyContinue
        Remove-Item Env:REPOGRAMMAR_COMMAND_DIR -ErrorAction SilentlyContinue
        Remove-Item Env:REPOGRAMMAR_INSTALL_DIR -ErrorAction SilentlyContinue
        Remove-Item Env:REPOGRAMMAR_WORKER_ROOT -ErrorAction SilentlyContinue
        Invoke-InstallerWithPath (Join-Path $DefaultHome ".local\bin") {
            & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                -InstallCliOnly `
                -FromSource `
                -SourceBinary $SourceBinary `
                -Yes
        }
        if ($LASTEXITCODE -ne 0) {
            throw "default-layout install failed with exit code $LASTEXITCODE"
        }
        Assert-PathExists (Join-Path $DefaultHome ".local\share\repogrammar\bin\repogrammar.exe") "default install dir did not resolve to ~/.local/share/repogrammar/bin"
        Assert-PathExists (Join-Path $DefaultHome ".local\bin\repogrammar.exe") "default command dir did not resolve to ~/.local/bin"
    } finally {
        $env:USERPROFILE = $SavedUserProfile
        if ($null -ne $SavedHome) { $env:HOME = $SavedHome } else { Remove-Item Env:HOME -ErrorAction SilentlyContinue }
        if ($null -ne $SavedXdg) { $env:XDG_DATA_HOME = $SavedXdg } else { Remove-Item Env:XDG_DATA_HOME -ErrorAction SilentlyContinue }
        if ($null -ne $SavedEnvCommandDir) { $env:REPOGRAMMAR_COMMAND_DIR = $SavedEnvCommandDir } else { Remove-Item Env:REPOGRAMMAR_COMMAND_DIR -ErrorAction SilentlyContinue }
        if ($null -ne $SavedEnvInstallDir) { $env:REPOGRAMMAR_INSTALL_DIR = $SavedEnvInstallDir } else { Remove-Item Env:REPOGRAMMAR_INSTALL_DIR -ErrorAction SilentlyContinue }
        if ($null -ne $SavedEnvWorkerRoot) { $env:REPOGRAMMAR_WORKER_ROOT = $SavedEnvWorkerRoot } else { Remove-Item Env:REPOGRAMMAR_WORKER_ROOT -ErrorAction SilentlyContinue }
    }

    $DefaultBuildCommandDir = Join-Path $TempRoot "default-build-bin"
    $DefaultBuildInstallDir = Join-Path $TempRoot "default-build-data"
    $FakeCargoDir = Join-Path $TempRoot "fake-cargo"
    $CargoLog = Join-Path $TempRoot "cargo.log"
    $ReleaseDir = Join-Path $RepoRoot "target\release"
    $ReleaseBinary = Join-Path $ReleaseDir "repogrammar.exe"
    $ReleaseBackup = Join-Path $TempRoot "repogrammar.release.backup.exe"
    $ReleaseBinaryExisted = Test-Path $ReleaseBinary
    if ($ReleaseBinaryExisted) {
        Copy-Item $ReleaseBinary $ReleaseBackup -Force
    }
    try {
        New-Item -ItemType Directory -Force -Path $FakeCargoDir | Out-Null
        $FakeCargo = Join-Path $FakeCargoDir "cargo.cmd"
        $FakeCargoScript = @"
@echo off
echo %*>>"$CargoLog"
if /I "%1"=="build" (
  if /I "%2"=="--release" (
    if not exist "$ReleaseDir" mkdir "$ReleaseDir"
    copy /Y "$SourceBinary" "$ReleaseBinary" >NUL
    exit /B 0
  )
)
exit /B 1
"@
        Set-Content -Path $FakeCargo -Value $FakeCargoScript -Encoding ASCII
        Invoke-InstallerWithPath "$FakeCargoDir;$DefaultBuildCommandDir" {
            & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                -InstallCliOnly `
                -FromSource `
                -CommandDir $DefaultBuildCommandDir `
                -InstallDir $DefaultBuildInstallDir `
                -Yes
        }
        if ($LASTEXITCODE -ne 0) {
            throw "default source build install failed with exit code $LASTEXITCODE"
        }
        Assert-Contains $CargoLog "build --release"
        Assert-PathExists (Join-Path $DefaultBuildCommandDir "repogrammar.exe") "default source build command was not installed"
        Assert-PathExists (Join-Path $DefaultBuildInstallDir "bin\repogrammar.exe") "default source build managed binary was not installed"
        & (Join-Path $DefaultBuildCommandDir "repogrammar.exe") version | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "default source build repogrammar version check failed"
        }
    } finally {
        if ($ReleaseBinaryExisted) {
            Copy-Item $ReleaseBackup $ReleaseBinary -Force
        } else {
            Remove-Item $ReleaseBinary -Force -ErrorAction SilentlyContinue
        }
    }

    $StateRepo = Join-Path $TempRoot "state-boundary-repo"
    New-Item -ItemType Directory -Force -Path (Join-Path $StateRepo ".repogrammar") | Out-Null
    Set-Content -Path (Join-Path $StateRepo ".repogrammar\sentinel") -Value "keep"
    Push-Location $StateRepo
    try {
        Invoke-InstallerWithPath (Join-Path $TempRoot "state-bin") {
            & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                -InstallCliOnly `
                -FromSource `
                -SourceBinary $SourceBinary `
                -CommandDir (Join-Path $TempRoot "state-bin") `
                -InstallDir (Join-Path $TempRoot "state-data") `
                -Yes
        }
        if ($LASTEXITCODE -ne 0) {
            throw "source install from state repo failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
    Assert-Contains (Join-Path $StateRepo ".repogrammar\sentinel") "keep"

    $ForeignCommandDir = Join-Path $TempRoot "foreign-bin"
    $ForeignInstallDir = Join-Path $TempRoot "foreign-data"
    New-Item -ItemType Directory -Force -Path $ForeignCommandDir | Out-Null
    Set-Content -Path (Join-Path $ForeignCommandDir "repogrammar.exe") -Value "foreign"
    $ForeignRefusalOut = Join-Path $TempRoot "foreign-refusal.out"
    $SavedForeignErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        Invoke-InstallerWithPath $ForeignCommandDir {
            & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                -InstallCliOnly `
                -FromSource `
                -SourceBinary $SourceBinary `
                -CommandDir $ForeignCommandDir `
                -InstallDir $ForeignInstallDir `
                -Yes *> $ForeignRefusalOut
        }
        $ForeignRefusalStatus = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $SavedForeignErrorActionPreference
    }
    if ($ForeignRefusalStatus -eq 0) {
        throw "source install replaced an unmanaged command without explicit opt-in"
    }
    Assert-Contains $ForeignRefusalOut "pass -ReplaceUnmanagedCommand"
    Assert-Contains (Join-Path $ForeignCommandDir "repogrammar.exe") "foreign"
    $RefusedBackups = @(Get-ChildItem -Path $ForeignCommandDir -Filter "repogrammar.exe.unmanaged-backup*" -ErrorAction SilentlyContinue)
    if ($RefusedBackups.Count -ne 0 -or (Test-Path (Join-Path $ForeignInstallDir "bin\repogrammar.exe"))) {
        throw "refused unmanaged command replacement created backup or install state"
    }

    Invoke-InstallerWithPath $ForeignCommandDir {
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir $ForeignCommandDir `
            -InstallDir $ForeignInstallDir `
            -ReplaceUnmanagedCommand `
            -Yes
    }
    if ($LASTEXITCODE -ne 0) {
        throw "source install with unmanaged command backup failed with exit code $LASTEXITCODE"
    }
    $Backups = @(Get-ChildItem -Path $ForeignCommandDir -Filter "repogrammar.exe.unmanaged-backup*")
    if ($Backups.Count -ne 1) {
        throw "expected one unmanaged command backup, found $($Backups.Count)"
    }
    Assert-Contains $Backups[0].FullName "foreign"

    # -Verify reports build consistency by SHA256; -Prune removes only PATH copies
    # whose hash differs from the authority. The child runs with a PATH limited to
    # the test directories so it can never touch real repogrammar copies.
    $VerifyHome = Join-Path $TempRoot "verify-home"
    $VerifyCommandDir = Join-Path $TempRoot "verify-bin"
    $VerifyInstallDir = Join-Path $TempRoot "verify-data"
    $VerifyStaleDir = Join-Path $TempRoot "verify-stale"
    New-Item -ItemType Directory -Force -Path $VerifyHome, $VerifyStaleDir | Out-Null
    Invoke-InstallerWithPath $VerifyCommandDir {
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir $VerifyCommandDir `
            -InstallDir $VerifyInstallDir `
            -Yes | Out-Null
    }
    if ($LASTEXITCODE -ne 0) {
        throw "verify setup install failed with exit code $LASTEXITCODE"
    }
    Set-Content -Path (Join-Path $VerifyStaleDir "repogrammar.exe") -Value "stale-bytes"
    $AuthorityBin = Join-Path (Join-Path $VerifyInstallDir "bin") "repogrammar.exe"
    $CodexDir = Join-Path $VerifyHome ".codex"
    New-Item -ItemType Directory -Force -Path $CodexDir | Out-Null
    Set-Content -Path (Join-Path $CodexDir "config.toml") `
        -Value "[mcp_servers.repogrammar]`ncommand = '$AuthorityBin'`nargs = [`"serve`"]"
    $VerifyOut = Join-Path $TempRoot "verify.out"
    $SavedVerifyUserProfile = $env:USERPROFILE
    $SavedVerifyPath = $env:PATH
    try {
        $env:USERPROFILE = $VerifyHome
        $env:PATH = "$VerifyCommandDir;$VerifyStaleDir"
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -Verify `
            -Prune `
            -CommandDir $VerifyCommandDir `
            -InstallDir $VerifyInstallDir `
            -Yes *> $VerifyOut
        $VerifyStatus = $LASTEXITCODE
    } finally {
        $env:USERPROFILE = $SavedVerifyUserProfile
        $env:PATH = $SavedVerifyPath
    }
    if ($VerifyStatus -ne 0) {
        throw "verify run failed with exit code $VerifyStatus"
    }
    Assert-Contains $VerifyOut "matches authority"
    Assert-Contains $VerifyOut "OK: configured agents use the same build"
    if (Test-Path (Join-Path $VerifyStaleDir "repogrammar.exe")) {
        throw "stale PATH copy was not pruned"
    }
    Assert-PathExists (Join-Path $VerifyCommandDir "repogrammar.exe") "command copy must survive prune"

    if ([System.IO.Path]::DirectorySeparatorChar -eq '\') {
        $LockedPruneCommandDir = Join-Path $TempRoot "locked-prune-bin"
        $LockedPruneInstallDir = Join-Path $TempRoot "locked-prune-data"
        $LockedPruneStaleDir = Join-Path $TempRoot "locked-prune-stale"
        $LockedPruneOut = Join-Path $TempRoot "locked-prune.out"
        $LockedReady = Join-Path $TempRoot "locked-prune.ready"
        New-Item -ItemType Directory -Force -Path $LockedPruneStaleDir | Out-Null
        Invoke-InstallerWithPath $LockedPruneCommandDir {
            & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                -InstallCliOnly `
                -FromSource `
                -SourceBinary $SourceBinary `
                -CommandDir $LockedPruneCommandDir `
                -InstallDir $LockedPruneInstallDir `
                -Yes | Out-Null
        }
        if ($LASTEXITCODE -ne 0) {
            throw "locked-prune setup install failed with exit code $LASTEXITCODE"
        }
        $LockedStale = Join-Path $LockedPruneStaleDir "repogrammar.exe"
        Set-Content -Path $LockedStale -Value "stale-bytes"
        $LockJob = Start-Job -ScriptBlock {
            param([string]$Path, [string]$Ready)
            $stream = [System.IO.File]::Open(
                $Path,
                [System.IO.FileMode]::Open,
                [System.IO.FileAccess]::Read,
                [System.IO.FileShare]::Read
            )
            try {
                Set-Content -Path $Ready -Value "ready"
                Start-Sleep -Seconds 30
            } finally {
                $stream.Dispose()
            }
        } -ArgumentList $LockedStale, $LockedReady
        try {
            for ($i = 0; $i -lt 100 -and !(Test-Path $LockedReady); $i++) {
                Start-Sleep -Milliseconds 50
            }
            if (!(Test-Path $LockedReady)) {
                throw "locked-prune helper did not acquire the stale copy"
            }
            $SavedLockedPath = $env:PATH
            $PreviousLockedErrorActionPreference = $ErrorActionPreference
            try {
                $env:PATH = "$LockedPruneCommandDir;$LockedPruneStaleDir"
                $ErrorActionPreference = "Continue"
                & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                    -Verify `
                    -Prune `
                    -CommandDir $LockedPruneCommandDir `
                    -InstallDir $LockedPruneInstallDir `
                    -Yes *> $LockedPruneOut
                $LockedPruneStatus = $LASTEXITCODE
            } finally {
                $env:PATH = $SavedLockedPath
                $ErrorActionPreference = $PreviousLockedErrorActionPreference
            }
        } finally {
            if ($LockJob) {
                Stop-Job $LockJob -ErrorAction SilentlyContinue
                Wait-Job $LockJob -Timeout 5 -ErrorAction SilentlyContinue | Out-Null
                Remove-Job $LockJob -Force -ErrorAction SilentlyContinue
            }
        }
        if ($LockedPruneStatus -eq 0) {
            throw "locked stale PATH copy prune unexpectedly succeeded"
        }
        Assert-Contains $LockedPruneOut "Failed to remove"
        Assert-Contains $LockedPruneOut "failed to remove 1 stale PATH copy/copies"
        Assert-PathExists $LockedStale "failed prune should leave the locked stale copy in place"
    }

    # Normal install/update paths run the same stale PATH cleanup automatically.
    $AutoPruneCommandDir = Join-Path $TempRoot "auto-prune-bin"
    $AutoPruneInstallDir = Join-Path $TempRoot "auto-prune-data"
    $AutoPruneStaleDir = Join-Path $TempRoot "auto-prune-stale"
    $AutoPruneOut = Join-Path $TempRoot "auto-prune.out"
    New-Item -ItemType Directory -Force -Path $AutoPruneStaleDir | Out-Null
    Set-Content -Path (Join-Path $AutoPruneStaleDir "repogrammar.exe") -Value "stale-bytes"
    Invoke-InstallerWithPath "$AutoPruneCommandDir;$AutoPruneStaleDir" {
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir $AutoPruneCommandDir `
            -InstallDir $AutoPruneInstallDir `
            -Yes *> $AutoPruneOut
    }
    if ($LASTEXITCODE -ne 0) {
        throw "auto-prune install failed with exit code $LASTEXITCODE"
    }
    Assert-Contains $AutoPruneOut "Stale PATH copies"
    Assert-Contains $AutoPruneOut "Removed"
    if (Test-Path (Join-Path $AutoPruneStaleDir "repogrammar.exe")) {
        throw "install/update path did not automatically prune stale PATH copy"
    }
    Assert-PathExists (Join-Path $AutoPruneCommandDir "repogrammar.exe") "command copy must survive automatic prune"

    # -Purge removes binaries/workers/data under a fake home + restricted PATH, so
    # it can never touch the developer's real install or running processes.
    $PurgeHome = Join-Path $TempRoot "purge-home"
    $PurgeCommandDir = Join-Path $TempRoot "purge-bin"
    $PurgeInstallDir = Join-Path $TempRoot "purge-data"
    $PurgeExtraDir = Join-Path $TempRoot "purge-extra"
    New-Item -ItemType Directory -Force -Path $PurgeHome, $PurgeExtraDir | Out-Null
    Invoke-InstallerWithPath $PurgeCommandDir {
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir $PurgeCommandDir `
            -InstallDir $PurgeInstallDir `
            -Yes | Out-Null
    }
    if ($LASTEXITCODE -ne 0) {
        throw "purge setup install failed with exit code $LASTEXITCODE"
    }
    Set-Content -Path (Join-Path $PurgeExtraDir "repogrammar.exe") -Value "extra-copy"
    $PurgeOut = Join-Path $TempRoot "purge.out"
    $SavedPurgeUserProfile = $env:USERPROFILE
    $SavedPurgePath = $env:PATH
    try {
        $env:USERPROFILE = $PurgeHome
        $env:PATH = "$PurgeCommandDir;$PurgeExtraDir"
        & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -Purge `
            -CommandDir $PurgeCommandDir `
            -InstallDir $PurgeInstallDir `
            -Yes *> $PurgeOut
        $PurgeStatus = $LASTEXITCODE
    } finally {
        $env:USERPROFILE = $SavedPurgeUserProfile
        $env:PATH = $SavedPurgePath
    }
    if ($PurgeStatus -ne 0) {
        throw "purge run failed with exit code $PurgeStatus"
    }
    if (Test-Path $PurgeInstallDir) {
        throw "purge did not remove the install data directory"
    }
    if (Test-Path (Join-Path $PurgeCommandDir "repogrammar.exe")) {
        throw "purge did not remove the command binary"
    }
    if (Test-Path (Join-Path $PurgeExtraDir "repogrammar.exe")) {
        throw "purge did not remove the extra PATH copy"
    }

    $FailureCommandDir = Join-Path $TempRoot "failure-bin"
    $FailureInstallDir = Join-Path $TempRoot "failure-data"
    $FailureOut = Join-Path $TempRoot "failure.out"
    $PreviousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        Invoke-InstallerWithPath $FailureCommandDir {
            & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                -InstallAndConfigure `
                -FromSource `
                -SourceBinary $SourceBinary `
                -CommandDir $FailureCommandDir `
                -InstallDir $FailureInstallDir `
                -Target none `
                -Yes *> $FailureOut
        }
        $FailureStatus = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $PreviousErrorActionPreference
    }
    if ($FailureStatus -eq 0) {
        throw "expected delegated install failure to return nonzero"
    }
    Assert-Contains $FailureOut "repogrammar install failed with exit code"
} finally {
    if ($null -ne $SavedGlobalUserProfile) { $env:USERPROFILE = $SavedGlobalUserProfile } else { Remove-Item Env:USERPROFILE -ErrorAction SilentlyContinue }
    if ($null -ne $SavedGlobalHome) { $env:HOME = $SavedGlobalHome } else { Remove-Item Env:HOME -ErrorAction SilentlyContinue }
    Remove-Item -Recurse -Force $TempRoot -ErrorAction SilentlyContinue
}

# The final contract case intentionally invokes a failing delegated install.
# Do not leak that expected child status as the test process result after all
# assertions and cleanup have completed successfully.
Write-Host "PowerShell source-only installer contract passed"
exit 0
