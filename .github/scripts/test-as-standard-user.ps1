# The job's user is elevated in every program it starts, so the tests run as a fresh standard
# user: nextest's, from the test programs and metadata the job's user built, and test programs
# that need nothing from the job user's profile, each run as it is. With -Exe, the user runs
# one program in scratch homes, and each run comes back as an object.
# Bound strictly, so a misspelt option fails rather than lands in $args.
[CmdletBinding()]
param(
    # Cargo's options for the build, as `cargo nextest list` takes them.
    [string] $Cargo,
    # A nextest filterset for the run of what -Cargo built.
    [string] $Filter,
    # Self-contained test programs, such as a published Microsoft.Testing.Platform one, inside
    # the workspace.
    [string[]] $Dotnet,
    # Where each of them leaves its results, in a folder named after it: a TRX report and the
    # files its tests kept with TestContext.AddResultFile, which only a TRX report copies there.
    [string] $Results,
    [string] $Exe,
    # Its argument lines, run in turn; an empty one runs it with none.
    [string[]] $Commands = @(''),
    # Built already, inside the workspace the user is granted read and run on. Without it, -Exe
    # leaves Credential Manager out of the leak check.
    [string] $Probe,
    # An exFAT VHD for the run, detached when it ends. The tests find a folder on it in
    # PITBOARD_TEST_VOLUME, and every account may write there: exFAT keeps no access list.
    [switch] $ExFat
)
$ErrorActionPreference = 'Stop'

if (-not $Cargo -and -not $Dotnet -and -not $Exe) {
    throw 'nothing to run: give -Cargo, -Dotnet, -Exe or a mix'
}
if (-not $Probe -and ($Cargo -or $Dotnet)) {
    throw 'the tests need -Probe for the leak check'
}
if ($Filter -match '[\s"]') {
    throw "-Filter $Filter holds a space or a quote, which Start-Process would pass to nextest split"
}

# A GitHub-hosted runner is thrown away after the job.
if (-not ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted')) {
    throw 'test-as-standard-user.ps1 runs only on a GitHub-hosted runner: it makes a Windows account, grants it rights on folders, may change who holds the symbolic-link right and may mount a volume'
}

function Split-Words([string] $Text) {
    @($Text -split '\s+' | Where-Object { $_ })
}

$probePath = $null
if ($Probe) {
    if (-not (Test-Path -LiteralPath $Probe -PathType Leaf)) {
        throw "no measurement probe at $Probe for the leak check: build it first, with cargo build --locked -p pitboard-probe"
    }
    $probePath = (Resolve-Path -LiteralPath $Probe).Path
}
if ($Exe) {
    if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { throw "no program at $Exe" }
    $exePath = (Resolve-Path -LiteralPath $Exe).Path
}

# Any of these present would give a test the runner's settings rather than its own.
function Get-MachineLayers {
    @(
        (Join-Path $env:ProgramData 'OpenAI\Codex'),
        'C:\Program Files\ClaudeCode',
        'HKLM:\SOFTWARE\Policies\ClaudeCode',
        'HKCU:\SOFTWARE\Policies\ClaudeCode'
    ) | Where-Object { Test-Path -LiteralPath $_ }
}

# The app's data folders hold its account windows' sign-ins.
function Get-LoginPlaces([string] $ProfileDir, [string] $LocalData) {
    @(
        (Join-Path $ProfileDir '.claude'),
        (Join-Path $ProfileDir '.claude.json'),
        (Join-Path $ProfileDir '.codex'),
        (Join-Path $LocalData 'Pitboard'),
        (Join-Path $LocalData 'Pitboard App'),
        (Join-Path $LocalData 'Pitboard App Debug')
    )
}

# The AppUserModelIDs AppNotificationManager.Register keeps a key for: the app's two builds and
# Pitboard.Probe's.
$notifiers = 'Pitboard.Pitboard', 'Pitboard.Pitboard.Debug', 'Pitboard.Probe'

