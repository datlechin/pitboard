# The job's user is elevated in every program it starts, so each test program runs as a fresh
# standard user. Doctests need the toolchain, which that user cannot reach, and are not run.
param(
    [Parameter(Mandatory)] [string] $Cargo,
    # Built already, inside the workspace the user is granted read and run on.
    [Parameter(Mandatory)] [string] $Probe,
    # What goes after `--` on cargo test's command line: test name filters, or '--skip <name>'.
    [string] $Pass = ''
)
$ErrorActionPreference = 'Stop'

# A GitHub-hosted runner is thrown away after the job.
if (-not ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted')) {
    throw 'test-as-standard-user.ps1 runs only on a GitHub-hosted runner: it makes a Windows account, grants it rights on folders and may change who holds the symbolic-link right'
}

function Split-Words([string] $Text) {
    @($Text -split '\s+' | Where-Object { $_ })
}

if (-not (Test-Path -LiteralPath $Probe -PathType Leaf)) {
    throw "no measurement probe at $Probe for the leak check: build it first, with cargo build --locked -p pitboard-probe"
}
$probePath = (Resolve-Path -LiteralPath $Probe).Path

# Any of these present would give a test the runner's settings rather than its own.
function Get-MachineLayers {
    @(
        (Join-Path $env:ProgramData 'OpenAI\Codex'),
        'C:\Program Files\ClaudeCode',
        'HKLM:\SOFTWARE\Policies\ClaudeCode',
        'HKCU:\SOFTWARE\Policies\ClaudeCode'
    ) | Where-Object { Test-Path -LiteralPath $_ }
}

function Get-LoginPlaces([string] $ProfileDir, [string] $LocalData) {
    @(
        (Join-Path $ProfileDir '.claude'),
        (Join-Path $ProfileDir '.claude.json'),
        (Join-Path $ProfileDir '.codex'),
        (Join-Path $LocalData 'Pitboard')
    )
}

$layers = @(Get-MachineLayers)
if ($layers.Count -gt 0) {
    throw "this runner is not hermetic for the tests, since it holds $($layers -join ', '), from which Codex or Claude Code may take settings on every account"
}
Write-Output 'Codex''s %ProgramData%\OpenAI\Codex, Claude Code''s C:\Program Files\ClaudeCode and its policy keys are absent from this runner.'
# What the job user has already is the image's; only what appears during the run is a leak.
$jobPlaces = @(Get-LoginPlaces $env:USERPROFILE $env:LOCALAPPDATA)
$jobPlacesBefore = @($jobPlaces | Where-Object { Test-Path -LiteralPath $_ })
Write-Output "Of the job user's own login places, $($jobPlacesBefore.Count) of $($jobPlaces.Count) are there before the tests run."

# Start-Process joins its arguments with spaces and quotes nothing.
$passed = @('--color=never'; Split-Words $Pass)
foreach ($word in $passed) {
    if ($word -match '["\s]') { throw "a test program's argument may hold no space or quote: $word" }
}

# Built as the job's user: Cargo and rustup are in its profile, which a standard user cannot open.
$built = & cargo test --no-run --message-format=json-render-diagnostics @(Split-Words $Cargo)
if ($LASTEXITCODE -ne 0) { throw "cargo test --no-run $Cargo failed" }
$tests = @(
    $built | Where-Object { $_.StartsWith('{') } | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.executable -and $_.profile.test } |
        Sort-Object -Property executable -Unique
)
if ($tests.Count -eq 0) { throw "cargo test --no-run $Cargo built no test program" }
$metadata = & cargo metadata --format-version 1 --no-deps --locked
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$metadata = ($metadata | Out-String) | ConvertFrom-Json
$workspace = $metadata.workspace_root
$readable = @($workspace)
if (-not $metadata.target_directory.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    $readable += $metadata.target_directory
}

$name = 'pbtest' + (Get-Random -Minimum 10000 -Maximum 99999)
$secure = ConvertTo-SecureString -String ([guid]::NewGuid().ToString('N') + 'Aa1!') -AsPlainText -Force
$cred = [pscredential]::new($name, $secure)
$logs = Join-Path $env:RUNNER_TEMP "test-as-$name"
New-Item -ItemType Directory -Force -Path $logs | Out-Null
# At the drive root, as runner-facts.ps1 ran it: a folder it cannot open fails with ERROR_DIRECTORY.
$runDir = Join-Path "$env:SystemDrive\" "pitboard-test-as-$name"

