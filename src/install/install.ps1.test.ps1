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