# What only an installed Release registers, under the job user's HKCU: its link schemes, its
# notifications, the start at sign-in and the command line on the user PATH.
function Get-JobRegistrations {
    foreach ($scheme in 'pitboard', 'pitboard-debug') {
        if (Test-Path -LiteralPath "HKCU:\Software\Classes\$scheme") { "HKCU\Software\Classes\$scheme" }
    }
    foreach ($aumid in $notifiers) {
        if (Test-Path -LiteralPath "HKCU:\Software\Classes\AppUserModelId\$aumid") { "HKCU\Software\Classes\AppUserModelId\$aumid" }
    }
    $run = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Pitboard' -ErrorAction SilentlyContinue
    if ($null -ne $run) { 'the Run value Pitboard' }
    if ([Environment]::GetEnvironmentVariable('Path', 'User') -match 'Programs\\Pitboard') { 'a user PATH naming Programs\Pitboard' }
}

# Every user's, which the job user may list. Through Windows PowerShell, whose Appx module
# PowerShell 7 does not load on every image.
function Get-PitboardPackages {
    $names = & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -Command `
        "Get-AppxPackage -AllUsers -Name 'Pitboard*' | ForEach-Object { `$_.PackageFullName }"
    if ($LASTEXITCODE -ne 0) { throw 'Get-AppxPackage could not list the packages' }
    @($names | Where-Object { $_ })
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
$jobRegistrationsBefore = @(Get-JobRegistrations)
$packagesBefore = @(Get-PitboardPackages)