function Start-AsUser([string] $Program, [string[]] $Arguments, [string] $In, [hashtable] $Environment, [string] $Label) {
    $out = Join-Path $logs "$Label.out.txt"
    $err = Join-Path $logs "$Label.err.txt"
    $start = @{
        FilePath = $Program
        Credential = $cred
        LoadUserProfile = $true
        WorkingDirectory = $In
        Wait = $true
        PassThru = $true
        RedirectStandardOutput = $out
        RedirectStandardError = $err
    }
    if ($Arguments) { $start.ArgumentList = $Arguments }
    if ($Environment) { $start.Environment = $Environment }
    $process = Start-Process @start
    [pscustomobject]@{ Exit = $process.ExitCode; Out = $out; Err = $err }
}

# Workflow commands off, so no line a test prints is read as one.
function Show-Run($Run) {
    $token = [guid]::NewGuid().ToString('N')
    Write-Output "::stop-commands::$token"
    foreach ($file in $Run.Out, $Run.Err) {
        $text = Get-Content -Raw -Path $file -ErrorAction SilentlyContinue
        if ($text) { Write-Output $text.TrimEnd() }
    }
    Write-Output "::$token::"
}

# SIDs are compared and written back, never printed.
function Get-SymlinkHolders {
    $export = Join-Path $logs 'rights.inf'
    secedit /export /cfg $export /areas USER_RIGHTS *> $null
    $line = Get-Content -Path $export | Where-Object { $_ -match '^SeCreateSymbolicLinkPrivilege\s*=' } | Select-Object -First 1
    Remove-Item -Force -Path $export -ErrorAction SilentlyContinue
    if (-not $line) { return '' }
    ((($line -split '=', 2)[1] -split ',') | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Sort-Object) -join ','
}

function Set-SymlinkHolders([string] $Holders) {
    $inf = Join-Path $logs 'rights-set.inf'
    $db = Join-Path $logs 'rights-set.sdb'
    @(
        '[Unicode]', 'Unicode=yes',
        '[Version]', 'signature="$CHICAGO$"', 'Revision=1',
        '[Privilege Rights]', "SeCreateSymbolicLinkPrivilege = $Holders"
    ) | Set-Content -Path $inf -Encoding Unicode
    secedit /configure /db $db /cfg $inf /areas USER_RIGHTS *> $null
    $exit = $LASTEXITCODE
    Remove-Item -Force -Path $inf, $db -ErrorAction SilentlyContinue
    if ($exit -ne 0) { throw "secedit could not set the symbolic-link right (exit $exit)" }
}

# icacls names the files it could not change, never a secret.
function Set-UserRights([string] $Folder, [string] $Rights) {
    $said = if ($Rights) {
        icacls $Folder /grant "*${sid}:(OI)(CI)$Rights" /Q 2>&1 | Out-String
    } else {
        icacls $Folder /remove:g "*$sid" /Q 2>&1 | Out-String
    }
    if ($LASTEXITCODE -ne 0) { throw "icacls could not set the standard user's rights on $Folder (exit $LASTEXITCODE): $said" }
}

