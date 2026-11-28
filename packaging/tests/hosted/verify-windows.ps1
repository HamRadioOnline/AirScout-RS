[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Asset,
    [Parameter(Mandatory)][string]$Version,
    [string]$UpgradeFixture
)

$ErrorActionPreference = 'Stop'
$Asset = (Resolve-Path -LiteralPath $Asset).ProviderPath
if ([IO.Path]::GetFileName($Asset) -cnotmatch '^AirScout-RS-(?<tag>.+)-(?<slug>windows-(?:x86_64|arm64|i686))\.(?<format>msi|zip)$') {
    throw 'Unexpected Windows asset name'
}
$slug = $Matches.slug
$tag = $Matches.tag
$format = $Matches.format
$machine = @{ 'windows-x86_64' = 0x8664; 'windows-arm64' = 0xaa64; 'windows-i686' = 0x14c }[$slug]
$tools = 'airscout', 'airscout-planeserver', 'airscout-cli', 'catcheck', 'elevation-tile-gen', 'maptile-gen'
if ([string]$env:WINDOWS_SIGNING -cnotin 'verified', 'missing', '') {
    throw 'Unexpected WINDOWS_SIGNING value'
}
$signed = $env:WINDOWS_SIGNING -ceq 'verified'
if (!$signed) { Write-Host 'Certum credentials unavailable; checking unsigned Windows package' }
$temp = [IO.Directory]::CreateTempSubdirectory('airscout-verify-')

function Assert-Signature($File) {
    if ($signed -and (Get-AuthenticodeSignature -FilePath $File).Status -ne 'Valid') {
        throw "Authenticode verification failed for $File"
    }
}

if (-not ('AirScout.NativeResources' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace AirScout {
    public static class NativeResources {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
        private static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
        private static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
        [DllImport("kernel32.dll")]
        private static extern bool FreeLibrary(IntPtr module);

        public static bool HasEmbeddedIcon(string path) {
            // Map the file as a resource image without running any code.
            IntPtr module = LoadLibraryEx(path, IntPtr.Zero, 0x22);
            if (module == IntPtr.Zero) return false;
            try {
                // Both GUI builds specify resource ID 1, type RT_GROUP_ICON (14).
                return FindResource(module, (IntPtr)1, (IntPtr)14) != IntPtr.Zero;
            } finally {
                FreeLibrary(module);
            }
        }
    }
}
'@
}

function Assert-Executables($BinDir) {
    foreach ($tool in $tools) {
        $file = Join-Path $BinDir "$tool.exe"
        Assert-Signature $file
        $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($file))
        try {
            if ($reader.ReadUInt16() -ne 0x5a4d) { throw "Missing DOS header in $file" }
            $reader.BaseStream.Position = 0x3c
            $pe = $reader.ReadUInt32()
            $reader.BaseStream.Position = $pe
            if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne $machine) {
                throw "Unexpected PE architecture in $file"
            }
            if ($tool -in 'airscout', 'airscout-planeserver' -and
                ![AirScout.NativeResources]::HasEmbeddedIcon($file)) {
                throw "Missing embedded GUI icon in $file"
            }
        } finally {
            $reader.Dispose()
        }
    }
    & python (Join-Path $PSScriptRoot 'smoke-stage.py') $BinDir $Version
    if ($LASTEXITCODE -ne 0) { throw "Packaged Windows command smoke failed in $BinDir" }
}

function Invoke-Msi($Operation, $Target, [switch]$AllowMissing) {
    $log = Join-Path $temp.FullName ("msi-" + [Guid]::NewGuid() + ".log")
    $args = "$Operation `"$Target`" /qn /norestart /l*v `"$log`""
    $process = Start-Process msiexec.exe -ArgumentList $args -PassThru
    if (!$process.WaitForExit(180000)) {
        $process.Kill()
        throw "Windows Installer timed out for $Operation"
    }
    if ($process.ExitCode -ne 0 -and !($AllowMissing -and $process.ExitCode -eq 1605)) {
        Get-Content -LiteralPath $log -Tail 45
        throw "Windows Installer returned $($process.ExitCode) for $Operation"
    }
}