function Test-Inside([string] $Path, [string] $Folder) {
    $Path.StartsWith($Folder.TrimEnd('\') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
}

$workspace = (Resolve-Path -LiteralPath $env:GITHUB_WORKSPACE).Path
$readable = @($workspace)
if ($Cargo) {
    # Built as the job's user: Cargo and rustup are in its profile, which a standard user cannot
    # open. Given both metadata files, nextest runs what was built without them.
    $binaries = & cargo nextest list --list-type binaries-only --message-format json @(Split-Words $Cargo)
    if ($LASTEXITCODE -ne 0) { throw "cargo nextest list $Cargo failed" }
    $cargoMetadata = & cargo metadata --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $metadata = ($cargoMetadata | Out-String) | ConvertFrom-Json
    $nextest = (Get-Command -Name cargo-nextest -CommandType Application).Source
    # Where the ci profile writes its report.
    $store = Join-Path $metadata.target_directory 'nextest'
    if (-not (Test-Inside $metadata.target_directory $workspace)) {
        $readable += $metadata.target_directory
    }
}
$programs = @(foreach ($program in $Dotnet) {
    if (-not (Test-Path -LiteralPath $program -PathType Leaf)) { throw "no test program at $program to run: build it first" }
    $path = (Resolve-Path -LiteralPath $program).Path
    if (-not (Test-Inside $path $workspace)) { throw "$path is not inside the workspace, $workspace" }
    $path
})

$name = 'pbtest' + (Get-Random -Minimum 10000 -Maximum 99999)
$secure = ConvertTo-SecureString -String ([guid]::NewGuid().ToString('N') + 'Aa1!') -AsPlainText -Force
$cred = [pscredential]::new($name, $secure)
$logs = Join-Path $env:RUNNER_TEMP "test-as-$name"
New-Item -ItemType Directory -Force -Path $logs | Out-Null
# At the drive root, as runner-facts.ps1 ran it: a folder it cannot open fails with ERROR_DIRECTORY.
$runDir = Join-Path "$env:SystemDrive\" "pitboard-test-as-$name"

# -Wait waits for every process the program started too. With -Alone the program alone is waited on, and what
# it left running is named and ended: a WebView2 browser process held by a dialog would hold the job until its
# timeout, with nothing said.
function Start-AsUser([string] $Program, [string[]] $Arguments, [string] $In, [hashtable] $Environment, [string] $Label, [switch] $Alone) {
    $out = Join-Path $logs "$Label.out.txt"
    $err = Join-Path $logs "$Label.err.txt"
    $start = @{
        FilePath = $Program
        Credential = $cred
        LoadUserProfile = $true
        WorkingDirectory = $In
        Wait = -not $Alone
        PassThru = $true
        RedirectStandardOutput = $out
        RedirectStandardError = $err
    }
    if ($Arguments) { $start.ArgumentList = $Arguments }
    if ($Environment) { $start.Environment = $Environment }
    try {
        $process = Start-Process @start
    } catch {
        # Out-Host, as the caller would take the lines for its result.
        Show-LogonFailures $userMade | Out-Host
        throw
    }
    if ($Alone) {
        # Read while it runs, so its exit code stays readable once it has gone.
        $null = $process.Handle
        $process.WaitForExit()
        foreach ($left in @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($process.Id)" -ErrorAction SilentlyContinue)) {
            $title = (Get-Process -Id $left.ProcessId -ErrorAction SilentlyContinue).MainWindowTitle
            "$Label left $($left.Name) running$(if ($title) { ", showing '$title'" }), and it is ended" | Out-Host
            & "$env:SystemRoot\System32\taskkill.exe" /T /F /PID $left.ProcessId 2>&1 | Out-Null
        }
    }
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

# A program that fails fast, as WinUI does on an exception nothing handled, prints nothing of it. Windows
# Error Reporting's events name the module and the code.
function Show-Crashes([datetime] $Since) {
    $filter = @{ LogName = 'Application'; ProviderName = 'Application Error', '.NET Runtime'; StartTime = $Since }
    $crashes = @(Get-WinEvent -FilterHashtable $filter -ErrorAction SilentlyContinue | Sort-Object TimeCreated)
    if ($crashes.Count -eq 0) { return }
    Write-Output "::group::The Application log's crashes since the tests started: $($crashes.Count)"
    $token = [guid]::NewGuid().ToString('N')
    Write-Output "::stop-commands::$token"
    foreach ($crash in $crashes) {
        Write-Output "$($crash.TimeCreated.ToString('HH:mm:ss')) $($crash.ProviderName) $($crash.Id): $($crash.Message)"
    }
    Write-Output "::$token::"
    Write-Output '::endgroup::'
}

# What the Security log says failed to log the standard user on or locked it out: the event, the
# account that tried, the logon type, the process and the status. Never a SID.
function Show-LogonFailures([datetime] $Since) {
    try {
        $filter = @{ LogName = 'Security'; Id = 4625, 4648, 4740; StartTime = $Since }
        $entries = @(Get-WinEvent -FilterHashtable $filter -ErrorAction SilentlyContinue |
            Where-Object { $_.Properties.Value -contains $name } | Sort-Object TimeCreated)
        # A 4648 is this script starting a program as the user with its password, not a failure.
        $failures = @($entries | Where-Object Id -EQ 4625).Count
        $explicit = @($entries | Where-Object Id -EQ 4648).Count
        $lockouts = @($entries | Where-Object Id -EQ 4740).Count
        Write-Output "::group::The Security log on the standard user: $failures failed logons, $explicit explicit logons, $lockouts lockouts"
        $token = [guid]::NewGuid().ToString('N')
        Write-Output "::stop-commands::$token"
        foreach ($entry in $entries) {
            $v = $entry.Properties.Value
            $said = switch ($entry.Id) {
                4625 { "by $($v[1]), logon type $($v[10]), $("$($v[11])".Trim())/$($v[12]), status $('0x{0:X8}' -f $v[7]) $('0x{0:X8}' -f $v[9]), process $($v[18]), from $($v[19])" }
                4648 { "by $($v[1]), to $($v[8]) $($v[9]), process $($v[11]), from $($v[12])" }
                4740 { "by $($v[4]), from $($v[1])" }
            }
            Write-Output "$($entry.TimeCreated.ToString('HH:mm:ss.fff')) $($entry.Id): $said"
        }
        Write-Output "::$token::"
        Write-Output '::endgroup::'
    } catch {
        Write-Output "The Security log could not be read: $($_.Exception.Message)"
    }
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

# diskpart reads its commands from a file.
function Invoke-Diskpart([string] $Doing, [string[]] $Commands) {
    $file = Join-Path $logs "diskpart-$Doing.txt"
    $Commands | Set-Content -Path $file -Encoding ascii
    $said = diskpart /s $file | Out-String
    if ($LASTEXITCODE -ne 0) { throw "diskpart could not $Doing the exFAT volume (exit $LASTEXITCODE): $said" }
}

$userMade = Get-Date
$user = New-LocalUser -Name $name -Password $secure -AccountNeverExpires -PasswordNeverExpires
$sid = $user.SID.Value
$holders = $null
$vhd = $null
$volumePath = $null
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
    if ($ExFat) {
        $vhd = Join-Path $env:RUNNER_TEMP "$name-exfat.vhdx"
        $letter = [char[]](68..90) | Where-Object { -not (Test-Path -Path "$($_):\") } | Select-Object -Last 1
        Invoke-Diskpart 'make' @(
            "create vdisk file=`"$vhd`" maximum=256 type=expandable",
            "select vdisk file=`"$vhd`"",
            'attach vdisk',
            'create partition primary',
            'format fs=exfat quick label=PBTEST',
            "assign letter=$letter"
        )
        $volumePath = "$($letter):\pitboard-test"
        New-Item -ItemType Directory -Force -Path $volumePath | Out-Null
    }
    if ($Cargo) {
        New-Item -ItemType Directory -Force -Path $store | Out-Null
        $granted.Add($store)
        Set-UserRights $store 'M'
        $userNextest = Join-Path $runDir (Split-Path -Leaf $nextest)
        Copy-Item -LiteralPath $nextest -Destination $userNextest
        $binariesFile = Join-Path $runDir 'binaries-metadata.json'
        $binaries | Set-Content -Path $binariesFile
        $cargoMetadataFile = Join-Path $runDir 'cargo-metadata.json'
        $cargoMetadata | Set-Content -Path $cargoMetadataFile
    }
    if ($Exe) {
        # A copy in the run's folder, which the user may run wherever the program was built.
        $userExe = Join-Path $runDir (Split-Path -Leaf $exePath)
        Copy-Item -LiteralPath $exePath -Destination $userExe
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
    if ($volumePath) { $userEnv.PITBOARD_TEST_VOLUME = $volumePath }
    # Made by the user itself, so the folder is its own.
    if (-not (Test-Path -LiteralPath $userEnv.TEMP)) {
        $temp = Start-AsUser "$env:SystemRoot\System32\cmd.exe" @('/d', '/c', 'mkdir', $userEnv.TEMP) $runDir $userEnv 'temp'
        if ($temp.Exit -ne 0) { Show-Run $temp; throw "the standard user could not make its temporary folder" }
    }

    $testsStarted = Get-Date
    # Start-Process joins its arguments with spaces and quotes nothing, and none of these holds one.
    if ($Cargo) {
        $arguments = @('nextest', 'run', '--profile', 'ci', '--binaries-metadata', $binariesFile, '--cargo-metadata', $cargoMetadataFile)
        if ($Filter) { $arguments += @('-E', $Filter) }
        $run = Start-AsUser $userNextest $arguments $runDir $userEnv 'nextest'
        Show-Run $run
        if ($run.Exit -ne 0) { $failed.Add("nextest exited $($run.Exit)") }
    }
    for ($i = 0; $i -lt $programs.Count; $i++) {
        # Otherwise Microsoft.Testing.Platform's results go beside the program, where the user may only read.
        # A run that outlasts its timeout is cancelled and says which tests were under way, before the job's own
        # timeout would end it with nothing said. A run that stops answering does not take the timeout, so
        # HangDump names those tests, and ends it, first.
        $resultsDir = Join-Path $runDir "results-$i"
        $arguments = @('--results-directory', $resultsDir, '--timeout', '12m', '--hangdump', '--hangdump-timeout', '8m', '--hangdump-type', 'Mini')
        if ($Results) { $arguments += '--report-trx' }
        $run = Start-AsUser $programs[$i] $arguments $runDir $userEnv "dotnet-$i" -Alone
        Write-Output "::group::$($programs[$i]), as the standard user: $(if ($run.Exit -eq 0) { 'passed' } else { "exited $($run.Exit)" })"
        Show-Run $run
        Write-Output '::endgroup::'
        # The run's folder is removed with the user.
        if ($Results -and (Test-Path -LiteralPath $resultsDir)) {
            New-Item -ItemType Directory -Force -Path $Results | Out-Null
            $kept = Join-Path $Results ([IO.Path]::GetFileNameWithoutExtension($programs[$i]))
            Copy-Item -Recurse -Force -LiteralPath $resultsDir -Destination $kept
        }
        if ($run.Exit -ne 0) { $failed.Add("$($programs[$i]) exited $($run.Exit)") }
    }
    if ($failed.Count -gt 0) {
        Show-Crashes $testsStarted
        Show-LogonFailures $userMade
    }
    if ($Exe) {
        $exeEnv = $userEnv.Clone()
        foreach ($folder in 'USERPROFILE', 'HOME', 'APPDATA', 'LOCALAPPDATA', 'CODEX_HOME', 'CLAUDE_CONFIG_DIR', 'PITBOARD_HOME') {
            $exeEnv[$folder] = Join-Path $runDir "homes\$folder"
            New-Item -ItemType Directory -Force -Path $exeEnv[$folder] | Out-Null
        }
        for ($i = 0; $i -lt $Commands.Count; $i++) {
            $run = Start-AsUser $userExe (Split-Words $Commands[$i]) $runDir $exeEnv "exe-$i"
            $label = "$(Split-Path -Leaf $exePath) $($Commands[$i])".TrimEnd()
            Write-Output "::group::$label, as the standard user: exited $($run.Exit)"
            Show-Run $run
            Write-Output '::endgroup::'
            [pscustomobject]@{ Command = $Commands[$i]; Exit = $run.Exit; Out = (Get-Content -Raw -Path $run.Out) }
        }
    }

    $leaked = [System.Collections.Generic.List[string]]::new()
    if ($probePath) {
        $leaks = Start-AsUser $probePath @('credman-names', '--leak-check') $runDir $userEnv 'leak-check'
        Write-Output "::group::Credential Manager's live login families and test items, as the standard user: $(if ($leaks.Exit -eq 0) { 'none' } else { 'FOUND OR UNREADABLE' })"
        Show-Run $leaks
        Write-Output '::endgroup::'
        if ($leaks.Exit -ne 0) {
            $leaked.Add("the leak check found a live login family's or a test's item in the standard user's Credential Manager, or could not list a family (exit $($leaks.Exit))")
        }
    } else {
        Write-Output 'No probe was given, so the leak check leaves Credential Manager out.'
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
    # reg.exe exits 0 where the key or value is there, as the standard user reads its own HKCU.
    $userKeys = [ordered]@{
        'HKCU\Software\Classes\pitboard' = @('query', 'HKCU\Software\Classes\pitboard')
        'HKCU\Software\Classes\pitboard-debug' = @('query', 'HKCU\Software\Classes\pitboard-debug')
        'the Run value Pitboard' = @('query', 'HKCU\Software\Microsoft\Windows\CurrentVersion\Run', '/v', 'Pitboard')
    }
    foreach ($aumid in $notifiers) {
        $userKeys["HKCU\Software\Classes\AppUserModelId\$aumid"] = @('query', "HKCU\Software\Classes\AppUserModelId\$aumid")
    }
    $i = 0
    foreach ($key in $userKeys.Keys) {
        $query = Start-AsUser "$env:SystemRoot\System32\reg.exe" $userKeys[$key] $runDir $userEnv "registry-$i"
        $i++
        if ($query.Exit -eq 0) { $leaked.Add("the standard user's $key exists") }
    }
    $path = Start-AsUser "$env:SystemRoot\System32\reg.exe" @('query', 'HKCU\Environment', '/v', 'Path') $runDir $userEnv 'registry-path'
    if ($path.Exit -eq 0 -and (Get-Content -Raw -Path $path.Out) -match 'Programs\\Pitboard') {
        $leaked.Add("the standard user's PATH names Programs\Pitboard")
    }
    foreach ($registration in (Get-JobRegistrations)) {
        if ($jobRegistrationsBefore -notcontains $registration) { $leaked.Add("the job user's $registration appeared") }
    }
    $packages = @(Get-PitboardPackages | Where-Object { $packagesBefore -notcontains $_ })
    if ($packages.Count -gt 0) { $leaked.Add("a package named Pitboard* appeared: $($packages -join ', ')") }
    if ($leaked.Count -eq 0) {
        Write-Output "No $(if ($probePath) { 'login, test item, ' })task, login folder, registration or package was left behind."
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
    if ($vhd) {
        try { Invoke-Diskpart 'detach' @("select vdisk file=`"$vhd`"", 'detach vdisk') } catch { $failed.Add($_.Exception.Message) }
        Remove-Item -Force -Path $vhd -ErrorAction SilentlyContinue
    }
}
if ($failed.Count -gt 0) {
    throw "as a standard user: $($failed -join '; ')"
}
if ($Cargo -or $Dotnet) { Write-Output 'The tests passed as a standard user.' }
exit 0