$user = New-LocalUser -Name $name -Password $secure -AccountNeverExpires -PasswordNeverExpires
$sid = $user.SID.Value
$holders = $null
$granted = [System.Collections.Generic.List[string]]::new()
$failed = [System.Collections.Generic.List[string]]::new()
try {
    Add-LocalGroupMember -Group 'Users' -Member $name -ErrorAction SilentlyContinue

    New-Item -ItemType Directory -Force -Path $runDir | Out-Null
    Set-UserRights $runDir 'M'
    foreach ($folder in $readable) {
        $granted.Add($folder)
        Set-UserRights $folder 'RX'
    }

    $devKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
    $developerMode = (Get-ItemProperty -Path $devKey -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense -eq 1
    if ($developerMode) {
        Write-Output 'Developer Mode is on, so the standard user can make symbolic links as it is.'
    } else {
        $holders = Get-SymlinkHolders
        Set-SymlinkHolders $(if ($holders) { "$holders,*$sid" } else { "*$sid" })
        Write-Output 'Developer Mode is off, so the standard user is given the symbolic-link right for this run.'
    }

    # The first logon makes the profile.
    $made = Start-AsUser "$env:SystemRoot\System32\cmd.exe" @('/d', '/c', 'exit', '0') $runDir $null 'profile'
    if ($made.Exit -ne 0) { Show-Run $made; throw "the standard user's first logon exited $($made.Exit)" }
    $profileDir = (Get-CimInstance -ClassName Win32_UserProfile | Where-Object { $_.SID -eq $sid }).LocalPath
    if (-not $profileDir) { throw 'the standard user has no profile after its first logon' }
    $userEnv = @{
        USERPROFILE = $profileDir
        HOMEDRIVE = Split-Path -Path $profileDir -Qualifier
        HOMEPATH = Split-Path -Path $profileDir -NoQualifier
        APPDATA = Join-Path $profileDir 'AppData\Roaming'
        LOCALAPPDATA = Join-Path $profileDir 'AppData\Local'
        TEMP = Join-Path $profileDir 'AppData\Local\Temp'
        TMP = Join-Path $profileDir 'AppData\Local\Temp'
        USERNAME = $name
        # Without it insta 1.48 runs `cargo metadata`, which is in the job user's profile.
        INSTA_WORKSPACE_ROOT = $workspace
        INSTA_UPDATE = 'no'
    }
    # Made by the user itself, so the folder is its own.
    if (-not (Test-Path -LiteralPath $userEnv.TEMP)) {
        $temp = Start-AsUser "$env:SystemRoot\System32\cmd.exe" @('/d', '/c', 'mkdir', $userEnv.TEMP) $runDir $userEnv 'temp'
        if ($temp.Exit -ne 0) { Show-Run $temp; throw "the standard user could not make its temporary folder" }
    }

    $i = 0
    foreach ($test in $tests) {
        $i++
        $environment = $userEnv.Clone()
        $environment.CARGO_MANIFEST_DIR = Split-Path -Parent $test.manifest_path
        $environment.CARGO_MANIFEST_PATH = $test.manifest_path
        $what = "$($test.target.name) ($($test.target.kind -join ', '))"
        $run = Start-AsUser $test.executable $passed $runDir $environment ('{0:D2}-{1}' -f $i, $test.target.name)
        $summary = @(Select-String -Path $run.Out -Pattern '^test result: (\w+)\.' | ForEach-Object { $_.Matches[0].Groups[1].Value })
        # The summary too, so a lost exit code cannot pass a failing program.
        $ok = $run.Exit -eq 0 -and $summary.Count -gt 0 -and $summary[-1] -eq 'ok'
        Write-Output "::group::$what as a standard user: $(if ($ok) { 'passed' } else { 'FAILED' })"
        Show-Run $run
        Write-Output '::endgroup::'
        if (-not $ok) { $failed.Add("$what exited $($run.Exit), its last result $(if ($summary) { $summary[-1] } else { 'missing' })") }
    }

    $leaked = [System.Collections.Generic.List[string]]::new()
    $leaks = Start-AsUser $probePath @('credman-names', '--leak-check') $runDir $userEnv 'leak-check'
    Write-Output "::group::Credential Manager's live login families and test items, as the standard user: $(if ($leaks.Exit -eq 0) { 'none' } else { 'FOUND OR UNREADABLE' })"
    Show-Run $leaks
    Write-Output '::endgroup::'
    if ($leaks.Exit -ne 0) {
        $leaked.Add("the leak check found a live login family's or a test's item in the standard user's Credential Manager, or could not list a family (exit $($leaks.Exit))")
    }
    # Every task, as the job's user, so a list that cannot be read fails rather than reads empty.
    $tasks = @(Get-ScheduledTask | Where-Object { $_.TaskPath -like '\Pitboard\*' })
    if ($tasks.Count -gt 0) { $leaked.Add("Task Scheduler's \Pitboard\ folder holds $($tasks.Count) task(s)") }
    foreach ($place in (Get-LoginPlaces $profileDir $userEnv.LOCALAPPDATA)) {
        if (Test-Path -LiteralPath $place) { $leaked.Add("the standard user's real $(Split-Path -Leaf $place) exists") }
    }
    $layers = @(Get-MachineLayers)
    if ($layers.Count -gt 0) { $leaked.Add("the machine's own layers appeared: $($layers -join ', ')") }
    foreach ($place in $jobPlaces) {
        if ((Test-Path -LiteralPath $place) -and ($jobPlacesBefore -notcontains $place)) {
            $leaked.Add("the job user's real $(Split-Path -Leaf $place) appeared")
        }
    }
    if ($leaked.Count -eq 0) {
        Write-Output 'No login, test item, task or login folder was left behind.'
    }
    $failed.AddRange($leaked)
} finally {
    if ($null -ne $holders) {
        Set-SymlinkHolders $holders
        if ((Get-SymlinkHolders) -ne $holders) { $failed.Add('the symbolic-link right was not set back as it was') }
    }
    foreach ($folder in $granted) {
        try { Set-UserRights $folder '' } catch { $failed.Add($_.Exception.Message) }
    }
    Remove-LocalUser -Name $name -ErrorAction SilentlyContinue
    Get-CimInstance -ClassName Win32_UserProfile | Where-Object { $_.SID -eq $sid } |
        Remove-CimInstance -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force -Path $runDir -ErrorAction SilentlyContinue
}
if ($failed.Count -gt 0) {
    throw "as a standard user: $($failed -join '; ')"
}
Write-Output "$($tests.Count) test programs passed as a standard user."
exit 0
