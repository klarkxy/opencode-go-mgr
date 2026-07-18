param(
  [Parameter(Mandatory = $true)]
  [string]$CandidateInstaller,

  [Parameter(Mandatory = $true)]
  [string]$CandidateVersion,

  [Parameter(Mandatory = $true)]
  [string]$InstallDirectory,

  [Parameter(Mandatory = $true)]
  [string]$DataDirectory,

  [string]$PreviousInstaller = '',
  [string]$PreviousTag = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($CandidateVersion -notmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$') {
  throw "CandidateVersion must be stable semver, got: $CandidateVersion"
}
$CandidateInstaller = (Resolve-Path -LiteralPath $CandidateInstaller).Path
if ($PreviousInstaller) {
  $PreviousInstaller = (Resolve-Path -LiteralPath $PreviousInstaller).Path
}

function Wait-Dashboard {
  param(
    [string]$ExpectedVersion = '',
    [int]$Attempts = 60
  )
  foreach ($attempt in 1..$Attempts) {
    try {
      $html = (Invoke-WebRequest http://127.0.0.1:9042/dashboard/ -UseBasicParsing).Content
      if ($html -notmatch 'id="app"') { throw 'Dashboard HTML is incomplete' }
      if (!$ExpectedVersion) { return $null }
      $status = Invoke-RestMethod http://127.0.0.1:9042/dashboard/api/settings/update-status
      if ($status.current_version -eq $ExpectedVersion) { return $status }
    } catch {}
    Start-Sleep 1
  }
  throw "Installed GUI did not expose dashboard version $ExpectedVersion"
}

function Stop-InstalledGui {
  param([string]$ExecutablePath)
  if (!$ExecutablePath) { return }
  Get-Process -Name ocg-manager -ErrorAction SilentlyContinue | Where-Object {
    try { $_.Path -eq $ExecutablePath } catch { $false }
  } | ForEach-Object {
    Stop-Process -Id $_.Id -Force
    if (!$_.WaitForExit(30000)) { throw "GUI process $($_.Id) did not stop" }
  }
}

function Invoke-Installer {
  param(
    [string]$Path,
    [string[]]$Arguments,
    [string]$Label,
    [int]$TimeoutSeconds = 180
  )
  Write-Host "Starting $Label"
  # Wait only for the launched installer process. The updater /R path leaves a
  # restarted GUI running, and the uninstaller may hand work to a temporary
  # child. Their observable postconditions are checked separately below.
  $installerProcess = Start-Process $Path -ArgumentList $Arguments -PassThru -WindowStyle Hidden
  if (!$installerProcess.WaitForExit(1000 * $TimeoutSeconds)) {
    try {
      $installerProcess.Kill($true)
      $null = $installerProcess.WaitForExit(30000)
    } catch {
      Stop-Process -Id $installerProcess.Id -Force -ErrorAction SilentlyContinue
    }
    throw "$Label did not exit within $TimeoutSeconds seconds"
  }
  if ($installerProcess.ExitCode -ne 0) {
    throw "$Label failed: $($installerProcess.ExitCode)"
  }
  Write-Host "Completed $Label"
}

function Test-RegistryValue {
  param(
    [string]$Path,
    [string]$Name
  )
  try {
    $key = Get-Item -LiteralPath $Path -ErrorAction Stop
  } catch [System.Management.Automation.ItemNotFoundException] {
    return $false
  }
  return [bool]($key -and ($key.GetValueNames() -contains $Name))
}

function Wait-UninstallComplete {
  param(
    [string]$ExecutablePath,
    [string]$UninstallerPath,
    [string]$RunKey,
    [int]$Attempts = 90
  )
  foreach ($attempt in 1..$Attempts) {
    $startupEntryPresent = Test-RegistryValue -Path $RunKey -Name 'OCG Manager'
    if (
      !(Test-Path -LiteralPath $ExecutablePath) -and
      !(Test-Path -LiteralPath $UninstallerPath) -and
      !$startupEntryPresent
    ) {
      return
    }
    Start-Sleep 1
  }
  throw 'Candidate uninstall did not remove installed binaries within the timeout'
}

$installDir = [IO.Path]::GetFullPath($InstallDirectory)
$data = [IO.Path]::GetFullPath($DataDirectory)
if (Test-Path -LiteralPath $data) { throw "Hosted runner profile is not clean: $data" }
$sentinel = Join-Path $data preserve-me
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$settingsUrl = 'http://127.0.0.1:9042/dashboard/api/settings'
$bootstrapOverwrite = [bool]$PreviousInstaller
$sentinelValue = if ($PreviousTag) { "preserved-from-$PreviousTag" } else { 'preserved' }

$process = $null
$guiPath = ''
try {
  if ($bootstrapOverwrite) {
    $label = if ($PreviousTag) { "published $PreviousTag install" } else { 'previous release install' }
    Invoke-Installer -Path $PreviousInstaller -Arguments @('/S', "/D=$installDir") -Label $label
    $gui = Get-ChildItem $installDir -Recurse -Filter ocg-manager.exe | Select-Object -First 1
    if (!$gui) { throw 'Published GUI executable is missing' }
    $guiPath = $gui.FullName
    $process = Start-Process $guiPath -ArgumentList '--startup' -PassThru -WindowStyle Hidden
    Wait-Dashboard

    New-Item -ItemType Directory -Force $data | Out-Null
    Set-Content $sentinel $sentinelValue
    $settings = Invoke-RestMethod $settingsUrl
    $settings.auto_start = $true
    Invoke-RestMethod $settingsUrl -Method Post -ContentType 'application/json' -Body ($settings | ConvertTo-Json) | Out-Null
    $expectedStartupValue = "`"$guiPath`" --startup"
    $startupValue = (Get-ItemProperty -LiteralPath $runKey -Name 'OCG Manager').'OCG Manager'
    if ($startupValue -ne $expectedStartupValue) { throw "Published install wrote unexpected startup value: $startupValue" }

    $previousPid = $process.Id
    Invoke-Installer -Path $CandidateInstaller -Arguments @('/UPDATE', '/P', '/R', '/ARGS', '--startup') -Label 'candidate overwrite update'
    foreach ($attempt in 1..30) {
      if (!(Get-Process -Id $previousPid -ErrorAction SilentlyContinue)) { break }
      Start-Sleep 1
    }
    if (Get-Process -Id $previousPid -ErrorAction SilentlyContinue) {
      throw "Published GUI process $previousPid survived the overwrite update"
    }

    $updateStatus = Wait-Dashboard -ExpectedVersion $CandidateVersion -Attempts 90
    if ($updateStatus.current_version -ne $CandidateVersion) {
      throw "Unexpected updated GUI version: $($updateStatus.current_version)"
    }
    $updatedSettings = Invoke-RestMethod $settingsUrl
    if (!$updatedSettings.auto_start) { throw 'Overwrite update did not preserve the auto-start setting' }
    if ((Get-Content $sentinel -Raw).Trim() -ne $sentinelValue) {
      throw 'Overwrite update did not preserve the data sentinel'
    }
    $startupValue = (Get-ItemProperty -LiteralPath $runKey -Name 'OCG Manager').'OCG Manager'
    if ($startupValue -ne $expectedStartupValue) { throw "Overwrite update changed startup value: $startupValue" }
  } else {
    Invoke-Installer -Path $CandidateInstaller -Arguments @('/S', "/D=$installDir") -Label 'candidate install'
    $gui = Get-ChildItem $installDir -Recurse -Filter ocg-manager.exe | Select-Object -First 1
    if (!$gui) { throw 'Installed GUI executable is missing' }
    $guiPath = $gui.FullName
    $process = Start-Process $guiPath -ArgumentList '--startup' -PassThru -WindowStyle Hidden
    Wait-Dashboard -ExpectedVersion $CandidateVersion | Out-Null
    New-Item -ItemType Directory -Force $data | Out-Null
    Set-Content $sentinel $sentinelValue
  }

  $settings = Invoke-RestMethod $settingsUrl
  $settings.auto_start = $true
  Invoke-RestMethod $settingsUrl -Method Post -ContentType 'application/json' -Body ($settings | ConvertTo-Json) | Out-Null
  $startupValue = (Get-ItemProperty -LiteralPath $runKey -Name 'OCG Manager').'OCG Manager'
  $expectedStartupValue = "`"$guiPath`" --startup"
  if ($startupValue -ne $expectedStartupValue) { throw "Unexpected startup value: $startupValue" }

  $settings.auto_start = $false
  Invoke-RestMethod $settingsUrl -Method Post -ContentType 'application/json' -Body ($settings | ConvertTo-Json) | Out-Null
  if (Test-RegistryValue -Path $runKey -Name 'OCG Manager') {
    throw 'Disabling auto-start left the startup entry behind'
  }
  $settings.auto_start = $true
  Invoke-RestMethod $settingsUrl -Method Post -ContentType 'application/json' -Body ($settings | ConvertTo-Json) | Out-Null
  $startupValue = (Get-ItemProperty -LiteralPath $runKey -Name 'OCG Manager').'OCG Manager'
  if ($startupValue -ne $expectedStartupValue) { throw "Unexpected restored startup value: $startupValue" }
} finally {
  if ($process -and !$process.HasExited) {
    Stop-Process -Id $process.Id -Force
    if (!$process.WaitForExit(30000)) { throw "GUI process $($process.Id) did not stop" }
  }
  Stop-InstalledGui $guiPath
}

$uninstaller = Get-ChildItem $installDir -Recurse -Filter uninstall.exe | Select-Object -First 1
if (!$uninstaller) { throw 'Uninstaller is missing' }
Invoke-Installer -Path $uninstaller.FullName -Arguments @('/S') -Label 'candidate uninstall'
Wait-UninstallComplete -ExecutablePath $guiPath -UninstallerPath $uninstaller.FullName -RunKey $runKey
if (Test-RegistryValue -Path $runKey -Name 'OCG Manager') {
  throw 'Uninstall left the startup entry behind'
}
if (!(Test-Path $sentinel)) { throw 'Silent uninstall deleted user data' }

Write-Host "Windows release smoke passed for v$CandidateVersion."
