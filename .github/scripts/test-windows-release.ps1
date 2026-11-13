# Tests package-windows.ps1, check-windows-build.ps1 and verify-windows-signatures.ps1 against a
# pitboard.exe built without the cfg and without test-support, as CI's windows job builds one in
# target/gated, and pack-session-bundle.ps1 with a stand-in for age.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')] [string] $Triple,
    # For the leak check of a 1.x build's runs as a standard user.
    [string] $Probe
)
$ErrorActionPreference = 'Stop'

# A GitHub-hosted runner is thrown away after the job.
if (-not ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted')) {
    throw 'test-windows-release.ps1 runs only on a GitHub-hosted runner: it runs the pitboard.exe it is given'
}

$package = Join-Path $PSScriptRoot 'package-windows.ps1'
$check = Join-Path $PSScriptRoot 'check-windows-build.ps1'
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$work = Join-Path $env:RUNNER_TEMP "test-windows-release-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $work | Out-Null

function Assert-Refused([string] $Case, [hashtable] $Given, [string] $Expected, [string] $ScriptPath = $check) {
    try {
        & $ScriptPath @Given | Out-Null
    } catch {
        if ($_.Exception.Message -notmatch $Expected) {
            throw "$Case`: refused for another reason: $($_.Exception.Message)"
        }
        Write-Output "ok: $Case"
        return
    }
    throw "$Case`: $(Split-Path -Leaf $ScriptPath) let it through"
}

$runner = $env:RUNNER_ENVIRONMENT
try {
    $env:RUNNER_ENVIRONMENT = 'self-hosted'
    $off = @{ Exe = $exePath; Version = '1.2.3'; Triple = $Triple; Out = (Join-Path $work 'off') }
    Assert-Refused 'packing off a GitHub-hosted runner' $off 'runs only on a GitHub-hosted runner' $package
    Assert-Refused 'checking off a GitHub-hosted runner' @{ Exe = $exePath; Triple = $Triple } 'runs only on a GitHub-hosted runner'
} finally {
    $env:RUNNER_ENVIRONMENT = $runner
}

$top = "pitboard-v1.2.3-beta.4-$Triple"
$zip = & $package -Exe $exePath -Version '1.2.3-beta.4' -Triple $Triple -Out $work
if ($zip -ne (Join-Path $work "$top.zip")) { throw "package-windows.ps1 wrote $zip" }
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.ZipFile
$archive = [IO.Compression.ZipFile]::OpenRead($zip)
try {
    $names = @($archive.Entries | ForEach-Object { $_.FullName })
    $expected = @('pitboard.exe', 'pitboardw.exe', 'completions/pitboard.ps1', 'completions/pitboard.bash', 'LICENSE', 'NOTICE', 'README.md') |
        ForEach-Object { "$top/$_" }
    if (($names -join ', ') -ne ($expected -join ', ')) { throw "the zip holds $($names -join ', ')" }
    foreach ($entry in $archive.Entries) {
        if ($entry.Length -eq 0) { throw "$($entry.FullName) is empty" }
    }
    $completions = @{ 'completions/pitboard.ps1' = 'Register-ArgumentCompleter'; 'completions/pitboard.bash' = 'complete -F' }
    foreach ($name in $completions.Keys) {
        $reader = [IO.StreamReader]::new($archive.GetEntry("$top/$name").Open())
        try { $text = $reader.ReadToEnd() } finally { $reader.Dispose() }
        if (-not $text.Contains($completions[$name])) { throw "$name is not the shell's completions" }
    }
    $built = @{ 'pitboard.exe' = $exePath; 'pitboardw.exe' = (Join-Path (Split-Path -Parent $exePath) 'pitboardw.exe') }
    foreach ($name in $built.Keys) {
        $packed = Join-Path $work "packed-$name"
        [IO.Compression.ZipFileExtensions]::ExtractToFile($archive.GetEntry("$top/$name"), $packed)
        if ((Get-FileHash -LiteralPath $packed).Hash -ne (Get-FileHash -LiteralPath $built[$name]).Hash) {
            throw "the zip holds another $name"
        }
    }
} finally {
    $archive.Dispose()
}
Write-Output "ok: the zip holds $($names.Count) files in $top, named with /"

function New-Zip([string] $Zip, [Collections.Specialized.OrderedDictionary] $Entries) {
    $archive = [IO.Compression.ZipFile]::Open($Zip, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in $Entries.Keys) {
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $Entries[$name], $name) | Out-Null
        }
    } finally {
        $archive.Dispose()
    }
}

