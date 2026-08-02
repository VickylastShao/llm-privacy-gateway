[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,

    [Parameter(Mandatory = $true)]
    [string]$ExpectedSha256,

    [Parameter(Mandatory = $true)]
    [string]$AppDataDirectoryName,

    [switch]$RequireSigned,

    [switch]$ConfirmCleanMachine
)

$ErrorActionPreference = "Stop"

if (-not $ConfirmCleanMachine) {
    throw "Pass -ConfirmCleanMachine only in a disposable VM or clean Windows account."
}

$installer = [IO.Path]::GetFullPath($InstallerPath)
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
    throw "Installer does not exist: $installer"
}

$expected = $ExpectedSha256.Trim().ToLowerInvariant()
if ($expected -notmatch "^[0-9a-f]{64}$") {
    throw "ExpectedSha256 must contain exactly 64 hexadecimal characters."
}
$actual = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) {
    throw "Installer SHA-256 mismatch. Expected $expected, received $actual."
}

function Assert-ValidSignature([string]$Path) {
    $signature = Get-AuthenticodeSignature -FilePath $Path
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "Authenticode verification failed for ${Path}: $($signature.Status)"
    }
}

if ($RequireSigned) {
    Assert-ValidSignature $installer
}

$installDirectory = Join-Path $env:LOCALAPPDATA "LLM Privacy Gateway"
$installedExecutable = Join-Path $installDirectory "lpg-desktop.exe"
$uninstaller = Join-Path $installDirectory "uninstall.exe"
$appDataDirectory = Join-Path $env:APPDATA $AppDataDirectoryName
$database = Join-Path $appDataDirectory "profiles.sqlite"
$startMenuRoot = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"

function Get-LpgUninstallRecord {
    Get-ChildItem "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall" `
        -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.DisplayName -eq "LLM Privacy Gateway" }
}

if ((Get-LpgUninstallRecord) -or (Test-Path -LiteralPath $installDirectory) -or (Test-Path -LiteralPath $appDataDirectory)) {
    throw "The target is not clean: an installation, uninstall record, or application-data directory already exists."
}

$install = Start-Process -FilePath $installer -ArgumentList "/S" -PassThru -Wait
if ($install.ExitCode -ne 0) {
    throw "Installer failed with exit code $($install.ExitCode)."
}

$installDeadline = [DateTime]::UtcNow.AddSeconds(20)
while ((-not (Test-Path -LiteralPath $installedExecutable)) -and [DateTime]::UtcNow -lt $installDeadline) {
    Start-Sleep -Milliseconds 250
}
if (-not (Test-Path -LiteralPath $installedExecutable)) {
    throw "Installed executable was not created."
}
if (-not (Test-Path -LiteralPath $uninstaller)) {
    throw "Uninstaller was not created."
}
if ($RequireSigned) {
    Assert-ValidSignature $installedExecutable
}

$shortcut = Get-ChildItem -LiteralPath $startMenuRoot -Filter "LLM Privacy Gateway.lnk" `
    -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $shortcut) {
    throw "Start Menu shortcut was not created."
}

$process = Start-Process -FilePath $installedExecutable -PassThru
$windowDeadline = [DateTime]::UtcNow.AddSeconds(20)
do {
    Start-Sleep -Milliseconds 250
    $process.Refresh()
} while ($process.MainWindowHandle -eq [IntPtr]::Zero -and -not $process.HasExited -and [DateTime]::UtcNow -lt $windowDeadline)

if ($process.HasExited) {
    throw "Application exited during first launch with code $($process.ExitCode)."
}
if ($process.MainWindowHandle -eq [IntPtr]::Zero) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "Application did not create a visible main window."
}

if (-not $process.CloseMainWindow()) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "Application did not accept a normal close request."
}
try {
    Wait-Process -Id $process.Id -Timeout 15 -ErrorAction Stop
} catch {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "Application did not exit after closing while protection was stopped."
}

if (-not (Test-Path -LiteralPath $database)) {
    throw "First launch did not create the local profile database."
}
$databaseHashBeforeUninstall = (Get-FileHash -LiteralPath $database -Algorithm SHA256).Hash.ToLowerInvariant()

$uninstall = Start-Process -FilePath $uninstaller -ArgumentList "/S" -PassThru -Wait
if ($uninstall.ExitCode -ne 0) {
    throw "Uninstaller failed with exit code $($uninstall.ExitCode)."
}
$uninstallDeadline = [DateTime]::UtcNow.AddSeconds(20)
while ((Test-Path -LiteralPath $installDirectory) -and [DateTime]::UtcNow -lt $uninstallDeadline) {
    Start-Sleep -Milliseconds 250
}
if (Test-Path -LiteralPath $installDirectory) {
    throw "Install directory remained after uninstall."
}
if (Get-LpgUninstallRecord) {
    throw "Uninstall record remained after uninstall."
}
if (Test-Path -LiteralPath $shortcut.FullName) {
    throw "Start Menu shortcut remained after uninstall."
}
if (-not (Test-Path -LiteralPath $database)) {
    throw "Uninstall removed the local profile database."
}
$databaseHashAfterUninstall = (Get-FileHash -LiteralPath $database -Algorithm SHA256).Hash.ToLowerInvariant()
if ($databaseHashAfterUninstall -ne $databaseHashBeforeUninstall) {
    throw "Uninstall changed the local profile database."
}

[pscustomobject]@{
    Status = "PASS"
    InstallerSha256 = $actual
    InstallerSigned = (Get-AuthenticodeSignature -FilePath $installer).Status.ToString()
    GuiWindowCreated = $true
    StoppedWindowExited = $true
    StartMenuShortcutCreated = $true
    UninstallPreservedDatabase = $true
    AppDataDirectory = $appDataDirectory
} | ConvertTo-Json -Compress
