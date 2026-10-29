# Reads a Release publish, or the MSI's image of one, for CI and the release, and runs none of it.
# CONTRIBUTING.md lists what it refuses.
param(
    [Parameter(Mandatory)] [string] $Folder,
    [Parameter(Mandatory)] [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')] [string] $Triple,
    # pitboard_ffi.dll as Cargo built it in this environment, which the publish must carry unchanged.
    [Parameter(Mandatory)] [string] $GatedLibrary,
    # The subject of the certificate that signs PitboardShare.msix, where the folder holds one.
    [string] $Signer,
    # The Windows update public key the build was given in WINDOWS_UPDATE_PUBLIC_KEY.
    [string] $UpdateKey,
    # The version Cargo.toml names, which the app's build was given too.
    [Parameter(Mandatory)] [string] $Version,
    # The FileVersion windows-versions.sh gives that version.
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')] [string] $FileVersion
)
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path -LiteralPath $Folder).Path
$exe = Join-Path $root 'Pitboard.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "no Pitboard.exe in $root" }
$gated = (Resolve-Path -LiteralPath $GatedLibrary).Path

# no-fixture-text.sh's words, as UTF-8 in native code and as UTF-16 in a .NET assembly's strings.
$fixtureText = 'A Pitboard fixture page', 'dana@work.example', 'PITBOARD_TEST_CLAUDE_DESKTOP'
$programs = @(Get-ChildItem -LiteralPath $root -Recurse -File -Include '*.exe', '*.dll')
foreach ($program in $programs) {
    $bytes = [IO.File]::ReadAllBytes($program.FullName)
    $narrow = [Text.Encoding]::Latin1.GetString($bytes)
    $wide = [Text.Encoding]::Unicode.GetString($bytes) + [Text.Encoding]::Unicode.GetString($bytes, 1, $bytes.Length - 1)
    foreach ($text in $fixtureText) {
        if ($narrow.Contains($text) -or $wide.Contains($text)) {
            throw "$($program.Name) holds a fixture's text, '$text': a Release build has none"
        }
    }
}
# The model reads PITBOARD_FIXTURE, and only a Debug build of the app reads a fixture's own variables.
foreach ($assembly in 'Pitboard.dll', 'Pitboard.Windows.dll') {
    $bytes = [IO.File]::ReadAllBytes((Join-Path $root $assembly))
    if ([Text.Encoding]::Unicode.GetString($bytes).Contains('PITBOARD_FIXTURE')) {
        throw "$assembly reads a fixture's variables: it was built as Debug"
    }
}
Write-Output "None of the $($programs.Count) programs and libraries holds a fixture."

