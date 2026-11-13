# Packs pitboard.exe, and the pitboardw.exe a task starts it through, built beside it, as a release
# ships them for Windows: pitboard-v<version>-<triple>.zip in -Out, holding one folder of the same
# name, as the tarballs do. Prints the zip's path.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')] [string] $Triple,
    [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = 'Stop'

# A GitHub-hosted runner is thrown away after the job.
if (-not ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted')) {
    throw 'package-windows.ps1 runs only on a GitHub-hosted runner: it runs the pitboard.exe it is given'
}
if ($Version -notmatch '^[0-9A-Za-z.+-]+$') { throw "`"$Version`" is not a version" }

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$name = "pitboard-v$Version-$Triple"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
# .NET reads a relative path against its own current folder, which is not PowerShell's.
$dist = (Resolve-Path -LiteralPath $Out).Path
$folder = Join-Path $dist $name
$zip = Join-Path $dist "$name.zip"
foreach ($old in $folder, $zip) {
    if (Test-Path -LiteralPath $old) { Remove-Item -Recurse -Force -LiteralPath $old }
}
New-Item -ItemType Directory -Path (Join-Path $folder 'completions') | Out-Null
$packed = Join-Path $folder 'pitboard.exe'
Copy-Item -LiteralPath $Exe -Destination $packed
$launcher = Join-Path (Split-Path -Parent $Exe) 'pitboardw.exe'
if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) { throw "no pitboardw.exe beside $Exe" }
Copy-Item -LiteralPath $launcher -Destination (Join-Path $folder 'pitboardw.exe')
foreach ($file in 'LICENSE', 'NOTICE', 'README.md') {
    Copy-Item -LiteralPath (Join-Path $root $file) -Destination $folder
}

# Never the runner's own homes, whatever the program comes to read.
$scratch = Join-Path $env:RUNNER_TEMP "package-windows-$([guid]::NewGuid().ToString('N'))"
$homes = @{}
foreach ($variable in 'USERPROFILE', 'HOME', 'APPDATA', 'LOCALAPPDATA', 'CODEX_HOME', 'CLAUDE_CONFIG_DIR', 'PITBOARD_HOME') {
    $homes[$variable] = Join-Path $scratch $variable
    New-Item -ItemType Directory -Force -Path $homes[$variable] | Out-Null
}

# Byte for byte: PowerShell decodes what a program prints and encodes it again.
function Save-Output([string] $Program, [string[]] $Arguments, [string] $Path) {
    $start = [Diagnostics.ProcessStartInfo]::new($Program)
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    foreach ($variable in $homes.Keys) { $start.Environment[$variable] = $homes[$variable] }
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $process = [Diagnostics.Process]::Start($start)
    $file = [IO.File]::Create($Path)
    try { $process.StandardOutput.BaseStream.CopyTo($file) } finally { $file.Dispose() }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw "pitboard $($Arguments -join ' ') exited $($process.ExitCode)" }
    if ((Get-Item -LiteralPath $Path).Length -eq 0) { throw "pitboard $($Arguments -join ' ') printed nothing" }
}

# Git Bash reads the bash completions.
Save-Output $packed @('completions', 'powershell') (Join-Path $folder 'completions\pitboard.ps1')
Save-Output $packed @('completions', 'bash') (Join-Path $folder 'completions\pitboard.bash')
Remove-Item -Recurse -Force -LiteralPath $scratch

# Each entry named here, with /, as the zip format asks.
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.ZipFile
$entries = 'pitboard.exe', 'pitboardw.exe', 'completions/pitboard.ps1', 'completions/pitboard.bash', 'LICENSE', 'NOTICE', 'README.md'
$archive = [IO.Compression.ZipFile]::Open($zip, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($entry in $entries) {
        $source = Join-Path $folder $entry
        $level = [IO.Compression.CompressionLevel]::Optimal
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $source, "$name/$entry", $level) | Out-Null
    }
} finally {
    $archive.Dispose()
}
Write-Output $zip
