[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$StageDir,
    [Parameter(Mandatory)][string]$Tag,
    [Parameter(Mandatory)][string]$OutputDir
)

$ErrorActionPreference = 'Stop'
$StageDir = (Resolve-Path -LiteralPath $StageDir).ProviderPath
$stage = Get-Content -LiteralPath (Join-Path $StageDir 'stage.json') -Raw | ConvertFrom-Json
if ($stage.slug -cnotin @('windows-x86_64', 'windows-arm64', 'windows-i686')) {
    throw 'Expected a Windows stage.'
}
if ($Tag -cnotmatch '^(?:v(?<version>[0-9]+\.[0-9]+\.[0-9]+)|(?:alpha|beta)-v(?<version>[0-9]+\.[0-9]+\.[0-9]+)-[A-Za-z0-9.]+)$' -or
    $Matches.version -cne $stage.version) {
    throw 'Tag must match the staged version.'
}

$name = "AirScout-RS-$Tag-$($stage.slug)"
$output = New-Item -ItemType Directory -Path $OutputDir -Force
$temp = [IO.Directory]::CreateTempSubdirectory('airscout-zip-')
try {
    $payload = New-Item -ItemType Directory -Path (Join-Path $temp.FullName $name)
    $files = @(
        'bin/airscout.exe', 'bin/airscout-planeserver.exe', 'bin/airscout-cli.exe',
        'bin/catcheck.exe', 'bin/elevation-tile-gen.exe', 'bin/maptile-gen.exe',
        'README.md', 'LICENSE', 'NOTICE'
    )
    foreach ($file in $files) {
        [IO.File]::Copy((Join-Path $StageDir $file), (Join-Path $payload.FullName (Split-Path -Leaf $file)))
    }
    $archive = Join-Path $temp.FullName "$name.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory(
        $payload.FullName, $archive, [IO.Compression.CompressionLevel]::Optimal, $true
    )
    Move-Item -LiteralPath $archive -Destination (Join-Path $output.FullName "$name.zip") -Force
} finally {
    Remove-Item -LiteralPath $temp.FullName -Recurse -Force
}