# Only PitboardShare.exe carries a package identity, through an <msix> element in its manifest.
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class PitboardManifest
{
    [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32", SetLastError = true)]
    private static extern IntPtr FindResourceW(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32", SetLastError = true)]
    private static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32", SetLastError = true)]
    private static extern IntPtr LockResource(IntPtr data);
    [DllImport("kernel32", SetLastError = true)]
    private static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32", SetLastError = true)]
    private static extern bool FreeLibrary(IntPtr module);

    // Each RT_MANIFEST resource by the ids Windows reserves for them, 1 to 16, keyed by id, of a
    // program mapped with LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE: read, never run.
    public static Dictionary<int, string> Of(string path)
    {
        IntPtr module = LoadLibraryExW(path, IntPtr.Zero, 0x2 | 0x20);
        if (module == IntPtr.Zero) throw new Win32Exception();
        try
        {
            var manifests = new Dictionary<int, string>();
            for (int id = 1; id <= 16; id++)
            {
                IntPtr found = FindResourceW(module, (IntPtr)id, (IntPtr)24);
                if (found == IntPtr.Zero) continue;
                IntPtr data = LockResource(LoadResource(module, found));
                byte[] bytes = new byte[SizeofResource(module, found)];
                Marshal.Copy(data, bytes, 0, bytes.Length);
                manifests[id] = Encoding.UTF8.GetString(bytes).TrimStart('\uFEFF');
            }
            return manifests;
        }
        finally
        {
            FreeLibrary(module);
        }
    }
}
'@
$share = Join-Path $root 'PitboardShare.exe'
$manifests = @{}
foreach ($program in $programs) {
    $manifests[$program.FullName] = [PitboardManifest]::Of($program.FullName)
    if ($program.FullName -eq $share) { continue }
    foreach ($manifest in $manifests[$program.FullName].Values) {
        if ($manifest -match '<(\w+:)?msix\b') {
            Write-Output $manifest
            throw "$($program.Name) asks for a package identity: only PitboardShare.exe may carry one"
        }
    }
}
# CREATEPROCESS_MANIFEST_RESOURCE_ID
if (-not $manifests[$exe].ContainsKey(1)) { throw 'Pitboard.exe has no manifest: app.manifest did not reach it' }
Write-Output 'No program but PitboardShare.exe carries a package identity.'

$package = Join-Path $root 'PitboardShare.msix'
if (Test-Path -LiteralPath $share -PathType Leaf) {
    if (-not $Signer) { throw 'PitboardShare.exe is here, so -Signer must name the subject that signs its package' }
    $element = $null
    if ($manifests[$share].ContainsKey(1)) {
        $element = ([xml] $manifests[$share][1]).GetElementsByTagName('msix', 'urn:schemas-microsoft-com:msix.v1') | Select-Object -First 1
    }
    if (-not $element) { throw 'PitboardShare.exe has no <msix> element in its manifest: it would run without its package identity' }
    if (-not (Test-Path -LiteralPath $package -PathType Leaf)) { throw 'no PitboardShare.msix beside PitboardShare.exe' }
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.ZipFile
    $zip = [IO.Compression.ZipFile]::OpenRead($package)
    try {
        $entry = $zip.GetEntry('AppxManifest.xml')
        if (-not $entry) { throw 'PitboardShare.msix holds no AppxManifest.xml' }
        $reader = [IO.StreamReader]::new($entry.Open())
        try { $packaged = [xml] $reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally {
        $zip.Dispose()
    }
    $names = [Xml.XmlNamespaceManager]::new($packaged.NameTable)
    $names.AddNamespace('m', 'http://schemas.microsoft.com/appx/manifest/foundation/windows10')
    $identity = $packaged.SelectSingleNode('/m:Package/m:Identity', $names)
    $applications = @($packaged.SelectNodes('/m:Package/m:Applications/m:Application', $names))
    if ($identity.GetAttribute('Publisher') -cne $Signer) {
        throw "PitboardShare.msix's Publisher is '$($identity.GetAttribute('Publisher'))', not the signer's subject, '$Signer'"
    }
    if ($applications.Count -ne 1 -or $applications[0].GetAttribute('Executable') -cne 'PitboardShare.exe') {
        throw 'PitboardShare.msix must hold one application, PitboardShare.exe'
    }
    $expected = [ordered]@{
        publisher = $identity.GetAttribute('Publisher')
        packageName = $identity.GetAttribute('Name')
        applicationId = $applications[0].GetAttribute('Id')
    }
    foreach ($name in $expected.Keys) {
        if ($element.GetAttribute($name) -cne $expected[$name]) {
            throw "PitboardShare.exe's <msix> $name is '$($element.GetAttribute($name))', and its package's is '$($expected[$name])'"
        }
    }
    Write-Output "PitboardShare.exe and PitboardShare.msix name one identity, $($expected.packageName)!$($expected.applicationId), published by $Signer."
} elseif (Test-Path -LiteralPath $package) {
    throw 'PitboardShare.msix is here without PitboardShare.exe'
}

# WinSparkle keeps no key but the base64 of 32 bytes, and with none it installs updates nobody
# signed, so Pitboard.dll carries one it takes, the one the build was given.
$read = Join-Path $PSScriptRoot 'read-assembly.ps1'
$raw = try { [Convert]::FromBase64String($UpdateKey) } catch { $null }
if (-not $raw -or $raw.Length -ne 32) {
    throw "the update key '$UpdateKey' is not the base64 of 32 bytes, so WinSparkle would take none and install updates nobody signed"
}
$carried = & $read -Assembly (Join-Path $root 'Pitboard.dll') -Metadata 'WindowsUpdatePublicKey'
if ($carried -cne $UpdateKey) {
    $has = if ($carried) { "the update key '$carried'" } else { 'no update key' }
    throw "Pitboard.dll carries $has, and the build was given '$UpdateKey'"
}
# WinSparkle sorts 1.0.0+<commit> below 1.0.0, and would offer a build its own version again and again.
# Named apart from $Version: PowerShell's names ignore case, and $version would replace it.
$compared = [Diagnostics.FileVersionInfo]::GetVersionInfo($exe).ProductVersion
if (-not $compared -or $compared.Contains('+')) {
    throw "Pitboard.exe's version is '$compared': WinSparkle compares it, so it carries no commit"
}
Write-Output "Pitboard $compared carries the update key $carried."
# The 1.0 session's build is the only one that reads another feed, and it never ships.
$feed = & $read -Assembly (Join-Path $root 'Pitboard.Windows.dll') -Constant 'Pitboard.Windows.SparkleUpdates.Feed'
$latest = 'https://github.com/datlechin/pitboard/releases/latest/download/appcast-windows.xml'
if ($feed -cne $latest) { throw "Pitboard.Windows.dll reads its updates from '$feed', not $latest" }
Write-Output "It reads its updates from $latest."

# RUSTFLAGS gives way only to CARGO_ENCODED_RUSTFLAGS. A dev-dependency's features are in
# cargo tree's features edges unless the edge kinds are named.
foreach ($name in 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS') {
    $flags = [Environment]::GetEnvironmentVariable($name)
    if ($flags -and $flags.Contains('pitboard_unreleased_windows')) {
        throw "$name opens the core to Windows before its release: $flags"
    }
}
$cargoToml = Join-Path $PSScriptRoot '..\..\Cargo.toml'
foreach ($crate in 'pitboard-ffi', 'pitboard-share-ffi') {
    $tree = & cargo tree --locked --manifest-path $cargoToml -e 'normal,build,features' --target $Triple -p $crate | Out-String
    if ($LASTEXITCODE -ne 0) { throw "cargo tree -p $crate exited $LASTEXITCODE" }
    if ($tree -match 'feature "(test-support|fixture)"') {
        throw "$crate resolves the $($Matches[1]) feature, which only a build for tests has"
    }
}
$library = Join-Path $root 'pitboard_ffi.dll'
if (-not (Test-Path -LiteralPath $library -PathType Leaf)) { throw 'no pitboard_ffi.dll beside Pitboard.exe' }
if ((Get-FileHash -LiteralPath $library).Hash -ne (Get-FileHash -LiteralPath $gated).Hash) {
    throw "the published pitboard_ffi.dll is not $gated"
}
Write-Output 'pitboard_ffi.dll is the one built here, and neither library has the cfg or a feature for tests.'

# What release.yml hands the signer, which signs only programs whose version resource names one product at one
# version. bin\pitboard.exe is the command line's, signed with its zip.
$ours = @(Get-ChildItem -LiteralPath $root -Recurse -File |
    Where-Object { $_.Name -like 'pitboard*' -and $_.Extension -in '.exe', '.dll' -and $_.DirectoryName -ne (Join-Path $root 'bin') })
foreach ($program in $ours) {
    $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($program.FullName)
    $file = '{0}.{1}.{2}.{3}' -f $info.FileMajorPart, $info.FileMinorPart, $info.FileBuildPart, $info.FilePrivatePart
    if ($info.ProductName -cne 'Pitboard' -or $info.CompanyName -cne 'Pitboard' -or $info.ProductVersion -cne $Version -or $file -ne $FileVersion) {
        throw ("$($program.Name) names the product '$($info.ProductName)' by '$($info.CompanyName)' at '$($info.ProductVersion)', " +
            "file $file, not Pitboard by Pitboard at $Version, file $FileVersion")
    }
}
Write-Output "$($ours.Name -join ', ') name Pitboard by Pitboard at $Version, file $FileVersion."

$machine = @{ 'x86_64-pc-windows-msvc' = 'AMD64'; 'aarch64-pc-windows-msvc' = 'ARM64' }[$Triple]
$readobj = Get-Command llvm-readobj -ErrorAction SilentlyContinue
$readobj = if ($readobj) { $readobj.Source } else { "$env:ProgramFiles\LLVM\bin\llvm-readobj.exe" }
$headers = & $readobj --file-headers $library | Out-String
if ($LASTEXITCODE -ne 0) { throw 'llvm-readobj could not read pitboard_ffi.dll' }
if ($headers -notmatch "Machine: IMAGE_FILE_MACHINE_$machine\b") {
    Write-Output $headers
    throw "pitboard_ffi.dll is not a library for $machine"
}
$imports = & $readobj --coff-imports $library | Out-String
if ($LASTEXITCODE -ne 0) { throw "llvm-readobj could not read pitboard_ffi.dll's imports" }
if ($imports -match '(?i)vcruntime|api-ms-win-crt') {
    Write-Output $imports
    throw 'pitboard_ffi.dll takes its C runtime from a DLL: it was built without +crt-static'
}
Write-Output "pitboard_ffi.dll is for $machine, with the C runtime inside it."

$sparkle = Join-Path $root 'WinSparkle.dll'
if (-not (Test-Path -LiteralPath $sparkle -PathType Leaf)) { throw 'no WinSparkle.dll beside Pitboard.exe: the app could not update itself' }
$headers = & $readobj --file-headers $sparkle | Out-String
if ($LASTEXITCODE -ne 0) { throw 'llvm-readobj could not read WinSparkle.dll' }
if ($headers -notmatch "Machine: IMAGE_FILE_MACHINE_$machine\b") {
    Write-Output $headers
    throw "WinSparkle.dll is not a library for $machine"
}
Write-Output "WinSparkle.dll is for $machine."
exit 0
