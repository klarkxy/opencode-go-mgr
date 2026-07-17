param(
  [string]$WorkflowPath = (Join-Path $PSScriptRoot '..\.github\workflows\release.yml')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$resolvedWorkflow = (Resolve-Path -LiteralPath $WorkflowPath).Path
$workflow = (Get-Content -LiteralPath $resolvedWorkflow -Raw) -replace "`r`n", "`n"
$stepName = [regex]::Escape('Smoke installed GUI and overwrite bootstrap (Windows)')
$pattern = "(?m)^      - name: $stepName[\s\S]*?^        run: \|\r?\n(?<body>(?:(?:^          [^\r\n]*|^[ \t]*$)(?:\r?\n|$))+)"
$match = [regex]::Match($workflow, $pattern)
if (!$match.Success) {
  throw "Cannot extract the Windows release smoke script from $resolvedWorkflow"
}

$body = $match.Groups['body'].Value -replace '(?m)^          ', ''
if ($body.Length -lt 5000) {
  throw "Extracted Windows release smoke script is unexpectedly short: $($body.Length) characters"
}

$tokens = $null
$errors = $null
[void][System.Management.Automation.Language.Parser]::ParseInput(
  $body,
  [ref]$tokens,
  [ref]$errors
)
if ($errors.Count) {
  $messages = $errors | ForEach-Object {
    "line $($_.Extent.StartLineNumber): $($_.Message)"
  }
  throw "Windows release smoke PowerShell is invalid:`n$($messages -join "`n")"
}

Write-Host "Windows release smoke PowerShell parsed successfully ($($body.Length) characters)."