# bottom 0.14.9's release is immutable, and the btm.exe in its x64 and its ARM64 zip is signed by
# SignPath Foundation. Each runner reads the other machine's program too, as windows-sign reads
# both on windows-2025. Nothing in them runs here, and no certificate is made or trusted.
$verify = Join-Path $PSScriptRoot 'verify-windows-signatures.ps1'
$signPath = 'CN=SignPath Foundation, O=SignPath Foundation, L=Lewes, S=Delaware, C=US'
$pinned = [ordered]@{
    'bottom_x86_64-pc-windows-msvc.zip' = '01660721B0FDC72B191F2B27B78DC9A3087AEB7B3FAD4A890C53870089D6B500'
    'bottom_aarch64-pc-windows-msvc.zip' = 'FABE9FC4850D12917150295900119EAA5455724658D1D709E013CACDC0390349'
}
foreach ($name in $pinned.Keys) {
    $file = Join-Path $work $name
    $url = "https://github.com/ClementTsang/bottom/releases/download/0.14.9/$name"
    Invoke-WebRequest -Uri $url -OutFile $file -MaximumRetryCount 5 -RetryIntervalSec 10
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $pinned[$name]) { throw "$url is not the zip read on 15 October 2026" }
    & $verify -Path $file -Mode signed -Subject $signPath
}
Write-Output 'ok: programs for x64 and ARM64 that SignPath Foundation signed pass as signed by it'
$bottom = Join-Path $work 'bottom_x86_64-pc-windows-msvc.zip'

$against = @{ Path = $bottom; Mode = 'signed'; Subject = 'CN=Pitboard' }
Assert-Refused 'a signed program checked against another subject' $against "not 'CN=Pitboard'" $verify
$unsigned = @{ Path = $bottom; Mode = 'unsigned' }
$expected = 'btm\.exe is Valid, and every program of an unsigned release is NotSigned'
Assert-Refused 'a signed program in an unsigned release' $unsigned $expected $verify
$nobody = @{ Path = $bottom; Mode = 'signed' }
Assert-Refused 'a signed release with no subject' $nobody 'names the subject its signer signs as' $verify

$archive = [IO.Compression.ZipFile]::OpenRead($bottom)
try {
    $btm = Join-Path $work 'btm.exe'
    [IO.Compression.ZipFileExtensions]::ExtractToFile($archive.GetEntry('btm.exe'), $btm)
} finally {
    $archive.Dispose()
}
$mixed = Join-Path $work 'mixed.zip'
New-Zip $mixed ([ordered]@{ 'mixed/btm.exe' = $btm; 'mixed/pitboard.exe' = $exePath })
$expected = 'mixed/pitboard\.exe is NotSigned, and every program of a signed release is Valid'
Assert-Refused 'an unsigned program among signed ones' @{ Path = $mixed; Mode = 'signed'; Subject = $signPath } $expected $verify

$readme = Join-Path $work 'readme.zip'
New-Zip $readme ([ordered]@{ 'readme/README.md' = (Join-Path $PSScriptRoot '..\..\README.md') })
Assert-Refused 'a zip that holds no program' @{ Path = $readme; Mode = 'unsigned' } 'readme\.zip holds no program' $verify

& $verify -Path $zip -Mode unsigned -Built $work
Write-Output 'ok: the zip of an unsigned build passes as unsigned, beside the zip that was built'

$grown = Join-Path $work 'grown'
New-Item -ItemType Directory -Path $grown | Out-Null
Copy-Item -LiteralPath $zip -Destination $grown
$grownZip = Join-Path $grown "$top.zip"
$archive = [IO.Compression.ZipFile]::Open($grownZip, [IO.Compression.ZipArchiveMode]::Update)
try {
    $notice = Join-Path $PSScriptRoot '..\..\NOTICE'
    [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $notice, "$top/NOTICE.txt") | Out-Null
} finally {
    $archive.Dispose()
}
$built = @{ Path = $grownZip; Mode = 'unsigned'; Built = $work }
Assert-Refused 'a zip that holds other entries than it was built with' $built 'holds other entries than it was built with' $verify

$tainted = Join-Path $work 'tainted\pitboard.exe'
New-Item -ItemType Directory -Path (Split-Path -Parent $tainted) | Out-Null
Copy-Item -LiteralPath $exePath -Destination $tainted
[IO.File]::AppendAllText($tainted, 'PITBOARD_TEST_CLAUDE_DESKTOP')
Assert-Refused 'a build that holds a variable only a build for tests reads' @{ Exe = $tainted; Triple = $Triple } 'only a build for tests reads'

