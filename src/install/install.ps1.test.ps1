$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $ScriptDir "..\.."))
$Installer = Join-Path $ScriptDir "install.ps1"
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("repogrammar-install-ps-test-" + [guid]::NewGuid().ToString())

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

try {
    New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null
    Push-Location $RepoRoot
    try {
        & cargo build --quiet --bin repogrammar
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    $SourceBinary = Join-Path $RepoRoot "target\debug\repogrammar.exe"
    Assert-PathExists $SourceBinary "source binary was not built"

    $CommandDir = Join-Path $TempRoot "bin"
    $InstallDir = Join-Path $TempRoot "data"
    & powershell -ExecutionPolicy Bypass -File $Installer `
        -InstallCliOnly `
        -FromSource `
        -SourceBinary $SourceBinary `
        -CommandDir $CommandDir `
        -InstallDir $InstallDir `
        -Yes
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
    & powershell -ExecutionPolicy Bypass -File $Installer `
        -InstallCliOnly `
        -FromSource `
        -SourceBinary $SourceBinary `
        -CommandDir $CommandDir `
        -InstallDir $InstallDir `
        -Yes
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
        & powershell -NoProfile -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -Yes
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
        $PowerShellExe = (Get-Command powershell).Source
        $PreviousPath = $env:PATH
        try {
            $env:PATH = $FakeCargoDir
            & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installer `
                -InstallCliOnly `
                -FromSource `
                -CommandDir $DefaultBuildCommandDir `
                -InstallDir $DefaultBuildInstallDir `
                -Yes
            if ($LASTEXITCODE -ne 0) {
                throw "default source build install failed with exit code $LASTEXITCODE"
            }
        } finally {
            $env:PATH = $PreviousPath
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
        & powershell -ExecutionPolicy Bypass -File $Installer `
            -InstallCliOnly `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir (Join-Path $TempRoot "state-bin") `
            -InstallDir (Join-Path $TempRoot "state-data") `
            -Yes
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
    & powershell -ExecutionPolicy Bypass -File $Installer `
        -InstallCliOnly `
        -FromSource `
        -SourceBinary $SourceBinary `
        -CommandDir $ForeignCommandDir `
        -InstallDir $ForeignInstallDir `
        -Yes
    if ($LASTEXITCODE -ne 0) {
        throw "source install with unmanaged command backup failed with exit code $LASTEXITCODE"
    }
    $Backups = @(Get-ChildItem -Path $ForeignCommandDir -Filter "repogrammar.exe.unmanaged-backup*")
    if ($Backups.Count -ne 1) {
        throw "expected one unmanaged command backup, found $($Backups.Count)"
    }
    Assert-Contains $Backups[0].FullName "foreign"

    $FailureCommandDir = Join-Path $TempRoot "failure-bin"
    $FailureInstallDir = Join-Path $TempRoot "failure-data"
    $FailureOut = Join-Path $TempRoot "failure.out"
    $PreviousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & powershell -ExecutionPolicy Bypass -File $Installer `
            -InstallAndConfigure `
            -FromSource `
            -SourceBinary $SourceBinary `
            -CommandDir $FailureCommandDir `
            -InstallDir $FailureInstallDir `
            -Target none `
            -Yes *> $FailureOut
        $FailureStatus = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $PreviousErrorActionPreference
    }
    if ($FailureStatus -eq 0) {
        throw "expected delegated install failure to return nonzero"
    }
    Assert-Contains $FailureOut "repogrammar install failed with exit code"
} finally {
    Remove-Item -Recurse -Force $TempRoot -ErrorAction SilentlyContinue
}