function Get-ProductCode($ReleaseTag) {
    $bytes = [Text.Encoding]::UTF8.GetBytes("AirScout-RS MSI/$ReleaseTag/$slug")
    $hash = [Security.Cryptography.SHA256]::HashData($bytes)
    return [Guid]::new([byte[]]($hash[0..15])).ToString('B').ToUpperInvariant()
}

function Test-Product($Code) {
    $key = 'HKLM:/SOFTWARE/Microsoft/Windows/CurrentVersion/Uninstall'
    $wow = 'HKLM:/SOFTWARE/WOW6432Node/Microsoft/Windows/CurrentVersion/Uninstall'
    return (Test-Path (Join-Path $key $Code)) -or (Test-Path (Join-Path $wow $Code))
}

function Test-MachinePath {
    $paths = [Environment]::GetEnvironmentVariable('Path', 'Machine').Split(';')
    return @($paths | Where-Object { $_.TrimEnd('\') -eq $installDir.TrimEnd('\') }).Count -gt 0
}

try {
    if ($format -eq 'zip') {
        Expand-Archive -LiteralPath $Asset -DestinationPath $temp.FullName
        Assert-Executables (Join-Path $temp.FullName ([IO.Path]::GetFileNameWithoutExtension($Asset)))
        return
    }

    Assert-Signature $Asset
    $programFiles = if ($slug -eq 'windows-i686') { ${env:ProgramFiles(x86)} } else { $env:ProgramFiles }
    $installDir = Join-Path $programFiles 'AirScout-RS'
    $shortcut = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'AirScout-RS.lnk'
    $candidate = Get-ProductCode $tag
    $toClean = [Collections.Generic.List[string]]::new()
    $toClean.Add($candidate)
    try {
        if ($UpgradeFixture) {
            $UpgradeFixture = (Resolve-Path -LiteralPath $UpgradeFixture).ProviderPath
            $betaTag = "beta-v$Version-smoke"
            $finalTag = "v$Version"
            $beta = Get-ProductCode $betaTag
            $final = Get-ProductCode $finalTag
            if ($tag -ceq $finalTag) {
                $betaAsset = $UpgradeFixture
                $finalAsset = $Asset
                $toClean.Add($beta)
            } elseif ($tag -ceq $betaTag -or $tag.StartsWith("beta-v$Version-")) {
                $beta = $candidate
                $betaAsset = $Asset
                $finalAsset = $UpgradeFixture
                $toClean.Add($final)
            } else {
                throw 'Upgrade requires a beta or stable candidate'
            }
            Invoke-Msi '/i' $betaAsset
            if (!(Test-Product $beta)) { throw 'The beta product did not register' }
            Invoke-Msi '/i' $finalAsset
            if ((Test-Product $beta) -or !(Test-Product $final)) {
                throw 'The final MSI did not replace the beta at the same version'
            }
            $installed = $final
        } else {
            Invoke-Msi '/i' $Asset
            $installed = $candidate
        }

        if (!(Test-Product $installed) -or !(Test-MachinePath) -or !(Test-Path -LiteralPath $shortcut -PathType Leaf)) {
            throw 'MSI failed to register the product, machine PATH or Start Menu shortcut'
        }
        # Machine PATH changes are visible to new processes after refresh.
        $previousPath = $env:Path
        try {
            $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine')
            $resolved = Get-Command 'airscout.exe' -CommandType Application
            if ($resolved.Source -ne (Join-Path $installDir 'airscout.exe')) {
                throw 'Machine PATH does not resolve the installed executable'
            }
        } finally {
            $env:Path = $previousPath
        }
        Assert-Executables $installDir
        Invoke-Msi '/x' $installed
        if ((Test-Product $installed) -or (Test-MachinePath) -or
            (Test-Path -LiteralPath $shortcut) -or (Test-Path (Join-Path $installDir 'airscout.exe'))) {
            throw 'MSI uninstall left the product, executable, PATH or shortcut in place'
        }
    } finally {
        foreach ($code in $toClean) {
            if (Test-Product $code) { Invoke-Msi '/x' $code -AllowMissing }
        }
    }
} finally {
    $temp.Delete($true)
}