Assert-Refused 'a build that names another version' @{ Exe = $exePath; Triple = $Triple; Version = '0.0.1' } "not 'pitboard 0\.0\.1'"

# The version resource keeps its strings in UTF-16, where Rust keeps none of its own.
$latin1 = [Text.Encoding]::Latin1
$text = $latin1.GetString([IO.File]::ReadAllBytes($exePath))
$named = $latin1.GetString([Text.Encoding]::Unicode.GetBytes('Pitboard'))
if (-not $text.Contains($named)) { throw 'pitboard.exe holds no version resource that names Pitboard' }
$renamed = Join-Path $work 'renamed\pitboard.exe'
New-Item -ItemType Directory -Path (Split-Path -Parent $renamed) | Out-Null
[IO.File]::WriteAllBytes($renamed, $latin1.GetBytes($text.Replace($named, $latin1.GetString([Text.Encoding]::Unicode.GetBytes('Pitbaord')))))
Assert-Refused 'a build whose version resource names another product' @{ Exe = $renamed; Triple = $Triple } "has ProductName 'Pitbaord', not 'Pitboard'"

$other = if ($Triple -eq 'x86_64-pc-windows-msvc') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
Assert-Refused 'a build for the other machine' @{ Exe = $exePath; Triple = $other } 'is not a program for'
$launcher = Join-Path (Split-Path -Parent $exePath) 'pitboardw.exe'
Assert-Refused 'a launcher that is a console program' @{ Exe = $exePath; Triple = $Triple; Launcher = $exePath } 'pitboardw.exe is not a GUI program'

$flags = @{ RUSTFLAGS = $env:RUSTFLAGS; CARGO_ENCODED_RUSTFLAGS = $env:CARGO_ENCODED_RUSTFLAGS }
try {
    $env:CARGO_ENCODED_RUSTFLAGS = $null
    $env:RUSTFLAGS = '-C target-feature=+crt-static --cfg pitboard_unreleased_windows'
    Assert-Refused 'a release whose RUSTFLAGS open it' @{ Exe = $exePath; Triple = $Triple; Release = $true } '^RUSTFLAGS opens'
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    $env:CARGO_ENCODED_RUSTFLAGS = "-C`u{1f}target-feature=+crt-static`u{1f}--cfg`u{1f}pitboard_unreleased_windows"
    Assert-Refused 'a release whose CARGO_ENCODED_RUSTFLAGS open it' @{ Exe = $exePath; Triple = $Triple; Release = $true } 'CARGO_ENCODED_RUSTFLAGS opens'
    $env:CARGO_ENCODED_RUSTFLAGS = $null
    $release = @{ Exe = $exePath; Triple = $Triple; Release = $true; Launcher = $launcher }
    if ($Probe) { $release.Probe = $Probe }
    & $check @release | Out-Null
    Write-Output 'ok: a build with neither passes as a release'
} finally {
    $env:RUSTFLAGS = $flags.RUSTFLAGS
    $env:CARGO_ENCODED_RUSTFLAGS = $flags.CARGO_ENCODED_RUSTFLAGS
}

# check-windows-build.ps1's pattern, against the edges that do reach the dev-dependency's feature.
$manifest = Join-Path $PSScriptRoot '..\..\Cargo.toml'
$tree = & cargo tree --locked --manifest-path $manifest -e features --target $Triple -p pitboard | Out-String
if ($LASTEXITCODE -ne 0) { throw "cargo tree exited $LASTEXITCODE" }
if ($tree -notmatch 'feature "(test-support|fixture)"') { throw "the -Release check's pattern finds no feature for tests in: $tree" }
Write-Output 'ok: the -Release check''s pattern finds test-support through the dev-dependency'

# A stand-in for age that keeps what it was asked and writes age's header for an X25519 key before
# the zip, or with STAND_IN_AGE_PLAIN the zip alone.
$pack = Join-Path $PSScriptRoot 'pack-session-bundle.ps1'
$standIn = Join-Path $work 'age.ps1'
Set-Content -LiteralPath $standIn -Value @'
Set-Content -LiteralPath $env:STAND_IN_AGE_ASKED -Value ($args -join ' ')
$header = if ($env:STAND_IN_AGE_PLAIN) { '' } else { "age-encryption.org/v1`n-> X25519 stand-in`n" }
[IO.File]::WriteAllBytes($args[-2], [byte[]]([Text.Encoding]::ASCII.GetBytes($header) + [IO.File]::ReadAllBytes($args[-1])))
exit 0
'@
$key = 'age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p'
$env:STAND_IN_AGE_ASKED = Join-Path $work 'age-asked.txt'
function New-Bundle([string] $Under, [string[]] $Files) {
    $folder = Join-Path $work $Under
    foreach ($file in $Files) {
        $path = Join-Path $folder $file
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
        [IO.File]::WriteAllText($path, $file)
    }
    $folder
}
$files = 'zip/pitboard.zip', 'msi/Pitboard-b.msi', 'msi/Pitboard-a.msi', 'probe/Pitboard.Probe/Pitboard.Probe.exe'
$sorted = 'msi/Pitboard-a.msi', 'msi/Pitboard-b.msi', 'probe/Pitboard.Probe/Pitboard.Probe.exe', 'zip/pitboard.zip'

