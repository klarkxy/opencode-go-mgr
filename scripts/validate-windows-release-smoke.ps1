param(
  [string]$WorkflowPath = (Join-Path $PSScriptRoot '..\.github\workflows\release.yml'),
  [string]$ScriptPath = (Join-Path $PSScriptRoot 'smoke-windows-release.ps1')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$resolvedWorkflow = (Resolve-Path -LiteralPath $WorkflowPath).Path
$resolvedScript = (Resolve-Path -LiteralPath $ScriptPath).Path
$workflow = Get-Content -LiteralPath $resolvedWorkflow -Raw
$script = Get-Content -LiteralPath $resolvedScript -Raw

$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
  $resolvedScript,
  [ref]$tokens,
  [ref]$errors
)
if ($errors.Count) {
  $messages = $errors | ForEach-Object {
    "line $($_.Extent.StartLineNumber): $($_.Message)"
  }
  throw "Windows release smoke PowerShell is invalid:`n$($messages -join "`n")"
}

if ($workflow -notmatch '\.\/scripts\/smoke-windows-release\.ps1\s+@parameters') {
  throw "Release workflow does not invoke $resolvedScript"
}
if ($workflow -match 'function\s+Invoke-Installer') {
  throw 'Release workflow still embeds the Windows installer implementation'
}
foreach ($requiredPattern in @(
  '\.WaitForExit\(1000 \* \$TimeoutSeconds\)',
  '\.Kill\(\$true\)',
  'Test-RegistryValue',
  'Wait-UninstallComplete',
  'Overwrite update did not preserve the auto-start setting'
)) {
  if ($script -notmatch $requiredPattern) {
    throw "Windows release smoke is missing required behavior: $requiredPattern"
  }
}
if ($script -match '\.PSObject\.Properties\.Name') {
  throw 'Windows release smoke must handle an empty registry value collection under StrictMode'
}
$registryHelper = $ast.Find({
  param($node)
  $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
    $node.Name -eq 'Test-RegistryValue'
}, $true)
if (!$registryHelper) {
  throw 'Windows release smoke is missing Test-RegistryValue'
}
& {
  param([string]$Definition)
  Invoke-Expression $Definition

  function Get-Item {
    [CmdletBinding()]
    param([string]$LiteralPath)
    if ($script:registryProbeMode -eq 'missing') {
      throw [System.Management.Automation.ItemNotFoundException]::new('missing registry key')
    }
    if ($script:registryProbeMode -eq 'denied') {
      throw [System.Security.SecurityException]::new('registry access denied')
    }
    $key = [pscustomobject]@{ ValueNames = @($script:registryProbeValues) }
    $key | Add-Member -MemberType ScriptMethod -Name GetValueNames -Value { @($this.ValueNames) }
    return $key
  }

  $script:registryProbeMode = 'present'
  $script:registryProbeValues = @()
  if (Test-RegistryValue -Path 'mock:' -Name 'OCG Manager') {
    throw 'An empty registry key was reported as containing the startup value'
  }
  $script:registryProbeValues = @('OCG Manager')
  if (!(Test-RegistryValue -Path 'mock:' -Name 'OCG Manager')) {
    throw 'The startup registry value was not detected'
  }
  $script:registryProbeMode = 'missing'
  if (Test-RegistryValue -Path 'mock:' -Name 'OCG Manager') {
    throw 'A missing registry key was reported as containing the startup value'
  }
  $script:registryProbeMode = 'denied'
  $accessDenied = $false
  try {
    Test-RegistryValue -Path 'mock:' -Name 'OCG Manager' | Out-Null
  } catch [System.Security.SecurityException] {
    $accessDenied = $true
  }
  if (!$accessDenied) {
    throw 'Registry access errors must fail the Windows release smoke'
  }
} $registryHelper.Extent.Text
if ($workflow -notmatch '\$env:USERPROFILE') {
  throw 'Release workflow must pass the real runner profile data directory'
}
if ($script -match '\$env:(USERPROFILE|HOME)\s*=') {
  throw 'Windows release smoke must not override the real runner profile'
}

Write-Host "Windows release smoke parsed successfully ($($script.Length) characters)."
