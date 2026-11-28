[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ManifestDir,
    [Parameter(Mandatory)][string]$Version
)

$ErrorActionPreference = 'Stop'
# Keep the GitHub request and retry logic testable on platforms without PowerShell.
& python (Join-Path $PSScriptRoot 'render.py') publish $Version $ManifestDir
exit $LASTEXITCODE
