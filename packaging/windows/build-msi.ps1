[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$StageDir,
    [Parameter(Mandatory)][string]$Tag,
    [Parameter(Mandatory)][ValidateSet('x64', 'arm64', 'x86')][string]$Architecture,
    [Parameter(Mandatory)][string]$OutputDir
)

$ErrorActionPreference = 'Stop'
$StageDir = (Resolve-Path -LiteralPath $StageDir).ProviderPath
$stage = Get-Content -LiteralPath (Join-Path $StageDir 'stage.json') -Raw | ConvertFrom-Json
$slug = @{ x64 = 'windows-x86_64'; arm64 = 'windows-arm64'; x86 = 'windows-i686' }[$Architecture]
$target = @{ x64 = 'x86_64'; arm64 = 'aarch64'; x86 = 'i686' }[$Architecture] + '-pc-windows-msvc'
if ($stage.slug -cne $slug -or $stage.target -cne $target) {
    throw "Stage must contain $target ($slug) binaries."
}
if ($Tag -cnotmatch '^(?:v(?<version>[0-9]+\.[0-9]+\.[0-9]+)|(?:alpha|beta)-v(?<version>[0-9]+\.[0-9]+\.[0-9]+)-[A-Za-z0-9.]+)$' -or
    $Matches.version -cne $stage.version) {
    throw 'Tag must match the staged version.'
}
$version = [version]::Parse($stage.version)
if ($version.Major -gt 255 -or $version.Minor -gt 255 -or $version.Build -gt 65535) {
    throw 'MSI ProductVersion exceeds the limits 255.255.65535.'
}

# Keep the MSI product identity stable for rebuilds of the same tag and architecture.
$identity = [Text.Encoding]::UTF8.GetBytes("AirScout-RS MSI/${Tag}/$slug")
$digest = [Security.Cryptography.SHA256]::HashData($identity)
$productCode = [Guid]::new([byte[]]($digest[0..15])).ToString('B').ToUpperInvariant()

$output = New-Item -ItemType Directory -Path $OutputDir -Force
$asset = Join-Path $output.FullName "AirScout-RS-$Tag-$slug.msi"
$wix = if ($env:WIX_EXE) { $env:WIX_EXE } else { 'wix' }
& $wix build -arch $Architecture -define "StageDir=$StageDir" `
    -define "ProductVersion=$version" -define "ProductCode=$productCode" `
    -pdbtype none -out $asset (Join-Path $PSScriptRoot 'AirScoutRS.wxs')
if ($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $asset -PathType Leaf)) {
    throw "WiX failed to build $asset (exit code $LASTEXITCODE)."
}
