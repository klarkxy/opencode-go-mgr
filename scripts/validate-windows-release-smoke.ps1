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
[void][System.Management.Automation.Language.Parser]::ParseFile(
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
  'Wait-UninstallComplete',
  'Overwrite update did not preserve the auto-start setting'
)) {
  if ($script -notmatch $requiredPattern) {
    throw "Windows release smoke is missing required behavior: $requiredPattern"
  }
}
if ($workflow -notmatch '\$env:USERPROFILE') {
  throw 'Release workflow must pass the real runner profile data directory'
}
if ($script -match '\$env:(USERPROFILE|HOME)\s*=') {
  throw 'Windows release smoke must not override the real runner profile'
}

Write-Host "Windows release smoke parsed successfully ($($script.Length) characters)."