$bundle = New-Bundle 'session' $files
$sealedDir = Join-Path $work 'sealed'
$sealed = & $pack -Folder $bundle -Name 'session-build-test' -Out $sealedDir -Age $standIn -Recipient $key
$zipped = Join-Path $sealedDir 'session-build-test.zip'
if ($sealed -cne "$zipped.age") { throw "pack-session-bundle.ps1 printed $sealed" }
$left = @(Get-ChildItem -LiteralPath $sealedDir -Force | ForEach-Object Name)
if (($left -join ', ') -cne 'session-build-test.zip.age') { throw "pack-session-bundle.ps1 left $($left -join ', ')" }
$asked = (Get-Content -LiteralPath $env:STAND_IN_AGE_ASKED -Raw).Trim()
if ($asked -cne "--encrypt --recipient $key --output $zipped.age $zipped") { throw "age was asked: $asked" }
$sums = @(Get-Content -LiteralPath (Join-Path $bundle 'SHA256SUMS'))
if ($sums.Count -ne $sorted.Count) { throw "SHA256SUMS has $($sums.Count) lines: $($sums -join '; ')" }
for ($i = 0; $i -lt $sorted.Count; $i++) {
    $hash = (Get-FileHash -LiteralPath (Join-Path $bundle $sorted[$i]) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($sums[$i] -cne "$hash  $($sorted[$i])") { throw "SHA256SUMS line $($i + 1) is '$($sums[$i])', not '$hash  $($sorted[$i])'" }
}
$header = "age-encryption.org/v1`n-> X25519 stand-in`n"
$bytes = [IO.File]::ReadAllBytes($sealed)
$inner = Join-Path $work 'session-build-test.zip'
[IO.File]::WriteAllBytes($inner, [byte[]]($bytes[$header.Length..($bytes.Length - 1)]))
$archive = [IO.Compression.ZipFile]::OpenRead($inner)
try { $names = @($archive.Entries | ForEach-Object { $_.FullName }) } finally { $archive.Dispose() }
$expected = @(@($sorted) + 'SHA256SUMS' | ForEach-Object { "session-build-test/$_" })
if (($names -join ', ') -cne ($expected -join ', ')) { throw "the session bundle's zip holds $($names -join ', ')" }
Write-Output 'ok: the session bundle is summed, zipped with / and handed to age, and only age''s file is left'

$summed = @{ Folder = $bundle; Name = 'session-build-test'; Out = (Join-Path $work 'summed'); Age = $standIn; Recipient = $key }
Assert-Refused 'a session bundle summed already' $summed 'holds a SHA256SUMS already' $pack
$spaced = @{ Folder = (New-Bundle 'spaced' @('msi/Pitboard a.msi')); Name = 'session-build-test'; Out = (Join-Path $work 'spaced-out'); Age = $standIn; Recipient = $key }
Assert-Refused 'a session bundle with a space in a name' $spaced 'has a space in its name' $pack
$plainDir = Join-Path $work 'plain-out'
$plain = @{ Folder = (New-Bundle 'plain' $files); Name = 'session-build-test'; Out = $plainDir; Age = $standIn; Recipient = $key }
try {
    $env:STAND_IN_AGE_PLAIN = '1'
    Assert-Refused 'an age that writes the zip unencrypted' $plain 'does not read as encrypted to an X25519 key' $pack
} finally {
    $env:STAND_IN_AGE_PLAIN = $null
}
$left = @(Get-ChildItem -LiteralPath $plainDir -Force | ForEach-Object Name)
if ($left) { throw "pack-session-bundle.ps1 refused, and left $($left -join ', ')" }
Write-Output 'ok: nothing is left of a session bundle age did not encrypt'
$env:STAND_IN_AGE_ASKED = $null

Remove-Item -Recurse -Force -LiteralPath $work
Write-Output 'Every case passed.'
exit 0
