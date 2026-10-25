# The runner facts, gathered on a disposable GitHub-hosted Windows runner by windows-facts.yml,
# as the job's user and as throwaway users this makes. Each change it makes is undone in a
# finally: the probe's marker, VHDs, a ReFS or Dev Drive volume, an SMB share, Bun and
# proper-lockfile in C:\pitboard-probe-tools, three throwaway users and their passwords, tasks,
# the job user's HKCU\Environment values, the state of OpenSSH Server, installed from Microsoft's
# release where the image ships none, Developer Mode and the symbolic-link right, a throwaway
# certificate in LocalMachine\TrustedPeople with the probe's sparse package, PitboardShare's and
# a link scheme registered for a throwaway user, and for the channel rows Scoop in a scratch
# root, winget's local manifests and the package they install, a build on the user PATH and in
# ~\.cargo\bin, a loopback server and Defender's attack surface reduction rules. RF-T7's VHD
# holds a dummy at \home\claude, and RF-T8 writes one value to the job user's HKCU\Environment
# and broadcasts the change, and makes files in the fresh administrator's scratch folder.
#
# Everything goes into one JSON report at -Out, saved after each row, so a run cut short still
# leaves what it gathered. A fact that cannot be gathered is recorded as an error and does not
# fail the step. The step fails when the probe's own report is missing, or when any report the
# probe or a helper printed cannot be parsed: an unparsed one is kept with its parse error and
# text, which hold nothing secret, since the probe and the helpers redact what they print. No
# SID and no user name is written into the report.
param(
    [Parameter(Mandatory)] [string] $Probe,
    [Parameter(Mandatory)] [string] $Triple,
    [Parameter(Mandatory)] [string] $Out,
    # The probe built for x64, on windows-11-arm only, for the emulated process of RF-H1.
    [string] $ProbeX64 = '',
    # crates/pitboard-probe/helpers, copied to the tools folder for the Bun rows.
    [Parameter(Mandatory)] [string] $Helpers,
    [Parameter(Mandatory)] [string] $BunVersion,
    [Parameter(Mandatory)] [string] $BunZip,
    [Parameter(Mandatory)] [string] $BunSha256,
    # pitboard.exe as a 0.x release builds it, for the channel rows.
    [string] $Pitboard = '',
    # The Windows app's Release publish, PitboardShare.exe and its unsigned PitboardShare.msix
    # beside Pitboard.exe, the package's publisher CF8's certificate's subject, CN=PitboardProbe.
    [string] $App = '',
    # Win32-OpenSSH from Microsoft's release, installed where the image ships no sshd
    # (windows-11-arm), so RF-M's network logon is measured on both images. Pinned by SHA-256.
    [string] $OpenSshTag = '',
    [string] $OpenSshZip = '',
    [string] $OpenSshSha256 = ''
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'test-certificate.ps1')

# A GitHub-hosted runner is thrown away after the job.
if (-not ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted')) {
    throw 'runner-facts.ps1 runs only on a GitHub-hosted runner: it makes Windows accounts, changes their passwords, mounts disks, shares a folder, installs and starts OpenSSH Server and installs Bun for a run'
}

$Probe = (Resolve-Path -Path $Probe).Path
$Helpers = (Resolve-Path -Path $Helpers).Path
if ($Pitboard) { $Pitboard = (Resolve-Path -LiteralPath $Pitboard).Path }
if ($App) { $App = (Resolve-Path -LiteralPath $App).Path }
$cf8Publisher = 'CN=PitboardProbe'
$scratch = Join-Path $env:RUNNER_TEMP 'pitboard-probe-scratch'
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
# The probe looks for its marker in the folder FOLDERID_Profile names, which is what this
# returns.
$profileDir = [Environment]::GetFolderPath('UserProfile')
$marker = Join-Path $profileDir 'pitboard-probe-throwaway.marker'
$tools = 'C:\pitboard-probe-tools'
$toolHelpers = Join-Path $tools 'helpers'
$pwsh = (Get-Process -Id $PID).Path
$jobSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$usersSid = 'S-1-5-32-545'
$administratorsSid = 'S-1-5-32-544'
$openSshUsersSid = 'S-1-5-32-585'
# The files of the reports that ConvertFrom-Json could not read. The step fails on any.
$script:unreadable = [System.Collections.Generic.List[string]]::new()
$report = [ordered]@{ triple = $Triple }

function Save-Report {
    $report | ConvertTo-Json -Depth 64 | Set-Content -Path $Out -Encoding utf8
}

# A report the probe or a helper wrote, parsed; $null when there is none. One that does not
# parse is kept as its parse error and its text, and named in $script:unreadable.
function Read-Json([string] $Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    $text = Get-Content -Raw -LiteralPath $Path
    if ([string]::IsNullOrWhiteSpace($text)) { return $null }
    try {
        return ($text | ConvertFrom-Json)
    } catch {
        $script:unreadable.Add((Split-Path -Leaf $Path))
        return [ordered]@{ unreadable = $true; parse_error = $_.Exception.Message; text = $text }
    }
}

function Invoke-Fact([scriptblock] $Block) {
    try { return (& $Block) } catch { return [ordered]@{ error = $_.Exception.Message } }
}

# One row: its result goes into $Result under $Key, and the report is saved.
function Invoke-Row($Result, [string] $Key, [scriptblock] $Block) {
    $Result[$Key] = Invoke-Fact $Block
    Save-Report
}

function Wait-Bounded($Process, [int] $TimeoutSeconds) {
    $exited = $Process.WaitForExit($TimeoutSeconds * 1000)
    if (-not $exited) {
        taskkill /PID $Process.Id /T /F *> $null
        $null = $Process.WaitForExit(15000)
    }
    [pscustomobject]@{ TimedOut = -not $exited; Exit = $(if ($Process.HasExited) { $Process.ExitCode } else { $null }) }
}

# Output to files, not pipes: a browser the probe starts can hold an inherited pipe open.
function Invoke-Probe([string] $Label, [string[]] $ProbeArgs, [int] $TimeoutSeconds = 900) {
    $file = Join-Path $scratch "pitboard-probe-report-$Label.json"
    Remove-Item -Force -Path $file -ErrorAction SilentlyContinue
    $process = Start-Process -FilePath $Probe -ArgumentList (Format-Arguments (@('--out', $file) + $ProbeArgs)) -NoNewWindow -PassThru `
        -RedirectStandardOutput (Join-Path $scratch "job-$Label-out.txt") -RedirectStandardError (Join-Path $scratch "job-$Label-err.txt")
    $null = $process.Handle
    $waited = Wait-Bounded $process $TimeoutSeconds
    [ordered]@{ timed_out = $waited.TimedOut; exit = $waited.Exit; report = (Read-Json $file) }
}

# Start-Process joins its arguments with spaces, so one holding a space is quoted here. None
# holds a quote or ends in a backslash.
function Format-Arguments([string[]] $Arguments) {
    ($Arguments | ForEach-Object { if ($_ -eq '' -or $_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
}

function New-Password {
    ConvertTo-SecureString -String ([guid]::NewGuid().ToString('N') + 'Aa1!') -AsPlainText -Force
}

function Get-PlainText([securestring] $Secure) {
    [System.Net.NetworkCredential]::new('', $Secure).Password
}

# $Text with the throwaway user's and the job user's SIDs, profiles and names taken out.
function Hide-Account([string] $Text, $User) {
    foreach ($pair in @(
            @($User.Sid, '<sid>'), @($jobSid, '<job-sid>'),
            @($User.ProfileDir, '<profile>'), @($env:USERPROFILE, '<job-profile>'),
            @($User.Name, '<user>'), @($env:USERNAME, '<job-user>'))) {
        if ($pair[0]) { $Text = $Text -replace [regex]::Escape($pair[0]), $pair[1] }
    }
    $Text
}

# The last lines a run wrote to standard error, for a run that failed, with the accounts taken
# out.
function Get-ErrorTail([string] $Path, $User) {
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    $text = (Get-Content -Path $Path -Tail 6 -ErrorAction SilentlyContinue) -join "`n"
    if (-not $text) { return $null }
    $text = Hide-Account $text $User
    if ($text.Length -gt 800) { $text = $text.Substring($text.Length - 800) }
    $text
}

# What a run of pitboard.exe --json said, by its codes and RF-E6's fields. The task's path
# names the user's SID, so it is compared, not kept.
function Get-Envelope($Run, $User) {
    $said = [ordered]@{ started = $Run.started; timed_out = $Run.timed_out; exit = $Run.exit }
    $envelope = $Run.report
    if ($envelope -and -not $envelope.unreadable) {
        $said.ok = $envelope.ok
        $said.warnings = @($envelope.warnings | ForEach-Object { $_.code })
        if ($envelope.error) {
            $said.code = $envelope.error.code
            $said.message = Hide-Account $envelope.error.message $User
        }
        if ($envelope.data) {
            foreach ($key in 'installed', 'threshold', 'removed') {
                if ($envelope.data.PSObject.Properties.Name -contains $key) { $said[$key] = $envelope.data.$key }
            }
            if ($envelope.data.task) { $said.task_is_watch_and_the_sid = $envelope.data.task -ceq "\Pitboard\Watch $($User.Sid)" }
        }
    }
    if ($Run.error) { $said.error = Hide-Account $Run.error $User }
    if ($Run.stderr_tail) { $said.stderr_tail = $Run.stderr_tail }
    $said
}

# Start a program as $User, with its own profile's variables and any $Extra ones (a $null
# value removes one), its standard output and error sent to files the job reads.
#
# CI run 37660991344 started these with -LoadUserProfile and the job user's variables, which
# Start-Process hands a child unless told otherwise, -Credential or not: the user's Local
# AppData then named the job user's folder, which it cannot open, and SHGetKnownFolderPath
# failed. So once the first run has made the profile, every run is given the user's own
# profile variables with -Environment (PowerShell 7.4 on).
function Start-AsUser($User, [string] $Label, [string[]] $ArgumentList, [string] $FilePath = $Probe, [hashtable] $Extra = @{}) {
    $stdout = Join-Path $scratch "runas-$($User.Label)-$Label-out.txt"
    $stderr = Join-Path $scratch "runas-$($User.Label)-$Label-err.txt"
    $environment = @{}
    if ($User.Env) { foreach ($k in $User.Env.Keys) { $environment[$k] = $User.Env[$k] } }
    foreach ($k in $Extra.Keys) { $environment[$k] = $Extra[$k] }
    $start = @{
        FilePath = $FilePath
        Credential = $User.Cred
        WorkingDirectory = $User.RunDir
        LoadUserProfile = $true
        PassThru = $true
        RedirectStandardOutput = $stdout
        RedirectStandardError = $stderr
    }
    if ($ArgumentList) { $start.ArgumentList = Format-Arguments $ArgumentList }
    if ($environment.Count -gt 0) { $start.Environment = $environment }
    try {
        $process = Start-Process @start
        # Read now, so the exit code can still be read once the process has gone.
        $null = $process.Handle
        [pscustomobject]@{ Process = $process; Out = $stdout; Err = $stderr; User = $User; Failure = $null }
    } catch {
        [pscustomobject]@{ Process = $null; Out = $stdout; Err = $stderr; User = $User; Failure = $_.Exception.Message }
    }
}

# Wait for a run, ending it and its children after $TimeoutSeconds, and read what it printed:
# a report, or with -Text the last lines of plain text.
function Wait-Run($Run, [int] $TimeoutSeconds = 900, [switch] $Text) {
    if (-not $Run.Process) { return [ordered]@{ started = $false; error = $Run.Failure } }
    $waited = Wait-Bounded $Run.Process $TimeoutSeconds
    $exit = $waited.Exit
    $result = [ordered]@{
        started = $true
        environment = if ($Run.User.Env) { 'the user''s own profile variables' } else { 'the job user''s' }
        timed_out = $waited.TimedOut
        exit = $exit
        exit_hex = if ($null -ne $exit) { '0x{0:X8}' -f $exit } else { $null }
    }
    if ($Text) {
        $result.output = (Get-Content -Path $Run.Out -Tail 20 -ErrorAction SilentlyContinue) -join "`n"
    } else {
        $result.report = Read-Json $Run.Out
    }
    if ($exit -ne 0 -or ($null -eq $result.report -and -not $Text)) { $result.stderr_tail = Get-ErrorTail $Run.Err $Run.User }
    $result
}

function Invoke-AsUser($User, [string] $Label, [string[]] $ArgumentList, [string] $FilePath = $Probe, [hashtable] $Extra = @{}, [int] $TimeoutSeconds = 900, [switch] $Text) {
    Wait-Run (Start-AsUser $User $Label $ArgumentList $FilePath $Extra) $TimeoutSeconds -Text:$Text
}

# Wait until the file appears, for at most $Seconds.
function Wait-File([string] $Path, [int] $Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while (-not (Test-Path -LiteralPath $Path) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
    Test-Path -LiteralPath $Path
}

function Get-FreeLetter {
    [char[]](68..90) | Where-Object { -not (Test-Path -Path "$($_):\") } | Select-Object -Last 1
}

function Get-Policies {
    $ci = Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' -ErrorAction SilentlyContinue
    $nt = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    $saferKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\Safer\CodeIdentifiers'
    $devKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
    [ordered]@{
        os = [ordered]@{
            product = $nt.ProductName
            edition = $nt.EditionID
            installation_type = $nt.InstallationType
            display_version = $nt.DisplayVersion
            current_build = $nt.CurrentBuild
            ubr = $nt.UBR
        }
        powershell = $PSVersionTable.PSVersion.ToString()
        smart_app_control_state = if ($ci) { $ci.VerifiedAndReputablePolicyState } else { $null }
        safer_default_level = if (Test-Path $saferKey) { (Get-ItemProperty -Path $saferKey).DefaultLevel } else { $null }
        safer_policy_present = Test-Path $saferKey
        applocker_effective_rules = Invoke-Fact {
            $p = Get-AppLockerPolicy -Effective
            ($p.RuleCollections | ForEach-Object { @($_).Count } | Measure-Object -Sum).Sum
        }
        wdac = Invoke-Fact {
            $g = Get-CimInstance -Namespace 'root\Microsoft\Windows\DeviceGuard' -ClassName 'Win32_DeviceGuard'
            [ordered]@{
                kernel_code_integrity = $g.CodeIntegrityPolicyEnforcementStatus
                user_mode_code_integrity = $g.UsermodeCodeIntegrityPolicyEnforcementStatus
            }
        }
        defender = Invoke-Fact {
            $m = Get-MpComputerStatus
            [ordered]@{
                service = $m.AMServiceEnabled
                antivirus = $m.AntivirusEnabled
                real_time = $m.RealTimeProtectionEnabled
                behavior_monitor = $m.BehaviorMonitorEnabled
                tamper_protected = $m.IsTamperProtected
            }
        }
        # RF-A1: how UAC treats an administrator here.
        uac = Invoke-Fact {
            $p = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'
            [ordered]@{
                enable_lua = $p.EnableLUA
                consent_prompt_behavior_admin = $p.ConsentPromptBehaviorAdmin
                filter_administrator_token = $p.FilterAdministratorToken
                prompt_on_secure_desktop = $p.PromptOnSecureDesktop
            }
        }
        developer_mode = if (Test-Path $devKey) {
            (Get-ItemProperty -Path $devKey -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
        } else { $null }
        winget_present = $null -ne (Get-Command -Name 'winget' -ErrorAction SilentlyContinue)
        scoop_present = $null -ne (Get-Command -Name 'scoop' -ErrorAction SilentlyContinue)
        server_service = Invoke-Fact { [string] (Get-Service -Name 'LanmanServer').Status }
    }
}

# W16's volume test: can the job make, format and mount an exFAT or FAT32 VHD with diskpart,
# and what do the rename routes do there. Each volume's entry goes into $Results as soon as
# it is made, so what was gathered survives a later failure.
function Get-Vhds($Results) {
    $dir = 'C:\pitboard-probe-vhd'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    try {
        foreach ($fs in 'exfat', 'fat32') {
            $entry = [ordered]@{ fs = $fs }
            $Results.Add($entry)
            $file = Join-Path $dir "pitboard-probe-$fs.vhdx"
            $letter = Get-FreeLetter
            $create = Join-Path $dir "create-$fs.txt"
            @(
                "create vdisk file=`"$file`" maximum=256 type=expandable",
                "select vdisk file=`"$file`"",
                'attach vdisk',
                'create partition primary',
                "format fs=$fs quick label=PBPROBE",
                "assign letter=$letter"
            ) | Set-Content -Path $create -Encoding ascii
            try {
                $made = diskpart /s $create | Out-String
                $entry.diskpart_exit = $LASTEXITCODE
                $entry.mounted = Test-Path -Path "$($letter):\"
                if ($entry.mounted) {
                    $volumeScratch = "$($letter):\pitboard-probe"
                    $entry.volume = Invoke-Probe "volume-$fs" @('volume', '--scratch', $volumeScratch)
                    $entry.flush_dir = Invoke-Probe "flush-$fs" @('flush-dir', '--scratch', $volumeScratch)
                    $entry.remove_home_while_locked = Invoke-Probe "lfx-$fs" @('lockfileex', '--mode', 'remove-home', '--seconds', '5', '--scratch', $volumeScratch)
                    $entry.file_id = Invoke-Probe "fid-$fs" @('file-id', '--scratch', $volumeScratch)
                } else {
                    $entry.diskpart_tail = ($made -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -Last 4) -join ' | '
                }
            } catch {
                $entry.error = $_.Exception.Message
            } finally {
                $detach = Join-Path $dir "detach-$fs.txt"
                @("select vdisk file=`"$file`"", 'detach vdisk') | Set-Content -Path $detach -Encoding ascii
                diskpart /s $detach *> $null
                $entry.detach_exit = $LASTEXITCODE
                Remove-Item -Force -Path $file -ErrorAction SilentlyContinue
            }
        }
    } finally {
        Remove-Item -Recurse -Force -Path $dir -ErrorAction SilentlyContinue
    }
}

# RF-Bun and RF-Lock: Bun from its release zip, only when its SHA-256 is the one the workflow
# pinned or was given, and proper-lockfile from the committed bun.lock with install scripts
# off, both under $tools, which every local user may read and run. Hands back bun.exe's path,
# or $null.
function Install-ProbeTools($Result, [string] $Version, [string] $Zip, [string] $Sha256) {
    $Result.bun_version = $Version
    $Result.bun_zip = $Zip
    New-Item -ItemType Directory -Force -Path $tools | Out-Null
    $download = Join-Path $tools $Zip
    Invoke-WebRequest -Uri "https://github.com/oven-sh/bun/releases/download/bun-v$Version/$Zip" -OutFile $download -UseBasicParsing
    $Result.sha256_matches = (Get-FileHash -Algorithm SHA256 -Path $download).Hash -eq $Sha256
    if (-not $Result.sha256_matches) { throw "$Zip is not the build whose SHA-256 the workflow pinned; nothing of it runs" }
    $unpacked = Join-Path $tools 'unpacked'
    Expand-Archive -Path $download -DestinationPath $unpacked -Force
    $bunDir = Join-Path $tools 'bun'
    New-Item -ItemType Directory -Force -Path $bunDir | Out-Null
    $bun = Join-Path $bunDir 'bun.exe'
    Move-Item -Path (Join-Path $unpacked ([IO.Path]::GetFileNameWithoutExtension($Zip) + '\bun.exe')) -Destination $bun
    Remove-Item -Recurse -Force -Path $unpacked, $download
    $Result.bun_reports = (& $bun --version | Out-String).Trim()

    Copy-Item -Recurse -Path $Helpers -Destination $toolHelpers
    $install = Join-Path $scratch 'pitboard-probe-bun-install.txt'
    $env:BUN_INSTALL_CACHE_DIR = Join-Path $tools 'cache'
    try {
        & $bun install --cwd $toolHelpers --frozen-lockfile --ignore-scripts *> $install
        $Result.install_exit = $LASTEXITCODE
    } finally {
        Remove-Item -Path Env:BUN_INSTALL_CACHE_DIR -ErrorAction SilentlyContinue
    }
    $Result.resolved = [ordered]@{}
    foreach ($package in 'proper-lockfile', 'graceful-fs', 'retry', 'signal-exit') {
        $manifest = Join-Path $toolHelpers "node_modules\$package\package.json"
        $Result.resolved[$package] = if (Test-Path -LiteralPath $manifest) { (Get-Content -Raw -LiteralPath $manifest | ConvertFrom-Json).version } else { $null }
    }
    icacls $tools /grant "*${usersSid}:(OI)(CI)RX" /T /Q *> $null
    $Result.granted_exit = $LASTEXITCODE
    if ($Result.install_exit -ne 0) { $Result.install_tail = (Get-Content -Path $install -Tail 6) -join "`n" }
    $bun
}

# A throwaway local user in $GroupSids, its run folder at the drive root (one it cannot open
# fails with ERROR_DIRECTORY), its profile made by a first run, and the probe's marker in it.
# Handed back once the account exists, whatever fails after, so the caller removes it.
function New-ThrowawayUser([string] $Label, [string] $Name, [string[]] $GroupSids, $Result) {
    $secure = New-Password
    $made = New-LocalUser -Name $Name -Password $secure -AccountNeverExpires -PasswordNeverExpires
    $user = [pscustomobject]@{
        Label = $Label
        Name = $Name
        Sid = $made.SID.Value
        Secure = $secure
        Cred = [pscredential]::new($Name, $secure)
        RunDir = "C:\pitboard-probe-runas-$Label"
        ProfileDir = $null
        Env = $null
        Scratch = $null
        PasswordReset = $false
    }
    try {
        foreach ($group in $GroupSids) { Add-LocalGroupMember -SID $group -Member $made -ErrorAction SilentlyContinue }
        New-Item -ItemType Directory -Force -Path $user.RunDir | Out-Null
        icacls $user.RunDir /grant "*$($user.Sid):(OI)(CI)M" *> $null
        $Result.start_process_takes_environment = (Get-Command -Name 'Start-Process').Parameters.ContainsKey('Environment')
        $Result.logon = Invoke-AsUser $user 'logon' @('logon')
        $Result.unsigned_probe_ran = ($Result.logon.started -and $Result.logon.exit -eq 0)
        $user.ProfileDir = (Get-CimInstance -ClassName Win32_UserProfile | Where-Object { $_.SID -eq $user.Sid }).LocalPath
        $Result.profile_made = [bool] $user.ProfileDir
        if ($user.ProfileDir) {
            Set-Content -Path (Join-Path $user.ProfileDir 'pitboard-probe-throwaway.marker') -Value 'ci runner, disposable'
            if ($Result.start_process_takes_environment) {
                $user.Env = @{
                    USERPROFILE = $user.ProfileDir
                    HOMEDRIVE = Split-Path -Path $user.ProfileDir -Qualifier
                    HOMEPATH = Split-Path -Path $user.ProfileDir -NoQualifier
                    APPDATA = Join-Path $user.ProfileDir 'AppData\Roaming'
                    LOCALAPPDATA = Join-Path $user.ProfileDir 'AppData\Local'
                    TEMP = Join-Path $user.ProfileDir 'AppData\Local\Temp'
                    TMP = Join-Path $user.ProfileDir 'AppData\Local\Temp'
                    USERNAME = $Name
                }
            }
            $user.Scratch = Join-Path $user.ProfileDir 'AppData\Local\Temp\pitboard-probe-s'
        }
    } catch {
        $Result.setup_error = $_.Exception.Message
        $user.ProfileDir = $null
    }
    $user
}

# The user's leak check first, unless an administrator's reset has left its vault unreadable,
# then the user, its processes, its profile and its run folder go.
function Remove-ThrowawayUser($User, $Result) {
    if ($User.ProfileDir -and -not $User.PasswordReset) {
        $Result.leak_check = Invoke-AsUser $User 'leak-check' @('credman-names', '--leak-check')
    }
    taskkill /F /FI "USERNAME eq $($User.Name)" /IM * *> $null
    Start-Sleep -Seconds 3
    Remove-LocalUser -SID $User.Sid -ErrorAction SilentlyContinue
    Get-CimInstance -ClassName Win32_UserProfile | Where-Object { $_.SID -eq $User.Sid } |
        Remove-CimInstance -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force -Path $User.RunDir -ErrorAction SilentlyContinue
    $Result.removed = $null -eq (Get-LocalUser -SID $User.Sid -ErrorAction SilentlyContinue)
    $Result.profile_removed = $null -eq (Get-CimInstance -ClassName Win32_UserProfile | Where-Object { $_.SID -eq $User.Sid })
}

# Developer Mode's registry value as it stands: whether the key and the value exist, and the
# value and its kind.
function Get-DevMode {
    $key = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
    $name = 'AllowDevelopmentWithoutDevLicense'
    $state = [ordered]@{ key = (Test-Path -Path $key); has_value = $false; value = $null; kind = $null }
    if ($state.key) {
        $k = Get-Item -Path $key
        if ($k.GetValueNames() -contains $name) {
            $state.has_value = $true
            $state.value = $k.GetValue($name, $null, 'DoNotExpandEnvironmentNames')
            $state.kind = $k.GetValueKind($name)
        }
    }
    $state
}

# The accounts holding the symbolic-link right, as secedit exports them (SIDs, sorted, one
# string): compared only, never written into the report.
function Get-SymlinkHolders([string] $Export) {
    secedit /export /cfg $Export /areas USER_RIGHTS *> $null
    $line = Get-Content -Path $Export | Where-Object { $_ -match '^SeCreateSymbolicLinkPrivilege\s*=' } | Select-Object -First 1
    Remove-Item -Force -Path $Export -ErrorAction SilentlyContinue
    if (-not $line) { return '' }
    ((($line -split '=', 2)[1] -split ',') | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Sort-Object) -join ','
}

# A template that sets the symbolic-link right to exactly $Holders, and nobody else when it
# is empty, and applies it with a database of its own.
function Set-SymlinkHolders([string] $Holders, [string] $Label) {
    $inf = Join-Path $scratch "pitboard-probe-rights-$Label.inf"
    $db = Join-Path $scratch "pitboard-probe-rights-$Label.sdb"
    @(
        '[Unicode]', 'Unicode=yes',
        '[Version]', 'signature="$CHICAGO$"', 'Revision=1',
        '[Privilege Rights]', "SeCreateSymbolicLinkPrivilege = $Holders"
    ) | Set-Content -Path $inf -Encoding Unicode
    secedit /configure /db $db /cfg $inf /areas USER_RIGHTS *> $null
    $exit = $LASTEXITCODE
    Remove-Item -Force -Path $inf, $db -ErrorAction SilentlyContinue
    $exit
}

# W12 and W13's symbolic links: as the user is, with Developer Mode on for one run, and with
# the right granted for one run, each put back exactly.
function Measure-Symlinks($User, $Result) {
    $Result.symlink_as_is = Invoke-AsUser $User 'sym-as-is' @('symlink', '--scratch', (Join-Path $User.RunDir 'as-is'))
    # The key is made only when it is missing (New-Item -Force on an existing key would replace
    # it and drop its other values), and the value and its kind are put back exactly as they
    # were, or removed with the key when the run made them.
    try {
        $devKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
        $devBefore = Get-DevMode
        try {
            if (-not $devBefore.key) { New-Item -Path $devKey | Out-Null }
            Set-ItemProperty -Path $devKey -Name 'AllowDevelopmentWithoutDevLicense' -Value 1 -Type DWord
            $Result.symlink_developer_mode = Invoke-AsUser $User 'sym-devmode' @('symlink', '--scratch', (Join-Path $User.RunDir 'devmode'))
        } finally {
            if (-not $devBefore.key) {
                Remove-Item -Path $devKey -ErrorAction SilentlyContinue
            } elseif (-not $devBefore.has_value) {
                Remove-ItemProperty -Path $devKey -Name 'AllowDevelopmentWithoutDevLicense' -ErrorAction SilentlyContinue
            } else {
                Set-ItemProperty -Path $devKey -Name 'AllowDevelopmentWithoutDevLicense' -Value $devBefore.value -Type $devBefore.kind
            }
            $Result.developer_mode_restored_exactly = (
                (Get-DevMode | ConvertTo-Json -Compress) -eq ($devBefore | ConvertTo-Json -Compress))
        }
    } catch {
        $Result.developer_mode_error = $_.Exception.Message
    }
    # Added for this user beside the accounts that already held it, then set back to exactly
    # those accounts, which is nobody when the export had no line for it; read back and
    # compared.
    try {
        $holders = Get-SymlinkHolders (Join-Path $scratch 'pitboard-probe-rights-before.inf')
        $Result.right_held_by_anyone_before = [bool] $holders
        $grant = if ($holders) { "$holders,*$($User.Sid)" } else { "*$($User.Sid)" }
        $Result.right_granted_exit = Set-SymlinkHolders $grant 'grant'
        try {
            $Result.symlink_right_granted = Invoke-AsUser $User 'sym-right' @('symlink', '--scratch', (Join-Path $User.RunDir 'right'))
        } finally {
            $Result.right_restore_exit = Set-SymlinkHolders $holders 'restore'
            $Result.right_restored_exactly = (
                (Get-SymlinkHolders (Join-Path $scratch 'pitboard-probe-rights-after.inf')) -eq $holders)
        }
    } catch {
        $Result.right_error = $_.Exception.Message
    }
}

# RF-C4: the replace loop beside a probe reader and Bun's, first by MoveFileExW with readers
# that do not share delete, then by the POSIX rename with readers that do, Bun's holding each
# open 50 ms as a tool keeps its login open.
function Measure-ReplaceLoop($User, [string] $Folder, [string] $Label, [int] $Rounds, [int] $Seconds, [string] $Bun) {
    $readerLoop = Join-Path $toolHelpers 'reader-loop.mjs'
    $passes = [ordered]@{}
    foreach ($pass in @(
            @{ name = 'movefile_beside_readers_not_sharing_delete'; route = 'movefile'; share = 'no-delete'; hold = '0' },
            @{ name = 'posix_beside_readers_sharing_delete'; route = 'posix'; share = 'delete'; hold = '50' })) {
        $tag = "$Label-$($pass.route)"
        $reader = Start-AsUser $User "$tag-reader" @('replace-reader', '--scratch', $Folder, '--share', $pass.share, '--seconds', '300')
        $bunReader = if ($Bun) { Start-AsUser $User "$tag-bun" @($readerLoop, $Folder, '300', $pass.hold) -FilePath $Bun }
        $loop = Invoke-AsUser $User "$tag-loop" @('replace-loop', '--scratch', $Folder, '--rounds', [string] $Rounds,
            '--lead-seconds', '10', '--route', $pass.route, '--seconds', [string] $Seconds) -TimeoutSeconds ($Seconds + 180)
        $passes[$pass.name] = [ordered]@{
            loop = $loop
            probe_reader = Wait-Run $reader 120
            bun_reader = if ($bunReader) { Wait-Run $bunReader 120 } else { $null }
        }
        Save-Report
    }
    $passes
}

# RF-C5 and RF-C5r: the probe holds proper-lockfile's lock while Bun's proper-lockfile checks
# it, then the other way round, with one of Claude Code's two timings.
function Measure-Lock($User, [string] $Folder, [string] $Timing, [string] $Bun) {
    $lockLoop = Join-Path $toolHelpers 'proper-lockfile-loop.mjs'
    $hold = Start-AsUser $User "c5-$Timing-hold" @('lock', '--scratch', $Folder, '--mode', 'hold', '--seconds', '30', '--timing', $Timing)
    Start-Sleep -Seconds 2
    $bunCheck = Invoke-AsUser $User "c5-$Timing-bun-check" @($lockLoop, $Folder, 'check', '25', $Timing) -FilePath $Bun -TimeoutSeconds 120
    $probeHold = Wait-Run $hold 120
    $bunHold = Start-AsUser $User "c5-$Timing-bun-hold" @($lockLoop, $Folder, 'hold', '30', $Timing) -FilePath $Bun
    Start-Sleep -Seconds 3
    $probeCheck = Invoke-AsUser $User "c5-$Timing-check" @('lock', '--scratch', $Folder, '--mode', 'check', '--seconds', '25', '--timing', $Timing) -TimeoutSeconds 120
    [ordered]@{
        probe_holds = [ordered]@{ probe = $probeHold; bun_checks = $bunCheck }
        bun_holds = [ordered]@{ bun = (Wait-Run $bunHold 120); probe_checks = $probeCheck }
    }
}

# RF-C6: LockFileEx on a home's state.lock held by one process and read by another, then the
# home removed while a holder runs.
function Measure-LockFileEx($User, [string] $Folder, [string] $Label) {
    $hold = Start-AsUser $User "$Label-hold" @('lockfileex', '--scratch', $Folder, '--mode', 'hold', '--seconds', '15')
    $ready = Wait-File (Join-Path $Folder 'pitboard-probe-home\pitboard-probe-ready') 20
    $read = Invoke-AsUser $User "$Label-read" @('lockfileex', '--scratch', $Folder, '--mode', 'read')
    [ordered]@{
        holder_ready = $ready
        read = $read
        hold = Wait-Run $hold 60
        remove_home = Invoke-AsUser $User "$Label-remove-home" @('lockfileex', '--scratch', $Folder, '--mode', 'remove-home', '--seconds', '5')
    }
}

# RF-E1, RF-E3 and RF-D2's tasks: a probe task read before it is there, then registered in
# $TaskFolder, run, read and deleted, by the job's user or by $User, with $In as the probe's
# scratch folder. With $Expect, the report the task's action writes there is waited for and
# read.
function Measure-Task($User, [string] $In, [string] $Label, [string] $TaskFolder, [string] $Action, [switch] $SidSuffix, [int] $WaitSeconds = 5, [string] $Expect = '') {
    $base = @('task', '--scratch', $In, '--label', $Label, '--folder', $TaskFolder)
    if ($SidSuffix) { $base += '--sid-suffix' }
    $call = {
        param([string] $Step, [string[]] $More)
        $all = $base + @('--action', $Step) + $More
        if ($User) { Invoke-AsUser $User "task-$Label-$TaskFolder-$Step" $all } else { Invoke-Probe "task-$Label-$TaskFolder-$Step" $all }
    }
    $result = [ordered]@{ read_before = (& $call 'read' @()) }
    $result.register = & $call 'register' @('--args', $Action)
    $result.run = & $call 'run' @()
    if ($Expect) { $result.output_appeared = Wait-File $Expect $WaitSeconds } else { Start-Sleep -Seconds $WaitSeconds }
    $result.read = & $call 'read' @()
    if ($Expect) { $result.output = Read-Json $Expect }
    $result.delete = & $call 'delete' @()
    $result
}

# RF-E6: the watch service's task as a fresh standard user's pitboard.exe installs, installs
# again and uninstalls it in the user's default homes, read by the user's probe at each step,
# then what is left. With no signed-in session the user's task never starts (RF-E1).
function Measure-WatchService($User) {
    if (-not $Pitboard) { throw 'no -Pitboard build was given' }
    $defaults = @{ HOME = $null; PITBOARD_HOME = $null; CLAUDE_CONFIG_DIR = $null; CODEX_HOME = $null }
    $read = { param([string] $Step) Invoke-AsUser $User "e6-read-$Step" @('task', '--action', 'read', '--watch-service') }
    $cli = {
        param([string] $Step, [string[]] $Arguments)
        Get-Envelope (Invoke-AsUser $User "e6-$Step" ($Arguments + '--json') -FilePath $Pitboard -Extra $defaults -TimeoutSeconds 120) $User
    }
    $runs = {
        @(Get-CimInstance -ClassName Win32_Process -Filter "Name = 'pitboard.exe'" | Where-Object {
                (Invoke-CimMethod -InputObject $_ -MethodName GetOwner -ErrorAction SilentlyContinue).User -eq $User.Name
            }).Count
    }
    $schedule = { $service = New-Object -ComObject 'Schedule.Service'; $service.Connect(); $service }
    $pitboardFolder = { try { (& $schedule).GetFolder('\Pitboard') } catch { $null } }
    $pitboardHome = Join-Path $User.ProfileDir 'AppData\Local\Pitboard'
    $r = [ordered]@{
        home_before = Test-Path -LiteralPath $pitboardHome
        folder_before = [bool] (& $pitboardFolder)
        read_before = & $read 'before'
    }
    try {
        $r.install = & $cli 'install' @('watch', 'install')
        Start-Sleep -Seconds 5
        $r.read = & $read 'installed'
        $r.runs = & $runs
        $r.status = & $cli 'status' @('watch', 'status')
        $r.install_again = & $cli 'install-again' @('watch', 'install', '--at', '90')
        Start-Sleep -Seconds 5
        $r.read_again = & $read 'installed-again'
        $r.status_again = & $cli 'status-again' @('watch', 'status')
        $r.uninstall = & $cli 'uninstall' @('watch', 'uninstall')
        $r.read_after = & $read 'after'
        $r.status_after = & $cli 'status-after' @('watch', 'status')
        $r.runs_after = & $runs
        $r.home_after = if (Test-Path -LiteralPath $pitboardHome) { @(Get-ChildItem -LiteralPath $pitboardHome -Recurse -Force -Name | Sort-Object) } else { $null }
    } finally {
        # What the uninstall left would change what the job user's RF-E1 row reads of \Pitboard\.
        # A task left in it keeps the folder from going.
        $r.left_its_task = Invoke-Fact {
            $task = "Watch $($User.Sid)"
            $folder = & $pitboardFolder
            if ($folder -and $(try { $folder.GetTask($task) } catch { $null })) { $folder.DeleteTask($task, 0); $true } else { $false }
        }
        $r.left_its_folder = Invoke-Fact {
            $folder = & $pitboardFolder
            if ($folder -and -not $r.folder_before -and $folder.GetTasks(1).Count -eq 0 -and $folder.GetFolders(0).Count -eq 0) {
                (& $schedule).GetFolder('\').DeleteFolder('Pitboard', 0)
                $true
            } else { $false }
        }
    }
    $r
}

# RF-E6 as the job user, whose tasks start (RF-E1): two probe tasks shaped as the watch
# service's, each holding a lock in place of `pitboard watch`, one with no time limit (PT0S) and
# one with PT1M. Past the second's limit the first is stopped, then stopped again.
function Measure-WatchTask([string] $In) {
    $limits = [ordered]@{ 'e6-pt0s' = 'PT0S'; 'e6-pt1m' = 'PT1M' }
    $call = {
        param([string] $Label, [string] $Step, [string[]] $More = @())
        Invoke-Probe "$Label-$Step" (@('task', '--scratch', $In, '--label', $Label, '--action', $Step) + $More)
    }
    # A hold is told by the folder in the command line its task started it with.
    $holds = {
        param([string] $Label)
        @(Get-CimInstance -ClassName Win32_Process -Filter "Name = 'pitboard-probe.exe'" |
                Where-Object { $_.CommandLine -and $_.CommandLine.Contains("--scratch $(Join-Path $In $Label) ") })
    }
    $r = [ordered]@{}
    try {
        foreach ($label in $limits.Keys) {
            $hold = "lockfileex --scratch $(Join-Path $In $label) --mode hold --seconds 900"
            $r[$label] = [ordered]@{
                register = & $call $label 'register' @('--logon', '--time-limit', $limits[$label], '--allow-battery', '--start-when-available', '--args', $hold)
            }
        }
        $started = Get-Date
        foreach ($label in $limits.Keys) { $r[$label].run = & $call $label 'run' }
        foreach ($label in $limits.Keys) {
            $r[$label].ready = Wait-File (Join-Path $In "$label\pitboard-probe-home\pitboard-probe-ready") 30
            $r[$label].ready_after_seconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
            $r[$label].read_running = & $call $label 'read'
            $r[$label].holds_running = (& $holds $label).Count
        }
        $wait = ($started.AddSeconds(150) - (Get-Date)).TotalSeconds
        if ($wait -gt 0) { Start-Sleep -Seconds ([int] [math]::Ceiling($wait)) }
        $r.read_later_after_seconds = [int] ((Get-Date) - $started).TotalSeconds
        foreach ($label in $limits.Keys) {
            $r[$label].read_later = & $call $label 'read'
            $r[$label].holds_later = (& $holds $label).Count
        }
        $unlimited = $r['e6-pt0s']
        $unlimited.stop = & $call 'e6-pt0s' 'stop'
        $stopped = Get-Date
        while ((& $holds 'e6-pt0s').Count -gt 0 -and ((Get-Date) - $stopped).TotalSeconds -lt 30) { Start-Sleep -Milliseconds 200 }
        $unlimited.holds_after_stop = (& $holds 'e6-pt0s').Count
        $unlimited.stop_took_seconds = [math]::Round(((Get-Date) - $stopped).TotalSeconds, 1)
        $unlimited.read_stopped = & $call 'e6-pt0s' 'read'
        $unlimited.stop_again = & $call 'e6-pt0s' 'stop'
    } finally {
        foreach ($label in $limits.Keys) {
            if (-not $r.Contains($label)) { continue }
            $r[$label].delete = & $call $label 'delete'
            $left = & $holds $label
            $r[$label].holds_left = $left.Count
            $left | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
        }
    }
    $r
}

# RF-C3r: a 51 GB expandable VHDX made ReFS, as a Dev Drive where the system is Windows 11
# (format fs=refs is a server's) and plain where it is Windows Server; the rename routes, the
# replace loop and LockFileEx there, as the job's user and as $User.
function Measure-ReFs($User, $Result, [string] $Bun) {
    $dir = 'C:\pitboard-probe-refs'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $file = Join-Path $dir 'pitboard-probe-refs.vhdx'
    $letter = Get-FreeLetter
    $devDrive = (Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').InstallationType -ne 'Server'
    $Result.kind = if ($devDrive) { 'dev_drive' } else { 'refs' }
    $create = Join-Path $dir 'create.txt'
    $lines = @(
        "create vdisk file=`"$file`" maximum=52224 type=expandable",
        "select vdisk file=`"$file`"",
        'attach vdisk',
        'create partition primary'
    )
    if (-not $devDrive) { $lines += 'format fs=refs quick label=PBREFS' }
    $lines += "assign letter=$letter"
    $lines | Set-Content -Path $create -Encoding ascii
    try {
        $made = diskpart /s $create | Out-String
        $Result.diskpart_exit = $LASTEXITCODE
        if ($devDrive) {
            $Result.format = Invoke-Fact {
                $formatted = Format-Volume -DriveLetter $letter -DevDrive -NewFileSystemLabel 'PBDEV' -Confirm:$false -Force
                [ordered]@{ file_system = $formatted.FileSystem }
            }
            $Result.devdrv_query = (fsutil devdrv query "$($letter):" 2>&1 | Out-String).Trim()
        }
        $Result.mounted = Test-Path -Path "$($letter):\"
        if (-not $Result.mounted) {
            $Result.diskpart_tail = ($made -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -Last 4) -join ' | '
            return
        }
        $root = "$($letter):\pitboard-probe"
        New-Item -ItemType Directory -Force -Path $root | Out-Null
        icacls $root /grant "*$($User.Sid):(OI)(CI)M" *> $null
        $Result.job = [ordered]@{
            volume = Invoke-Probe 'refs-volume' @('volume', '--scratch', (Join-Path $root 'job'))
            flush_dir = Invoke-Probe 'refs-flush' @('flush-dir', '--scratch', (Join-Path $root 'job'))
            file_id = Invoke-Probe 'refs-fid' @('file-id', '--scratch', (Join-Path $root 'job'))
        }
        Save-Report
        $userFolder = Join-Path $root 'user'
        $Result.user = [ordered]@{
            volume = Invoke-AsUser $User 'refs-volume' @('volume', '--scratch', $userFolder)
            flush_dir = Invoke-AsUser $User 'refs-flush' @('flush-dir', '--scratch', $userFolder)
        }
        Save-Report
        $Result.user.replace_loop = Measure-ReplaceLoop $User $userFolder 'refs' 2000 90 $Bun
        $Result.user.lockfileex = Measure-LockFileEx $User (Join-Path $root 'user-lfx') 'refs-lfx'
    } finally {
        $detach = Join-Path $dir 'detach.txt'
        @("select vdisk file=`"$file`"", 'detach vdisk') | Set-Content -Path $detach -Encoding ascii
        diskpart /s $detach *> $null
        $Result.detach_exit = $LASTEXITCODE
        Remove-Item -Recurse -Force -Path $dir -ErrorAction SilentlyContinue
        $Result.removed = -not (Test-Path -Path $dir)
    }
}

# RF-C3s: a folder shared over SMB to $User alone, mapped as P: inside a process of $User's
# own, where the probe reads the volume and flushes a folder.
function Measure-SmbShare($User, $Result) {
    $dir = 'C:\pitboard-probe-share'
    $share = 'pitboard-probe-share'
    try {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        icacls $dir /grant "*$($User.Sid):(OI)(CI)M" *> $null
        New-SmbShare -Name $share -Path $dir -FullAccess "$env:COMPUTERNAME\$($User.Name)" | Out-Null
        $Result.shared = $true
        $mapper = Join-Path $User.RunDir 'pitboard-probe-c3s.ps1'
        @(
            'param([string] $Probe, [string] $Out)',
            "net use P: \\localhost\$share /persistent:no *> `$null",
            '"mapped $LASTEXITCODE"',
            '& $Probe --out (Join-Path $Out ''pitboard-probe-c3s-volume.json'') volume --scratch P:\pitboard-probe *> $null',
            '& $Probe --out (Join-Path $Out ''pitboard-probe-c3s-flush.json'') flush-dir --scratch P:\pitboard-probe *> $null',
            '& $Probe --out (Join-Path $Out ''pitboard-probe-c3s-fid.json'') file-id --scratch P:\pitboard-probe *> $null',
            'net use P: /delete /y *> $null',
            '"unmapped $LASTEXITCODE"'
        ) | Set-Content -Path $mapper -Encoding ascii
        $Result.user_run = Invoke-AsUser $User 'c3s' @('-NoProfile', '-NonInteractive', '-File', $mapper, '-Probe', $Probe, '-Out', $User.RunDir) -FilePath $pwsh -Text
        $Result.volume = Read-Json (Join-Path $User.RunDir 'pitboard-probe-c3s-volume.json')
        $Result.flush_dir = Read-Json (Join-Path $User.RunDir 'pitboard-probe-c3s-flush.json')
        $Result.file_id = Read-Json (Join-Path $User.RunDir 'pitboard-probe-c3s-fid.json')
    } finally {
        Remove-SmbShare -Name $share -Force -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force -Path $dir -ErrorAction SilentlyContinue
        $Result.unshared = $null -eq (Get-SmbShare -Name $share -ErrorAction SilentlyContinue)
    }
}

# Run a command over ssh as $User@localhost with the key, and read the report it prints.
function Invoke-Ssh($User, [string] $Label, [string[]] $SshArgs, [string] $Command) {
    $stdout = Join-Path $scratch "ssh-$Label-out.txt"
    $stderr = Join-Path $scratch "ssh-$Label-err.txt"
    & ssh.exe @SshArgs $Command > $stdout 2> $stderr
    $exit = $LASTEXITCODE
    $result = [ordered]@{ exit = $exit; report = (Read-Json $stdout) }
    if ($exit -ne 0) { $result.stderr_tail = Get-ErrorTail $stderr $User }
    $result
}

# Why sshd turns a session down, when it does: its effective settings for $User, who holds
# the network logon rights it needs, and its last events, with the user's name taken out.
function Get-SshdDiagnosis($User) {
    $sshd = Join-Path $env:SystemRoot 'System32\OpenSSH\sshd.exe'
    $wanted = '^(allowusers|allowgroups|denyusers|denygroups|authorizedkeysfile|pubkeyauthentication|passwordauthentication|authenticationmethods|strictmodes) '
    $export = Join-Path $scratch 'pitboard-probe-rights-ssh.inf'
    secedit /export /cfg $export /areas USER_RIGHTS *> $null
    $rights = Get-Content -Path $export -ErrorAction SilentlyContinue
    Remove-Item -Force -Path $export -ErrorAction SilentlyContinue
    $holders = {
        param([string] $Right)
        $line = $rights | Where-Object { $_ -match "^$Right\s*=" } | Select-Object -First 1
        $sids = if ($line) { @((($line -split '=', 2)[1] -split ',') | ForEach-Object { $_.Trim().TrimStart('*') }) } else { @() }
        [ordered]@{
            everyone = $sids -contains 'S-1-1-0'
            users = $sids -contains $usersSid
            authenticated_users = $sids -contains 'S-1-5-11'
            this_user = $sids -contains $User.Sid
            administrators = $sids -contains $administratorsSid
        }
    }
    [ordered]@{
        effective = Invoke-Fact {
            @(& $sshd -T -C "user=$($User.Name),host=localhost,addr=127.0.0.1" 2>&1 | ForEach-Object { "$_" } |
                Where-Object { $_ -match $wanted } | ForEach-Object { $_ -replace [regex]::Escape($User.Name), '<user>' })
        }
        config = Invoke-Fact {
            @(Get-Content -Path (Join-Path $env:ProgramData 'ssh\sshd_config') |
                Where-Object { $_ -match '^\s*(AllowGroups|AllowUsers|DenyGroups|DenyUsers|Match|AuthorizedKeysFile|PasswordAuthentication|PubkeyAuthentication)\b' } |
                ForEach-Object { $_.Trim() })
        }
        network_logon_right = & $holders 'SeNetworkLogonRight'
        deny_network_logon_right = & $holders 'SeDenyNetworkLogonRight'
        events = Invoke-Fact {
            @(Get-WinEvent -LogName 'OpenSSH/Operational' -MaxEvents 15 -ErrorAction Stop | ForEach-Object {
                    (($_.Message -replace [regex]::Escape($User.Name), '<user>') -split "`r?`n")[0]
                })
        }
    }
}

# WF-b: install Win32-OpenSSH from Microsoft's release where the image ships none
# (windows-11-arm), so RF-M measures the network logon on both images. The zip is taken only
# when its SHA-256 is the one the workflow pinned. Uninstall-OpenSsh removes the service and
# the files. Hands back the install folder, or $null when the image already has sshd or no zip
# was given.
function Install-OpenSsh($Result, [string] $Tag, [string] $Zip, [string] $Sha256) {
    if (Get-Service -Name 'sshd' -ErrorAction SilentlyContinue) { $Result.shipped_sshd = $true; return $null }
    $Result.shipped_sshd = $false
    if (-not $Zip) { return $null }
    $dir = 'C:\pitboard-probe-openssh'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $download = Join-Path $dir $Zip
    Invoke-WebRequest -Uri "https://github.com/PowerShell/Win32-OpenSSH/releases/download/$Tag/$Zip" -OutFile $download -UseBasicParsing
    $Result.sha256_matches = (Get-FileHash -Algorithm SHA256 -Path $download).Hash -eq $Sha256
    if (-not $Result.sha256_matches) { throw "$Zip is not the Win32-OpenSSH build whose SHA-256 the workflow pinned; nothing of it is installed" }
    $configDir = Join-Path $env:ProgramData 'ssh'
    # Whether this run makes the config folder, so Uninstall-OpenSsh removes only what it made,
    # and the ssh-agent the image ships, since install-sshd.ps1 deletes and recreates it and
    # uninstall-sshd.ps1 then removes it.
    $Result.config_existed = Test-Path -LiteralPath $configDir
    $agent = Get-CimInstance -ClassName Win32_Service -Filter "Name='ssh-agent'" -ErrorAction SilentlyContinue
    $Result.ssh_agent_before = if ($agent) {
        [ordered]@{ present = $true; start_mode = [string] $agent.StartMode; path = [string] $agent.PathName }
    } else {
        [ordered]@{ present = $false }
    }
    $sshHome = $null
    try {
        Expand-Archive -Path $download -DestinationPath $dir -Force
        $sshHome = Get-ChildItem -Path $dir -Directory | Select-Object -First 1 -ExpandProperty FullName
        if (-not $sshHome) { throw 'the Win32-OpenSSH zip held no folder' }
        & (Join-Path $sshHome 'install-sshd.ps1') *> (Join-Path $scratch 'openssh-install.txt')
        $Result.install_exit = $LASTEXITCODE
        $env:PATH = "$sshHome;$env:PATH"
        New-Item -ItemType Directory -Force -Path $configDir | Out-Null
        $config = Join-Path $configDir 'sshd_config'
        if (-not (Test-Path -LiteralPath $config)) { Copy-Item -Path (Join-Path $sshHome 'sshd_config_default') -Destination $config }
        & (Join-Path $sshHome 'ssh-keygen.exe') -A *> (Join-Path $scratch 'openssh-keygen.txt')
        try { & (Join-Path $sshHome 'FixHostFilePermissions.ps1') -Confirm:$false *> (Join-Path $scratch 'openssh-fixperms.txt') } catch { $null = $_ }
        Set-Service -Name 'sshd' -StartupType Manual -ErrorAction SilentlyContinue
        $Result.installed = [bool] (Get-Service -Name 'sshd' -ErrorAction SilentlyContinue)
        $Result.tag = $Tag
    } catch {
        Uninstall-OpenSsh $sshHome $Result
        throw
    }
    $sshHome
}

# Remove what Install-OpenSsh added: the sshd service, the unpacked files and, when this run
# made it, the whole %ProgramData%\ssh folder. uninstall-sshd.ps1 also removes the ssh-agent
# service, so the image's own is recreated from what Install-OpenSsh recorded.
function Uninstall-OpenSsh([string] $SshHome, $Result) {
    try {
        Stop-Service -Name 'sshd' -Force -ErrorAction SilentlyContinue
        Stop-Service -Name 'ssh-agent' -Force -ErrorAction SilentlyContinue
        if ($SshHome -and (Test-Path -LiteralPath (Join-Path $SshHome 'uninstall-sshd.ps1'))) {
            & (Join-Path $SshHome 'uninstall-sshd.ps1') *> (Join-Path $scratch 'openssh-uninstall.txt')
        }
    } catch { $Result.uninstall_error = $_.Exception.Message }
    Remove-Item -Recurse -Force -Path 'C:\pitboard-probe-openssh' -ErrorAction SilentlyContinue
    if (-not $Result.config_existed) {
        Remove-Item -Recurse -Force -Path (Join-Path $env:ProgramData 'ssh') -ErrorAction SilentlyContinue
    }
    $agent = $Result.ssh_agent_before
    if ($agent -and $agent.present -and -not (Get-Service -Name 'ssh-agent' -ErrorAction SilentlyContinue) -and $agent.path) {
        $startup = switch ($agent.start_mode) { 'Auto' { 'Automatic' } 'Manual' { 'Manual' } default { 'Disabled' } }
        try {
            New-Service -Name 'ssh-agent' -BinaryPathName $agent.path -DisplayName 'OpenSSH Authentication Agent' -StartupType $startup | Out-Null
        } catch { $Result.ssh_agent_restore_error = $_.Exception.Message }
        $Result.ssh_agent_restored = [bool] (Get-Service -Name 'ssh-agent' -ErrorAction SilentlyContinue)
    }
    $Result.sshd_gone = $null -eq (Get-Service -Name 'sshd' -ErrorAction SilentlyContinue)
}

# RF-M: a loopback ssh session as $User by key, where the image ships sshd or Install-OpenSsh
# put it there, in which the probe reads the logon, DPAPI, Credential Manager and the processes
# of $User's other logon. $User joins OpenSSH Users where that group gates logins. The service
# is put back as it was.
function Measure-OpenSsh($User, $Result) {
    $service = Get-Service -Name 'sshd' -ErrorAction SilentlyContinue
    $Result.sshd_present = [bool] $service
    if (-not $service) { return }
    # The sshd build this logon ran under, so the next run attributes a DPAPI difference to the
    # build rather than the image.
    $sshdPath = (Get-CimInstance -ClassName Win32_Service -Filter "Name='sshd'" -ErrorAction SilentlyContinue).PathName
    if ($sshdPath) { $sshdPath = ($sshdPath -replace '^"([^"]+)".*$', '$1').Trim('"') }
    $Result.sshd = [ordered]@{
        path = $sshdPath
        product_version = if ($sshdPath -and (Test-Path -LiteralPath $sshdPath)) {
            (Get-Item -LiteralPath $sshdPath).VersionInfo.ProductVersion
        } else { $null }
    }
    $before = [ordered]@{ status = [string] $service.Status; start_type = [string] $service.StartType }
    $Result.before = $before
    $keyDir = Join-Path $scratch 'pitboard-probe-ssh'
    $sshDir = Join-Path $User.ProfileDir '.ssh'
    $holder = $null
    try {
        if ($before.start_type -eq 'Disabled') { Set-Service -Name 'sshd' -StartupType Manual }
        $sshUsers = Get-LocalGroup -SID $openSshUsersSid -ErrorAction SilentlyContinue
        $Result.openssh_users_group = [bool] $sshUsers
        if ($sshUsers) { Add-LocalGroupMember -SID $openSshUsersSid -Member $User.Name }
        New-Item -ItemType Directory -Force -Path $keyDir | Out-Null
        $key = Join-Path $keyDir 'pitboard-probe-m-key'
        $keygen = Start-Process -FilePath 'ssh-keygen.exe' -ArgumentList (Format-Arguments @('-q', '-t', 'ed25519', '-N', '', '-C', 'pitboard-probe', '-f', $key)) `
            -NoNewWindow -Wait -PassThru -RedirectStandardOutput (Join-Path $scratch 'ssh-keygen-out.txt') -RedirectStandardError (Join-Path $scratch 'ssh-keygen-err.txt')
        $Result.key_made = ($keygen.ExitCode -eq 0) -and (Test-Path -LiteralPath "$key.pub")
        icacls $key /inheritance:r /grant:r "*${jobSid}:F" *> $null
        New-Item -ItemType Directory -Force -Path $sshDir | Out-Null
        $authorized = Join-Path $sshDir 'authorized_keys'
        Copy-Item -Path "$key.pub" -Destination $authorized
        foreach ($path in $sshDir, $authorized) {
            icacls $path /inheritance:r /grant:r "*$($User.Sid):F" "*S-1-5-18:F" "*${administratorsSid}:F" *> $null
        }
        if ($before.status -ne 'Running') { Start-Service -Name 'sshd' }
        $Result.sshd_status = [string] (Get-Service -Name 'sshd').Status

        # A codex.exe of $User's in its other logon, for processes to find from the session.
        $holderExe = Join-Path $User.RunDir 'm\codex.exe'
        New-Item -ItemType Directory -Force -Path (Split-Path $holderExe) | Out-Null
        Copy-Item -Path $Probe -Destination $holderExe
        $holder = Start-AsUser $User 'm-holder' @('lock', '--scratch', (Join-Path $User.Scratch 'm-holder'), '--mode', 'hold', '--seconds', '120') -FilePath $holderExe

        $byKey = @('-i', $key, '-o', 'BatchMode=yes', '-o', 'PreferredAuthentications=publickey', '-o', 'StrictHostKeyChecking=no',
            '-o', "UserKnownHostsFile=$keyDir\known_hosts", '-o', 'ConnectTimeout=20', "$($User.Name)@localhost")
        $rows = [ordered]@{
            logon = 'logon'
            dpapi = "dpapi --scratch $($User.Scratch)\m"
            credman_write = 'credman-item --action write --name pitboard-probe-m'
            credman_read = 'credman-item --action read --name pitboard-probe-m'
            credman_delete = 'credman-item --action delete --name pitboard-probe-m'
            credman_names = 'credman-names'
            processes = 'processes --find codex.exe'
        }
        $session = [ordered]@{}
        foreach ($row in $rows.Keys) {
            $session[$row] = Invoke-Ssh $User "key-$row" $byKey "$Probe $($rows[$row])"
            Save-Report
        }
        $Result.key = $session
        if ($session.logon.exit -ne 0) { $Result.diagnosis = Get-SshdDiagnosis $User }
    } finally {
        if ($holder -and $holder.Process -and -not $holder.Process.HasExited) { taskkill /PID $holder.Process.Id /T /F *> $null }
        Invoke-AsUser $User 'm-cleanup' @('credman-item', '--action', 'delete', '--name', 'pitboard-probe-m') | Out-Null
        if ($before.status -ne 'Running') { Stop-Service -Name 'sshd' -Force -ErrorAction SilentlyContinue }
        Set-Service -Name 'sshd' -StartupType $before.start_type -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force -Path $keyDir, $sshDir -ErrorAction SilentlyContinue
        $after = Get-Service -Name 'sshd' -ErrorAction SilentlyContinue
        $Result.restored_exactly = ([string] $after.Status -eq $before.status) -and ([string] $after.StartType -eq $before.start_type)
    }
}

# The fresh standard user's rows, each saved as it lands.
function Measure-FreshUser($User, $Result, [string] $Bun, [string] $X64) {
    $s = $User.Scratch
    $Result.tokens = Invoke-AsUser $User 'tokens' @('tokens')
    $Result.homes = Invoke-AsUser $User 'homes' @('homes')
    Save-Report
    Measure-Symlinks $User $Result
    Save-Report

    Invoke-Row $Result 'rf_b1' {
        $fakeProfile = Join-Path $User.RunDir 'b1-profile'
        $fakeHome = Join-Path $User.RunDir 'b1-home'
        New-Item -ItemType Directory -Force -Path $fakeProfile, $fakeHome | Out-Null
        $bash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
        $r = [ordered]@{
            userprofile_overridden = Invoke-AsUser $User 'b1-userprofile' @('homes') -Extra @{ USERPROFILE = $fakeProfile }
            home_set = Invoke-AsUser $User 'b1-home' @('homes') -Extra @{ HOME = $fakeHome }
            git_bash_present = Test-Path -LiteralPath $bash
        }
        if ($r.git_bash_present) {
            $r.from_git_bash = Invoke-AsUser $User 'b1-bash' @('-c', "'$($Probe -replace '\\', '/')' homes") -FilePath $bash
        }
        $r
    }
    Invoke-Row $Result 'rf_b1e' {
        $script = Join-Path $toolHelpers 'os-homedir.mjs'
        $fakeProfile = Join-Path $User.RunDir 'b1-profile'
        $node = (Get-Command -Name 'node' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1).Source
        $r = [ordered]@{ node_present = [bool] $node }
        $scriptArgs = @($script, $User.ProfileDir)
        if ($node) {
            $r.node = Invoke-AsUser $User 'b1e-node' $scriptArgs -FilePath $node
            $r.node_userprofile_overridden = Invoke-AsUser $User 'b1e-node-up' $scriptArgs -FilePath $node -Extra @{ USERPROFILE = $fakeProfile }
        }
        if ($Bun) {
            $r.bun = Invoke-AsUser $User 'b1e-bun' $scriptArgs -FilePath $Bun
            $r.bun_userprofile_overridden = Invoke-AsUser $User 'b1e-bun-up' $scriptArgs -FilePath $Bun -Extra @{ USERPROFILE = $fakeProfile }
        }
        $r
    }
    # The scratch folder is made by the user's own probe here, so later rows read its ACL.
    Invoke-Row $Result 'rf_c3' { Invoke-AsUser $User 'c3-volume' @('volume', '--scratch', $s) }
    Invoke-Row $Result 'rf_c1' { Invoke-AsUser $User 'c1-acl' @('acl', '--scratch', $s) }
    Invoke-Row $Result 'rf_c1b' {
        $dir = Join-Path $s 'pitboard-probe-bunmode'
        $r = [ordered]@{}
        if ($Bun) { $r.bun = Invoke-AsUser $User 'c1b-bun' @((Join-Path $toolHelpers 'bun-mode.mjs'), $s) -FilePath $Bun }
        $r.file = Invoke-AsUser $User 'c1b-file' @('acl', '--scratch', $s, '--describe', (Join-Path $dir 'pitboard-probe-bunmode.json'))
        $r.folder = Invoke-AsUser $User 'c1b-folder' @('acl', '--scratch', $s, '--describe', $dir)
        $r
    }
    Invoke-Row $Result 'rf_c2' {
        $protected = Join-Path $s 'pitboard-probe-protected'
        [ordered]@{
            create = Invoke-AsUser $User 'c2-create' @('acl', '--scratch', $s, '--create', '--keep')
            open_as_the_user = Invoke-AsUser $User 'c2-open' @('acl', '--scratch', $s, '--open', $protected)
            open_as_the_job = Invoke-Probe 'c2-open-job' @('acl', '--scratch', $s, '--open', $protected)
            remove = Invoke-AsUser $User 'c2-remove' @('acl', '--scratch', $s, '--remove', $protected)
        }
    }
    Invoke-Row $Result 'rf_c4' { Measure-ReplaceLoop $User (Join-Path $s 'c4') 'c4' 10000 180 $Bun }
    Invoke-Row $Result 'rf_c5' {
        $r = [ordered]@{ flush_dir = Invoke-AsUser $User 'c5-flush' @('flush-dir', '--scratch', $s) }
        if ($Bun) { $r.lock = Measure-Lock $User (Join-Path $s 'c5') 'write' $Bun }
        $r
    }
    if ($Bun) { Invoke-Row $Result 'rf_c5r' { Measure-Lock $User (Join-Path $s 'c5r') 'refresh' $Bun } }
    Invoke-Row $Result 'rf_c6' { Measure-LockFileEx $User (Join-Path $s 'c6') 'c6' }
    Invoke-Row $Result 'rf_d1' { Invoke-AsUser $User 'd1' @('dpapi', '--scratch', $s) }
    Invoke-Row $Result 'rf_e1' {
        [ordered]@{
            pitboard_folder = Measure-Task $User $s 'e1' 'pitboard' 'logon' -SidSuffix
            root_folder = Measure-Task $User $s 'e1' 'root' 'logon' -SidSuffix
        }
    }
    Invoke-Row $Result 'rf_e6' { Measure-WatchService $User }
    Invoke-Row $Result 'rf_d2' {
        $out = Join-Path $s 'pitboard-probe-d2-unseal.json'
        [ordered]@{
            seal = Invoke-AsUser $User 'd2-seal' @('dpapi', '--scratch', $s, '--action', 'seal', '--file', 'pitboard-probe-d2.bin')
            task = Measure-Task $User $s 'd2' 'probe' "--out $out dpapi --scratch $s --action unseal --file pitboard-probe-d2.bin" -WaitSeconds 30 -Expect $out
        }
    }
    # RF-D2b: a restricted token like Codex's unelevated sandbox against a sealed park and a
    # protected file. The restricted-token block seals and makes both as this user, then runs
    # dpapi unseal and acl open from the restricted token.
    Invoke-Row $Result 'rf_d2b' {
        Invoke-AsUser $User 'd2b' @('restricted-token', '--scratch', (Join-Path $s 'd2b'))
    }
    Invoke-Row $Result 'rf_e5' {
        $e5 = Join-Path $User.RunDir 'e5'
        $app = Join-Path $e5 'app'
        New-Item -ItemType Directory -Force -Path $app | Out-Null
        $exe = Join-Path $app 'pitboard-probe.exe'
        Copy-Item -Path $Probe -Destination $exe
        $symlink = Join-Path $e5 'pitboard-probe-symlink.exe'
        $hardLink = Join-Path $e5 'pitboard-probe-hardlink.exe'
        $junction = Join-Path $e5 'current'
        New-Item -ItemType SymbolicLink -Path $symlink -Target $exe | Out-Null
        New-Item -ItemType HardLink -Path $hardLink -Target $exe | Out-Null
        New-Item -ItemType Junction -Path $junction -Target $app | Out-Null
        [ordered]@{
            direct = Invoke-AsUser $User 'e5-direct' @('exe-path') -FilePath $exe
            file_symlink = Invoke-AsUser $User 'e5-symlink' @('exe-path') -FilePath $symlink
            junction = Invoke-AsUser $User 'e5-junction' @('exe-path') -FilePath (Join-Path $junction 'pitboard-probe.exe')
            hard_link = Invoke-AsUser $User 'e5-hardlink' @('exe-path') -FilePath $hardLink
        }
    }
    Invoke-Row $Result 'rf_f1' {
        $r = [ordered]@{}
        foreach ($start in 'running', 'suspended') {
            foreach ($run in @(
                    @{ key = 'exe'; chain = 'exe'; rounds = 1000; more = @() },
                    @{ key = 'cmd-node-exe'; chain = 'cmd-node-exe'; rounds = 1000; more = @() },
                    @{ key = 'exe-breakaway'; chain = 'exe-breakaway'; rounds = 200; more = @() },
                    @{ key = 'exe-breakaway-ok'; chain = 'exe-breakaway'; rounds = 200; more = @('--allow-breakaway') })) {
                $r["$($run.key)_$start"] = Invoke-AsUser $User "f1-$($run.key)-$start" (@('job', '--scratch', (Join-Path $s 'f1'),
                    '--chain', $run.chain, '--rounds', [string] $run.rounds, '--start', $start) + $run.more) -TimeoutSeconds 600
                Save-Report
            }
        }
        $r
    }
    Invoke-Row $Result 'rf_f3' { Measure-BrowserFolder $User (Join-Path $s 'f3') }
    Invoke-Row $Result 'rf_h1' {
        $h1 = Join-Path $User.RunDir 'h1'
        $copies = [ordered]@{ lower = (Join-Path $h1 'a\codex.exe'); mixed = (Join-Path $h1 'b\Codex.EXE') }
        if ($X64) { $copies.x64 = (Join-Path $h1 'x\codex.exe') }
        foreach ($k in $copies.Keys) {
            New-Item -ItemType Directory -Force -Path (Split-Path $copies[$k]) | Out-Null
            Copy-Item -Path $(if ($k -eq 'x64') { $X64 } else { $Probe }) -Destination $copies[$k]
        }
        # winget's nested portable behind Links\codex.exe: a file symlink codex.exe ->
        # codex-<triple>.exe, started through the link, so processes names the image Toolhelp
        # reports for it.
        $linkTargetName = "codex-$Triple.exe"
        $linkTarget = Join-Path $h1 "link\$linkTargetName"
        $link = Join-Path $h1 'link\codex.exe'
        New-Item -ItemType Directory -Force -Path (Split-Path $linkTarget) | Out-Null
        Copy-Item -Path $Probe -Destination $linkTarget
        $linkMade = $false
        try {
            New-Item -ItemType SymbolicLink -Path $link -Target $linkTarget | Out-Null
            $linkMade = $true
        } catch { $null = $_ }
        $runs = [ordered]@{}
        foreach ($k in $copies.Keys) {
            $runs[$k] = Start-AsUser $User "h1-$k" @('lock', '--scratch', (Join-Path $s "h1-$k"), '--mode', 'hold', '--seconds', '40') -FilePath $copies[$k]
        }
        if ($linkMade) {
            $runs.link = Start-AsUser $User 'h1-link' @('lock', '--scratch', (Join-Path $s 'h1-link'), '--mode', 'hold', '--seconds', '40') -FilePath $link
        }
        $jobCopy = Join-Path $scratch 'h1\codex.exe'
        New-Item -ItemType Directory -Force -Path (Split-Path $jobCopy) | Out-Null
        Copy-Item -Path $Probe -Destination $jobCopy
        $jobRun = Start-Process -FilePath $jobCopy -ArgumentList (Format-Arguments @('lock', '--scratch', (Join-Path $scratch 'h1-job'), '--mode', 'hold', '--seconds', '40')) `
            -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $scratch 'h1-job-out.txt') -RedirectStandardError (Join-Path $scratch 'h1-job-err.txt')
        Start-Sleep -Seconds 8
        # Find codex.exe and the triple name, so the report shows which Toolhelp gives the link.
        $find = @('processes', '--find', 'codex.exe', '--find', $linkTargetName)
        $r = [ordered]@{
            x64_copy = [bool] $X64
            link_made = $linkMade
            link_target = $linkTargetName
            as_the_user = Invoke-AsUser $User 'h1-find' $find
            as_the_job = Invoke-Probe 'h1-find-job' $find
        }
        foreach ($k in $runs.Keys) { $r["holder_$k"] = Wait-Run $runs[$k] 90 }
        $null = $jobRun.WaitForExit(90000)
        $r
    }
    # RF-T8: FILE_ID_INFO, reserved names, long paths with and without pitboard.exe's manifest,
    # and GetProcessTimes, on NTFS as this user.
    Invoke-Row $Result 'rf_t8_file_id' { Invoke-AsUser $User 't8-fid' @('file-id', '--scratch', $s) }
    Invoke-Row $Result 'rf_t8_reserved_names' { Invoke-AsUser $User 't8-names' @('reserved-names', '--scratch', $s) }
    Invoke-Row $Result 'rf_t8_long_path' {
        $manifested = Join-Path (Split-Path -Parent $Probe) 'pitboard-probe-longpath.exe'
        [ordered]@{
            plain = Invoke-AsUser $User 't8-long-plain' @('long-path', '--scratch', $s)
            pitboard_manifest = if (Test-Path -LiteralPath $manifested) {
                Invoke-AsUser $User 't8-long-manifest' @('long-path', '--scratch', $s) -FilePath $manifested
            } else { 'not built' }
        }
    }
    Invoke-Row $Result 'rf_t8_process_times' { Invoke-AsUser $User 't8-times' @('process-times', '--children', '50') }
    if ($Bun) {
        Invoke-Row $Result 'rf_g3' {
            $secrets = Join-Path $toolHelpers 'bun-secrets.mjs'
            $r = [ordered]@{}
            try {
                $r.set = Invoke-AsUser $User 'g3-set' @($secrets, 'set') -FilePath $Bun
                $i = 0
                foreach ($name in 'claude-code-user', 'claude-code-user#0', 'claude-code-user#m') {
                    $target = "pitboard-citest-bun/$name"
                    $r[$name] = [ordered]@{
                        read = Invoke-AsUser $User "g3-read-$i" @('credman-item', '--action', 'read', '--name', $target)
                        delete = Invoke-AsUser $User "g3-delete-$i" @('credman-item', '--action', 'delete', '--name', $target)
                    }
                    $i++
                }
            } finally {
                $r.cleanup = Invoke-AsUser $User 'g3-cleanup' @($secrets, 'delete') -FilePath $Bun
                $r.leak_check = Invoke-AsUser $User 'g3-leak-check' @('credman-names', '--leak-check')
            }
            $r
        }
        # RF-WW1: a dummy handed on descriptor 3 through the C runtime's inheritance table, read
        # back by Bun the way Claude Code reads its token descriptor, as a pipe and as a socket.
        Invoke-Row $Result 'rf_ww1' {
            $fd3 = Join-Path $toolHelpers 'fd3-read.mjs'
            [ordered]@{
                pipe = Invoke-AsUser $User 'ww1-pipe' @('fd-handover', '--bun', $Bun, '--script', $fd3, '--kind', 'pipe')
                socket = Invoke-AsUser $User 'ww1-socket' @('fd-handover', '--bun', $Bun, '--script', $fd3, '--kind', 'socket')
            }
        }
    }
}

# A tool of the Windows SDK on this image, for this machine's architecture where there is one.
function Find-SdkTool([string] $Name) {
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
    $all = @(Get-ChildItem -Path $kits -Filter $Name -Recurse -ErrorAction SilentlyContinue | Sort-Object FullName -Descending)
    $native = $all | Where-Object { $_.Directory.Name -eq $arch } | Select-Object -First 1
    $found = if ($native) { $native } else { $all | Select-Object -First 1 }
    if (-not $found) { throw "$Name is not under $kits" }
    $found
}

# CF8: the probe's sparse package as $User, with each manifest in sparse/, then PitboardShare's
# from -App. A throwaway certificate for CN=PitboardProbe is made into a PFX here and trusted in
# LocalMachine\TrustedPeople from the job user's elevated side: Add-AppxPackage refused it from
# the user's own TrustedPeople with 0x800B0109 in run 38882775456. Each package is offered for
# the user with its program's folder as its external location, and the pitboard-probe-cf8 link
# scheme is put under the user's HKCU\Software\Classes, its handler the plain probe. The identity
# process writes, starts its children, opens the link through LaunchUriAsync, as
# PitboardShare.exe once handed a page on, and starts the plain probe through CreateProcessW
# with a link as its only argument, as PitboardShare.exe starts Pitboard.exe; a process
# without identity reads back what each wrote.
# PitboardShare.exe --register then adds PitboardShare's package, as the MSI does, and
# Pitboard.exe --unregister-share removes it, as the MSI's removal does. The user's part runs in
# its own Windows PowerShell, whose Appx cmdlets PowerShell 7 cannot load, and each change is
# undone.
function Measure-SparseIdentity($User, $Result) {
    $s = $User.Scratch
    $sparse = Join-Path $PSScriptRoot '..\..\crates\pitboard-probe\sparse'
    $location = Join-Path $User.RunDir 'cf8'
    $installed = Join-Path $User.RunDir 'cf8-pitboard'
    $work = Join-Path $env:RUNNER_TEMP 'pitboard-probe-cf8'
    $userScript = Join-Path $User.RunDir 'pitboard-probe-cf8.ps1'
    $cer = Join-Path $User.RunDir 'pitboard-probe-cf8.cer'
    $pfx = Join-Path $work 'pitboard-probe-cf8.pfx'
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $handler = Join-Path $location 'pitboard-probe.exe'
    $identityProbe = Join-Path $location 'pitboard-probe-sparse.exe'
    # %1 is the link, taken as an unused --scratch, so Windows appends nothing the probe refuses.
    $command = "$handler --out $(Join-Path $s 'pitboard-probe-cf8-launched.json') sparse --action write --tag launched --scratch `"%1`""
    $thumbprint = $null
    New-Item -ItemType Directory -Force -Path $location, $work | Out-Null
    try {
        foreach ($name in 'pitboard-probe.exe', 'pitboard-probe-sparse.exe') {
            Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $Probe) $name) -Destination $location
        }
        $makeappx = Find-SdkTool 'makeappx.exe'
        $signtool = Find-SdkTool 'signtool.exe'
        $Result.sdk_tools = "$($makeappx.Directory.Parent.Name)\$($makeappx.Directory.Name)"
        $password = Export-TestCertificate $cf8Publisher $pfx $cer
        $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($cer)
        $thumbprint = $certificate.Thumbprint
        $machine = [Security.Cryptography.X509Certificates.X509Store]::new('TrustedPeople', 'LocalMachine')
        try {
            $machine.Open('ReadWrite')
            $machine.Add($certificate)
        } finally {
            $machine.Close()
        }
        $Result.trusted_in = 'LocalMachine\TrustedPeople'
        Save-Report
        Set-Content -Path $userScript -Encoding ascii -Value @'
$ErrorActionPreference = 'Stop'
$key = 'HKCU:\Software\Classes\pitboard-probe-cf8'
function Hide([string] $Text) {
    $Text.Replace($env:USERPROFILE, '<profile>').Replace($env:USERNAME, '<user>') -replace 'S-1-5-21-[0-9-]+', '<sid>'
}
# The external location as Pitboard.exe --unregister-share reads it, through PackageManager.
function Get-Share {
    $package = Get-AppxPackage -Name 'PitboardShare'
    if (-not $package) { return 'PitboardShare: none' }
    $at = try {
        $null = [Windows.Management.Deployment.PackageManager, Windows.Management.Deployment, ContentType = WindowsRuntime]
        $external = (New-Object Windows.Management.Deployment.PackageManager).FindPackageForUser('', $package.PackageFullName).EffectiveExternalPath
        if (-not $external) { 'none' }
        elseif ($external.TrimEnd('\') -ieq $env:PITBOARD_PROBE_CF8_INSTALLED.TrimEnd('\')) { 'the install' }
        else { 'another folder' }
    } catch {
        "unread: $(Hide $_.Exception.Message)"
    }
    "PitboardShare: $($package.Status), signature $($package.SignatureKind), external location $at"
}
try {
    switch ($env:PITBOARD_PROBE_CF8_STEP) {
        'register' {
            New-Item -Path "$key\shell\open\command" -Force | Out-Null
            Set-ItemProperty -LiteralPath $key -Name '(default)' -Value 'URL:pitboard-probe-cf8'
            Set-ItemProperty -LiteralPath $key -Name 'URL Protocol' -Value ''
            Set-ItemProperty -LiteralPath "$key\shell\open\command" -Name '(default)' -Value $env:PITBOARD_PROBE_CF8_COMMAND
            Add-AppxPackage -Path $env:PITBOARD_PROBE_CF8_PACKAGE -ExternalLocation $env:PITBOARD_PROBE_CF8_LOCATION
            $package = Get-AppxPackage -Name 'PitboardProbe'
            "registered: $($package.Status), signature $($package.SignatureKind)"
        }
        'unregister' {
            Get-AppxPackage -Name 'PitboardProbe' | Remove-AppxPackage
            if (Test-Path -LiteralPath $key) { Remove-Item -LiteralPath $key -Recurse -Force }
            $left = @()
            if (Get-AppxPackage -Name 'PitboardProbe') { $left += 'package' }
            if (Test-Path -LiteralPath $key) { $left += 'scheme' }
            "unregistered, left: $($left -join ', ')"
        }
        'share' { Get-Share }
        'share-clean' {
            Get-AppxPackage -Name 'PitboardShare' | Remove-AppxPackage
            Get-Share
        }
    }
} catch {
    'failed 0x{0:X8}: {1}' -f $_.Exception.HResult, (Hide $_.Exception.Message)
}
'@
        $userArgs = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $userScript)
        foreach ($kind in 'virtualized', 'unvirtualized') {
            Invoke-Row $Result "cf8_$kind" {
                $package = Join-Path $work "pitboard-probe-$kind.msix"
                $folder = Join-Path $work "pkg-$kind"
                New-Item -ItemType Directory -Force -Path (Join-Path $folder 'Assets') | Out-Null
                $arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
                $xml = (Get-Content -Raw -LiteralPath (Join-Path $sparse "AppxManifest-$kind.xml")) -replace 'ProcessorArchitecture="arm64"', "ProcessorArchitecture=`"$arch`""
                Set-Content -LiteralPath (Join-Path $folder 'AppxManifest.xml') -Value $xml -Encoding utf8
                $png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=='
                [IO.File]::WriteAllBytes((Join-Path $folder 'Assets\probe.png'), [Convert]::FromBase64String($png))
                $r = [ordered]@{}
                $r.pack = ((& $makeappx.FullName pack /d $folder /p $package /nv /o 2>&1) | Select-Object -Last 1) -replace [regex]::Escape($work), '<work>'
                $r.pack_exit = $LASTEXITCODE
                $r.sign = ((& $signtool.FullName sign /fd SHA256 /f $pfx /p $password $package 2>&1) | Select-Object -Last 1) -replace [regex]::Escape($work), '<work>'
                $r.sign_exit = $LASTEXITCODE
                if ($r.pack_exit -ne 0 -or $r.sign_exit -ne 0) { return $r }
                $extra = @{
                    PITBOARD_PROBE_CF8_PACKAGE = $package
                    PITBOARD_PROBE_CF8_LOCATION = $location
                    PITBOARD_PROBE_CF8_COMMAND = $command
                }
                try {
                    $r.register = Invoke-AsUser $User "cf8-$kind-register" $userArgs -FilePath $windowsPowerShell -Text -Extra ($extra + @{ PITBOARD_PROBE_CF8_STEP = 'register' })
                    $r.identity = Invoke-AsUser $User "cf8-$kind-identity" @('sparse', '--action', 'identity') -FilePath $identityProbe
                    $r.write = Invoke-AsUser $User "cf8-$kind-write" @('sparse', '--action', 'write', '--tag', 'identity-parent') -FilePath $identityProbe
                    $r.children = Invoke-AsUser $User "cf8-$kind-children" @('sparse', '--action', 'children', '--scratch', $s) -FilePath $identityProbe
                    $r.launch = Invoke-AsUser $User "cf8-$kind-launch" @('sparse', '--action', 'launch', '--scratch', $s) -FilePath $identityProbe -TimeoutSeconds 120
                    $r.handoff = Invoke-AsUser $User "cf8-$kind-handoff" @('sparse', '--action', 'handoff', '--scratch', $s) -FilePath $identityProbe -TimeoutSeconds 120
                    $r.read_without_identity = Invoke-AsUser $User "cf8-$kind-read" @('sparse', '--action', 'read') -FilePath $handler
                } finally {
                    $r.clean = Invoke-AsUser $User "cf8-$kind-clean" @('sparse', '--action', 'clean') -FilePath $handler
                    $r.unregister = Invoke-AsUser $User "cf8-$kind-unregister" $userArgs -FilePath $windowsPowerShell -Text -Extra ($extra + @{ PITBOARD_PROBE_CF8_STEP = 'unregister' })
                    Remove-Item -Recurse -Force -LiteralPath $folder, $package -ErrorAction SilentlyContinue
                }
                $r
            }
        }
        if ($App) {
            Invoke-Row $Result 'cf8_unregister_share' {
                $r = [ordered]@{}
                Copy-Item -LiteralPath $App -Destination $installed -Recurse
                $package = Join-Path $installed 'PitboardShare.msix'
                $r.sign = ((& $signtool.FullName sign /fd SHA256 /f $pfx /p $password $package 2>&1) | Select-Object -Last 1) -replace [regex]::Escape($installed), '<install>'
                $r.sign_exit = $LASTEXITCODE
                if ($r.sign_exit -ne 0) { return $r }
                $extra = @{ PITBOARD_PROBE_CF8_INSTALLED = $installed }
                try {
                    $r.register = Invoke-AsUser $User 'cf8-share-register' @('--register') -FilePath (Join-Path $installed 'PitboardShare.exe') -Text
                    $r.registered = Invoke-AsUser $User 'cf8-share-registered' $userArgs -FilePath $windowsPowerShell -Text -Extra ($extra + @{ PITBOARD_PROBE_CF8_STEP = 'share' })
                    $timer = [Diagnostics.Stopwatch]::StartNew()
                    $r.unregister_share = Invoke-AsUser $User 'cf8-unregister-share' @('--unregister-share') -FilePath (Join-Path $installed 'Pitboard.exe') -Text
                    $r.unregister_share_ms = $timer.ElapsedMilliseconds
                    $r.after = Invoke-AsUser $User 'cf8-share-after' $userArgs -FilePath $windowsPowerShell -Text -Extra ($extra + @{ PITBOARD_PROBE_CF8_STEP = 'share' })
                } finally {
                    $r.clean = Invoke-AsUser $User 'cf8-share-clean' $userArgs -FilePath $windowsPowerShell -Text -Extra ($extra + @{ PITBOARD_PROBE_CF8_STEP = 'share-clean' })
                }
                $r
            }
        }
    } finally {
        if ($thumbprint) {
            $machine = [Security.Cryptography.X509Certificates.X509Store]::new('TrustedPeople', 'LocalMachine')
            try {
                $machine.Open('ReadWrite')
                $machine.Certificates | Where-Object { $_.Thumbprint -eq $thumbprint } | ForEach-Object { $machine.Remove($_) }
                $Result.certificate_removed = -not ($machine.Certificates | Where-Object { $_.Thumbprint -eq $thumbprint })
            } finally {
                $machine.Close()
            }
        }
        Remove-Item -Recurse -Force -LiteralPath $work, $location, $installed -ErrorAction SilentlyContinue
        Remove-Item -Force -LiteralPath $cer, $userScript -ErrorAction SilentlyContinue
        $Result.removed = -not (Test-Path -LiteralPath $work) -and -not (Test-Path -LiteralPath $location) -and
            -not (Test-Path -LiteralPath $installed) -and -not (Test-Path -LiteralPath $cer)
        Save-Report
    }
}

# RF-CM2, RF-D1b and RF-D1d, last, since they change the user's password: the leak check,
# then a seal, the user's own change through ADSI in a process of its own (both passwords in
# its environment, never on its command line) and an unseal with the new password, then
# another seal, an administrator's reset and an unseal of both.
function Measure-PasswordChanges($User, $Result) {
    $s = $User.Scratch
    Invoke-Row $Result 'rf_cm2' { Invoke-AsUser $User 'cm2' @('credman-names', '--leak-check') }
    Invoke-Row $Result 'rf_d1b' {
        $r = [ordered]@{ seal = Invoke-AsUser $User 'd1b-seal' @('dpapi', '--scratch', $s, '--action', 'seal', '--file', 'pitboard-probe-d1b.bin') }
        $changer = Join-Path $User.RunDir 'pitboard-probe-d1b.ps1'
        @(
            'try {',
            '    $u = [adsi]("WinNT://" + $env:COMPUTERNAME + "/" + $env:PITBOARD_PROBE_USER + ",user")',
            '    $u.ChangePassword($env:PITBOARD_PROBE_OLD, $env:PITBOARD_PROBE_NEW)',
            '    "changed"',
            '} catch {',
            '    "failed 0x{0:X8}" -f $_.Exception.HResult',
            '}'
        ) | Set-Content -Path $changer -Encoding ascii
        $new = New-Password
        $r.change = Invoke-AsUser $User 'd1b-change' @('-NoProfile', '-NonInteractive', '-File', $changer) -FilePath $pwsh -Text -Extra @{
            PITBOARD_PROBE_USER = $User.Name
            PITBOARD_PROBE_OLD = (Get-PlainText $User.Secure)
            PITBOARD_PROBE_NEW = (Get-PlainText $new)
        }
        if ($r.change.output -match 'changed') {
            $User.Secure = $new
            $User.Cred = [pscredential]::new($User.Name, $new)
        }
        $r.unseal_with_the_new_password = Invoke-AsUser $User 'd1b-unseal' @('dpapi', '--scratch', $s, '--action', 'unseal', '--file', 'pitboard-probe-d1b.bin')
        $r
    }
    Invoke-Row $Result 'rf_d1d' {
        $r = [ordered]@{ seal = Invoke-AsUser $User 'd1d-seal' @('dpapi', '--scratch', $s, '--action', 'seal', '--file', 'pitboard-probe-d1d.bin') }
        $reset = New-Password
        Set-LocalUser -SID $User.Sid -Password $reset
        $User.Secure = $reset
        $User.Cred = [pscredential]::new($User.Name, $reset)
        $User.PasswordReset = $true
        $r.reset = $true
        $r.unseal_d1b_after_reset = Invoke-AsUser $User 'd1d-unseal-d1b' @('dpapi', '--scratch', $s, '--action', 'unseal', '--file', 'pitboard-probe-d1b.bin')
        $r.unseal_d1d_after_reset = Invoke-AsUser $User 'd1d-unseal-d1d' @('dpapi', '--scratch', $s, '--action', 'unseal', '--file', 'pitboard-probe-d1d.bin')
        $r
    }
}

# RF-E1c: $Second registers its task in a \Pitboard\ $First made, then $First deletes its own
# and tries the folder with $Second's still in it, then $Second deletes its own and $First the
# folder.
function Measure-SharedTaskFolder($First, $Second) {
    $step = {
        param($User, [string] $Step, [string[]] $More)
        $all = @('task', '--scratch', (Join-Path $User.Scratch 'e1c'), '--label', 'e1c', '--folder', 'pitboard', '--sid-suffix', '--action', $Step) + $More
        Invoke-AsUser $User "task-e1c-$Step" $all
    }
    $r = [ordered]@{}
    $r.first_register = & $step $First 'register' @('--args', 'logon')
    $r.second_read = & $step $Second 'read' @()
    $r.second_register = & $step $Second 'register' @('--args', 'logon')
    $r.first_delete = & $step $First 'delete' @()
    $r.second_delete = & $step $Second 'delete' @()
    $r.first_delete_again = & $step $First 'delete' @()
    $r
}

# RF-B2: a second user whose name is not ASCII: pb, D with stroke, a with dot below, t. With
# $Fresh, the first fresh user still there, RF-E1c between the two.
function Measure-NonAsciiUser($Result, [string] $Bun, $Fresh) {
    $user = $null
    try {
        $name = 'pb' + [char] 0x0110 + [char] 0x1EA1 + 't' + (Get-Random -Minimum 10000 -Maximum 99999)
        $user = New-ThrowawayUser 'nonascii' $name @($usersSid) $Result
        if ($user.ProfileDir -and $Fresh) {
            Invoke-Row $Result 'rf_e1c' { Measure-SharedTaskFolder $Fresh $user }
        }
        if ($user.ProfileDir) {
            $s = $user.Scratch
            $codex = Join-Path $s 'codex'
            $cc = Join-Path $s 'cc'
            # Made by the user, so they are its own and have canonical spellings to hash.
            $Result.made_homes = Invoke-AsUser $user 'b2-mkdir' @('/d', '/c', 'mkdir', $codex, $cc) -FilePath (Join-Path $env:SystemRoot 'System32\cmd.exe') -Text
            $Result.homes = Invoke-AsUser $user 'b2-homes' @('homes', '--codex-home', $codex, '--claude-config-dir', $cc)
            $Result.homes_by_short_names = Invoke-Fact {
                $fso = New-Object -ComObject 'Scripting.FileSystemObject'
                Invoke-AsUser $user 'b2-homes-short' @('homes', '--codex-home', $fso.GetFolder($codex).ShortPath, '--claude-config-dir', $fso.GetFolder($cc).ShortPath)
            }
            if ($Bun) { $Result.bun_os_homedir = Invoke-AsUser $user 'b2-bun' @((Join-Path $toolHelpers 'os-homedir.mjs'), $user.ProfileDir) -FilePath $Bun }
        }
    } catch {
        $Result.error = $_.Exception.Message
    } finally {
        if ($user) { Remove-ThrowawayUser $user $Result }
        Save-Report
    }
}

# RF-T8: files the elevated job user makes in $User's scratch folder, owned by Administrators,
# each with a protected DACL, opened from $User's UAC-filtered token, in which Administrators
# is a deny-only SID, and described as that token reads them.
function Measure-FilteredToken($User) {
    $s = $User.Scratch
    # In run 39310064516, icacls /inheritance:r /grant:r left the user's ACE on the file meant
    # for administrators alone, so each descriptor is set whole.
    $files = [ordered]@{
        user_and_administrators = "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;$($User.Sid))"
        administrators_alone = 'O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)'
        user_read_alone = "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FR;;;$($User.Sid))"
    }
    $r = [ordered]@{}
    try {
        foreach ($kind in $files.Keys) {
            $path = Join-Path $s ('pitboard-probe-ace-' + ($kind -replace '_', '-'))
            Set-Content -LiteralPath $path -Value 'x'
            $security = [System.Security.AccessControl.FileSecurity]::new()
            $security.SetSecurityDescriptorSddlForm($files[$kind])
            $set = Invoke-Fact { Set-Acl -LiteralPath $path -AclObject $security; 'set' }
            $r[$kind] = [ordered]@{
                descriptor_set = $set
                described = Invoke-AsUser $User "t8-ace-$kind-describe" @('acl', '--scratch', $s, '--describe', $path)
                opened = Invoke-AsUser $User "t8-ace-$kind" @('acl', '--scratch', $s, '--open', $path)
            }
        }
    } finally {
        Get-ChildItem -LiteralPath $s -Filter 'pitboard-probe-ace-*' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
        $r.removed = -not (Get-ChildItem -LiteralPath $s -Filter 'pitboard-probe-ace-*' -ErrorAction SilentlyContinue)
    }
    $r
}

# RF-A1's other half: a fresh member of Administrators, started as the standard user is.
function Measure-FreshAdmin($Result) {
    $user = $null
    try {
        $user = New-ThrowawayUser 'admin' ('pbadmin' + (Get-Random -Minimum 10000 -Maximum 99999)) @($usersSid, $administratorsSid) $Result
        if ($user.ProfileDir) {
            $s = $user.Scratch
            $Result.tokens = Invoke-AsUser $user 'a1-tokens' @('tokens', '--scratch', $s)
            $Result.acl = Invoke-AsUser $user 'a1-acl' @('acl', '--scratch', $s)
            $Result.acl_create = Invoke-AsUser $user 'a1-create' @('acl', '--scratch', $s, '--create', '--keep')
            $Result.acl_remove = Invoke-AsUser $user 'a1-remove' @('acl', '--scratch', $s, '--remove', (Join-Path $s 'pitboard-probe-protected'))
            Save-Report
            $Result.rf_t8_filtered_token = Invoke-Fact { Measure-FilteredToken $user }
        }
    } catch {
        $Result.error = $_.Exception.Message
    } finally {
        if ($user) { Remove-ThrowawayUser $user $Result }
        Save-Report
    }
}

# RF-T8's program started by the job user's Explorer: `explorer.exe <file>` hands the file to
# the Explorer already running, which opens a .cmd with its own environment. The .cmd says
# whether it got this script's environment instead, and the variable as it found it.
function Start-FromExplorer([string] $Dir, [string] $Label, [string] $Name) {
    $cmd = Join-Path $Dir "pitboard-probe-sc-$Label.cmd"
    $out = Join-Path $Dir "pitboard-probe-sc-$Label.txt"
    @(
        '@echo off',
        '(',
        'if defined PITBOARD_PROBE_SC_PARENT (echo parent=this script) else (echo parent=explorer)',
        "if defined $Name (echo value=%$Name%) else (echo value=unset)",
        ") > `"$out.tmp`"",
        "move /y `"$out.tmp`" `"$out`" > nul"
    ) | Set-Content -LiteralPath $cmd -Encoding ascii
    Start-Process -FilePath (Join-Path $env:SystemRoot 'explorer.exe') -ArgumentList "`"$cmd`""
    $r = [ordered]@{ ran = Wait-File $out 30 }
    if ($r.ran) {
        foreach ($line in Get-Content -LiteralPath $out) {
            $pair = $line.Trim() -split '=', 2
            if ($pair.Count -eq 2) { $r[$pair[0]] = $pair[1] }
        }
    }
    $r
}

# RF-T8: a value written to the job user's HKCU\Environment with no broadcast, then the
# broadcast setx sends, as a probe running meanwhile sees each and as a program Explorer starts
# sees each. The value is put back as it was and broadcast again, so Explorer drops it too.
function Measure-SettingChange($Result) {
    $name = 'PITBOARD_PROBE_SETTINGCHANGE'
    $dir = Join-Path $scratch 'sc'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $read = {
        $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
        $state = if ($k -and $k.GetValueNames() -contains $name) {
            [ordered]@{ value = $k.GetValue($name, $null, 'DoNotExpandEnvironmentNames'); kind = [string] $k.GetValueKind($name) }
        } else { $null }
        if ($k) { $k.Close() }
        $state
    }
    $before = & $read
    $Result.present_before = $null -ne $before
    $watchOut = Join-Path $dir 'pitboard-probe-sc-watch.json'
    $watch = $null
    $env:PITBOARD_PROBE_SC_PARENT = 'this script'
    try {
        $watch = Start-Process -FilePath $Probe -ArgumentList (Format-Arguments @('--out', $watchOut, 'setting-change', '--name', $name, '--seconds', '60', '--scratch', $dir)) `
            -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $scratch 'sc-watch-out.txt') -RedirectStandardError (Join-Path $scratch 'sc-watch-err.txt')
        $null = $watch.Handle
        $Result.watch_ready = Wait-File (Join-Path $dir 'pitboard-probe-sc-ready') 30
        $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        $k.SetValue($name, 'set-by-the-row', [Microsoft.Win32.RegistryValueKind]::String)
        $k.Close()
        Start-Sleep -Seconds 5
        $Result.explorer_before_the_broadcast = Start-FromExplorer $dir 'before' $name
        $Result.broadcast = Invoke-Probe 'sc-broadcast' @('setting-change', '--action', 'broadcast', '--name', $name)
        Start-Sleep -Seconds 3
        $Result.explorer_after_the_broadcast = Start-FromExplorer $dir 'after' $name
        $waited = Wait-Bounded $watch 120
        $Result.watch = [ordered]@{ timed_out = $waited.TimedOut; exit = $waited.Exit; report = (Read-Json $watchOut) }
    } finally {
        Remove-Item -Path Env:PITBOARD_PROBE_SC_PARENT -ErrorAction SilentlyContinue
        if ($watch -and -not $watch.HasExited) { taskkill /PID $watch.Id /T /F *> $null }
        $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        if ($null -eq $before) {
            $k.DeleteValue($name, $false)
        } else {
            $k.SetValue($name, $before.value, [Microsoft.Win32.RegistryValueKind] $before.kind)
        }
        $k.Close()
        $Result.broadcast_after_restoring = Invoke-Probe 'sc-broadcast-restored' @('setting-change', '--action', 'broadcast', '--name', $name)
        $Result.restored_exactly = ((& $read) | ConvertTo-Json -Compress) -eq ($before | ConvertTo-Json -Compress)
        Remove-Item -Recurse -Force -LiteralPath $dir -ErrorAction SilentlyContinue
    }
}

$browserImages = 'msedge', 'chrome', 'firefox'

function Stop-Browsers {
    Stop-Process -Name ($browserImages + 'OpenWith') -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}

# RF-F3: whether the browser a sign-in opens keeps the folder the sign-in ran in, the page
# opened as Claude Code's sign-in and as Codex's open theirs, each with no browser running
# first, then by each other browser the image has, started as Codex starts the http
# association's program. The probe ends the browsers it saw start; any left are ended here.
function Measure-BrowserFolder($User, [string] $In) {
    $r = [ordered]@{}
    $runs = [ordered]@{ claude = @('--as', 'claude'); codex = @('--as', 'codex') }
    foreach ($name in 'chrome', 'firefox') {
        $key = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\$name.exe"
        $path = (Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue).'(default)'
        if ($path -and (Test-Path -LiteralPath $path.Trim('"'))) {
            $runs["codex_$name"] = @('--as', 'codex', '--browser', $path.Trim('"'))
        }
    }
    try {
        foreach ($run in $runs.Keys) {
            Stop-Browsers
            $probeArgs = @('browser-folder', '--scratch', (Join-Path $In $run)) + $runs[$run]
            $r[$run] = if ($User) {
                Invoke-AsUser $User "f3-$run" $probeArgs -TimeoutSeconds 300
            } else {
                Invoke-Probe "f3-$run" $probeArgs -TimeoutSeconds 300
            }
            $r[$run].browser_processes_left = @(Get-Process -Name $browserImages -ErrorAction SilentlyContinue).Count
            Save-Report
        }
    } finally {
        Stop-Browsers
        Remove-Item -Recurse -Force -LiteralPath $In -ErrorAction SilentlyContinue
    }
    $r
}

# The job user's rows: RF-E1's, RF-E6's, RF-E3, RF-D2's, RF-F2 and RF-F3.
function Measure-JobUser($Result) {
    $s = Join-Path $scratch 'job'
    Invoke-Row $Result 'rf_e1' {
        [ordered]@{
            pitboard_folder = Measure-Task $null $s 'e1j' 'pitboard' 'logon' -SidSuffix
            root_folder = Measure-Task $null $s 'e1j' 'root' 'logon' -SidSuffix
        }
    }
    Invoke-Row $Result 'rf_e6' { Measure-WatchTask (Join-Path $s 'e6') }
    # The three values are put back exactly as they were, value and kind, or removed where
    # they were absent.
    Invoke-Row $Result 'rf_e3' {
        $names = 'PITBOARD_HOME', 'CLAUDE_CONFIG_DIR', 'CODEX_HOME'
        $read = {
            $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
            $state = [ordered]@{}
            foreach ($n in $names) {
                $state[$n] = if ($k -and $k.GetValueNames() -contains $n) {
                    [ordered]@{ value = $k.GetValue($n, $null, 'DoNotExpandEnvironmentNames'); kind = [string] $k.GetValueKind($n) }
                } else { $null }
            }
            if ($k) { $k.Close() }
            $state
        }
        $before = & $read
        $r = [ordered]@{ present_before = @($names | Where-Object { $null -ne $before[$_] }) }
        try {
            $r.setx_exit = [ordered]@{}
            foreach ($n in $names) {
                setx $n (Join-Path $s ('e3-' + $n.ToLowerInvariant())) *> $null
                $r.setx_exit[$n] = $LASTEXITCODE
            }
            $out = Join-Path $s 'pitboard-probe-e3-homes.json'
            $r.task = Measure-Task $null $s 'e3' 'probe' "--out $out homes" -WaitSeconds 30 -Expect $out
        } finally {
            $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
            foreach ($n in $names) {
                if ($null -eq $before[$n]) {
                    $k.DeleteValue($n, $false)
                } else {
                    $k.SetValue($n, $before[$n].value, [Microsoft.Win32.RegistryValueKind] $before[$n].kind)
                }
            }
            $k.Close()
            $r.restored_exactly = ((& $read) | ConvertTo-Json -Compress) -eq ($before | ConvertTo-Json -Compress)
        }
        $r
    }
    Invoke-Row $Result 'rf_d2' {
        $out = Join-Path $s 'pitboard-probe-d2j-unseal.json'
        [ordered]@{
            seal = Invoke-Probe 'd2j-seal' @('dpapi', '--scratch', $s, '--action', 'seal', '--file', 'pitboard-probe-d2j.bin')
            task = Measure-Task $null $s 'd2j' 'probe' "--out $out dpapi --scratch $s --action unseal --file pitboard-probe-d2j.bin" -WaitSeconds 30 -Expect $out
        }
    }
    Invoke-Row $Result 'rf_t8_setting_change' {
        $r = [ordered]@{}
        try { Measure-SettingChange $r } catch { $r.error = $_.Exception.Message }
        $r
    }
    Invoke-Row $Result 'rf_f2' {
        $r = [ordered]@{}
        foreach ($open in 'shell', 'assoc') {
            foreach ($end in 'terminate-job', 'allowlist', 'parent-exit') {
                $r["$($open)_$end"] = [ordered]@{
                    run = Invoke-Probe "f2-$open-$end" @('job-browser', '--scratch', (Join-Path $s 'f2'), '--open', $open, '--end', $end) -TimeoutSeconds 120
                    edge_left = @(Get-Process -Name 'msedge' -ErrorAction SilentlyContinue).Count
                }
                Stop-Process -Name 'msedge' -Force -ErrorAction SilentlyContinue
                Start-Sleep -Seconds 2
                Save-Report
            }
        }
        $r
    }
    Invoke-Row $Result 'rf_f3' { Measure-BrowserFolder $null (Join-Path $s 'f3') }
}

# L2, L3, L4 and L8: the command line as Scoop, winget, a folder on the user PATH and
# ~\.cargo\bin put it, the gated build in the zips package-windows.ps1 makes, served from a
# loopback server. Each change is undone in the finally below.

# The 19 rules Microsoft's attack surface reduction reference lists, read on 15 October 2026.
$asrRules = @(
    '56a863a9-875e-4185-98a7-b882c64b5ce5', '7674ba52-37eb-4a4f-a9a1-f0f9a1619a2c',
    'd4f940ab-401b-4efc-aadc-ad5f3c50688a', '9e6c4e1f-7d60-472f-ba1a-a39ef669e4b2',
    'be9ba2d9-53ea-4cdc-84e5-9b1eeee46550', '01443614-cd74-433a-b99e-2ecdc07bfc25',
    '5beb7efe-fd9a-4556-801d-275e5ffc04cc', 'd3e037e1-3eb8-44c8-a917-57927947596d',
    '3b576869-a4ec-4529-8536-b80a7769e899', '75668c1f-73b5-4cf0-bb93-3ecf5cb7cc84',
    '26190899-1602-49e8-8b27-eb1d0a1ce869', 'e6db77e5-3df2-4cf1-b95a-636979351e5b',
    'd1e49aac-8f56-4280-b9ba-993a6d77406c', '33ddedf1-c6e0-47cb-833e-de6133960387',
    'b2b3f03d-6a65-4f7b-a9c7-1c7ef74a9ba4', 'c0033c00-d16d-4114-a5a0-dc9b3a7d2ceb',
    'a8f5898e-1dc8-49a9-9878-85004b8a61e6', '92e97fa1-2edf-4476-bdd6-9dd0b4dddc7b',
    'c1db55ab-c21a-4637-bb3f-a12568109d35'
)

# The rules Defender has, by lowercase ID, with each one's action: 0 off, 1 block, 2 audit, 6 warn.
function Get-AsrRules {
    $preference = Get-MpPreference
    $ids = @($preference.AttackSurfaceReductionRules_Ids)
    $actions = @($preference.AttackSurfaceReductionRules_Actions)
    $rules = [ordered]@{}
    for ($i = 0; $i -lt $ids.Count; $i++) {
        if ($ids[$i]) { $rules[$ids[$i].ToLowerInvariant()] = [int] $actions[$i] }
    }
    $rules
}

# The last lines a tool printed, with the job user's name and folders taken out.
function Get-Tail([string] $Text, [int] $Lines = 12) {
    if (-not $Text) { return $null }
    foreach ($pair in @(@($env:LOCALAPPDATA, '<job-local>'), @($env:USERPROFILE, '<job-profile>'), @($env:USERNAME, '<job-user>'))) {
        if ($pair[0]) { $Text = $Text -replace [regex]::Escape($pair[0]), $pair[1] }
    }
    $Text = (@($Text -split "\r?\n" | Where-Object { $_.Trim() }) | Select-Object -Last $Lines) -join "`n"
    if ($Text.Length -gt 1500) { $Text = $Text.Substring($Text.Length - 1500) }
    $Text
}

# A native program, or Scoop through its shim, which always exits with a code: the code and the
# last lines it printed. Errors do not stop Scoop here, as they do not in a terminal.
function Invoke-Tool([string] $FilePath, [string[]] $ArgumentList) {
    $ErrorActionPreference = 'Continue'
    $out = & $FilePath @ArgumentList *>&1 | Out-String
    [ordered]@{ exit = ('0x{0:X8}' -f $LASTEXITCODE); tail = Get-Tail $out }
}

# Every value of the job user's HKCU\Environment, unexpanded, with its kind.
function Get-UserEnvironment {
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    $state = [ordered]@{}
    foreach ($n in $k.GetValueNames()) {
        $state[$n] = [ordered]@{ value = $k.GetValue($n, $null, 'DoNotExpandEnvironmentNames'); kind = [string] $k.GetValueKind($n) }
    }
    $k.Close()
    $state
}

# Puts HKCU\Environment back exactly: values added since are deleted, the rest written as they
# were. True when it reads back the same.
function Restore-UserEnvironment($Before) {
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    foreach ($n in @($k.GetValueNames())) {
        if (-not $Before.Contains($n)) { $k.DeleteValue($n, $false) }
    }
    foreach ($n in $Before.Keys) {
        $k.SetValue($n, $Before[$n].value, [Microsoft.Win32.RegistryValueKind] $Before[$n].kind)
    }
    $k.Close()
    ((Get-UserEnvironment) | ConvertTo-Json -Compress) -eq ($Before | ConvertTo-Json -Compress)
}

function Add-UserPath([string] $Dir) {
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    $path = $k.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
    $k.SetValue('Path', ($path.TrimEnd(';') + ";$Dir").TrimStart(';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
    $k.Close()
}

# PATH as a program started after these installs gets it: the machine's Path, then the
# user's, each entry expanded.
function Get-NewPath {
    $machine = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment')
    $user = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    $entries = foreach ($pair in @(@('machine', $machine), @('user', $user))) {
        foreach ($e in ($pair[1].GetValue('Path', '', 'DoNotExpandEnvironmentNames') -split ';')) {
            if ($e.Trim()) { [pscustomobject]@{ From = $pair[0]; Dir = [Environment]::ExpandEnvironmentVariables($e.Trim()).TrimEnd('\') } }
        }
    }
    $machine.Close()
    $user.Close()
    @($entries)
}

function Get-PathLabel([string] $Dir, $Places) {
    foreach ($label in $Places.Keys) {
        if ($Dir.TrimEnd('\') -like $Places[$label]) { return $label }
    }
    'other'
}

# A pitboard.exe that keeps running from where it is: its PowerShell completions, about 29 KB,
# fill a pipe nobody reads.
function Start-Held([string] $Exe, $Homes) {
    $start = [Diagnostics.ProcessStartInfo]::new($Exe)
    $start.ArgumentList.Add('completions')
    $start.ArgumentList.Add('powershell')
    foreach ($variable in $Homes.Keys) { $start.Environment[$variable] = $Homes[$variable] }
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $process = [Diagnostics.Process]::Start($start)
    Start-Sleep -Seconds 2
    $process
}

function Stop-Held($Process) {
    if ($Process -and -not $Process.HasExited) {
        $Process.Kill()
        $null = $Process.WaitForExit(10000)
    }
}

# A winget manifest for Pitboard.CLI at $Version, in the three files komac writes, its zip
# served at $Url.
function Write-WingetManifest([string] $Folder, [string] $Version, [string] $Url, [string] $Sha256) {
    New-Item -ItemType Directory -Force -Path $Folder | Out-Null
    $arch = if ($Triple -eq 'aarch64-pc-windows-msvc') { 'arm64' } else { 'x64' }
    $common = "PackageIdentifier: Pitboard.CLI`nPackageVersion: $Version`n"
    Set-Content -LiteralPath (Join-Path $Folder 'Pitboard.CLI.yaml') -Encoding utf8 -Value (
        $common + "DefaultLocale: en-US`nManifestType: version`nManifestVersion: 1.10.0`n")
    Set-Content -LiteralPath (Join-Path $Folder 'Pitboard.CLI.locale.en-US.yaml') -Encoding utf8 -Value (
        $common + "PackageLocale: en-US`nPublisher: Pitboard`nPackageName: Pitboard`nLicense: Apache-2.0`n" +
        "ShortDescription: Switch Claude Code and Codex between your own accounts`nManifestType: defaultLocale`nManifestVersion: 1.10.0`n")
    Set-Content -LiteralPath (Join-Path $Folder 'Pitboard.CLI.installer.yaml') -Encoding utf8 -Value (
        $common + "InstallerType: zip`nNestedInstallerType: portable`nNestedInstallerFiles:`n" +
        "- RelativeFilePath: pitboard-v$Version-$Triple\pitboard.exe`n  PortableCommandAlias: pitboard`n" +
        "Installers:`n- Architecture: $arch`n  InstallerUrl: $Url`n  InstallerSha256: $Sha256`n" +
        "ManifestType: installer`nManifestVersion: 1.10.0`n")
}

# Serves -Folder's files on http://127.0.0.1:<port>/, as a release's downloads are served.
$loopbackServer = @'
param([int] $Port, [string] $Folder)
$listener = [Net.HttpListener]::new()
$listener.Prefixes.Add("http://127.0.0.1:$Port/")
$listener.Start()
while ($listener.IsListening) {
    $context = $listener.GetContext()
    $relative = [Uri]::UnescapeDataString($context.Request.Url.AbsolutePath.TrimStart('/')).Replace('/', '\')
    $file = [IO.Path]::GetFullPath((Join-Path $Folder $relative))
    if ($file.StartsWith($Folder + '\') -and (Test-Path -LiteralPath $file -PathType Leaf)) {
        $bytes = [IO.File]::ReadAllBytes($file)
        $context.Response.ContentType = 'application/zip'
        $context.Response.ContentLength64 = $bytes.Length
        $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
    } else {
        $context.Response.StatusCode = 404
    }
    $context.Response.Close()
}
'@

function Measure-Channels($Result) {
    if (-not $Pitboard) { throw 'no -Pitboard build was given' }
    $root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
    $version = $null
    $section = ''
    foreach ($line in Get-Content -LiteralPath (Join-Path $root 'Cargo.toml')) {
        if ($line -match '^\[(.+)\]\s*$') { $section = $Matches[1] }
        elseif ($section -eq 'workspace.package' -and $line -match '^version = "(.+)"') { $version = $Matches[1]; break }
    }
    $parts = $version -split '[.-]'
    $next = "$($parts[0]).$($parts[1]).$([int] $parts[2] + 1)"
    $Result.versions = @($version, $next)

    $channels = Join-Path $scratch 'channels'
    $serve = Join-Path $channels 'serve'
    $homes = @{}
    foreach ($variable in 'USERPROFILE', 'HOME', 'APPDATA', 'LOCALAPPDATA', 'CODEX_HOME', 'CLAUDE_CONFIG_DIR', 'PITBOARD_HOME') {
        $homes[$variable] = Join-Path $channels "homes\$variable"
        New-Item -ItemType Directory -Force -Path $homes[$variable] | Out-Null
    }
    $zips = [ordered]@{}
    foreach ($v in $version, $next) {
        $zip = & (Join-Path $PSScriptRoot 'package-windows.ps1') -Exe $Pitboard -Version $v -Triple $Triple -Out (Join-Path $serve "v$v")
        $zips[$v] = [ordered]@{ path = $zip; sha256 = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash }
    }
    $exeHash = (Get-FileHash -LiteralPath $Pitboard -Algorithm SHA256).Hash

    $scoopRoot = Join-Path $channels 'scoop'
    $scoop = Join-Path $scoopRoot 'shims\scoop.ps1'
    $msiBin = Join-Path $channels 'msi-bin'
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    $links = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links'
    $places = [ordered]@{
        scoop_shims = Join-Path $scoopRoot 'shims'
        winget_links = $links
        winget_package = (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages') + '\*'
        msi_bin = $msiBin
        cargo_bin = $cargoBin
    }
    $wingetPackages = { Get-ChildItem -Directory -Path (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages') -Filter 'Pitboard.CLI*' -ErrorAction SilentlyContinue }
    $wingetKeys = {
        Get-ChildItem -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
            Where-Object { $_.PSChildName -like 'Pitboard.CLI*' }
    }
    # What a winget install of Pitboard.CLI leaves: the link, the package's folder and its Uninstall key.
    $wingetLeft = {
        if (Get-Item -Force -LiteralPath (Join-Path $links 'pitboard.exe') -ErrorAction SilentlyContinue) { 'winget_link' }
        if (& $wingetPackages) { 'winget_package' }
        if (& $wingetKeys) { 'winget_uninstall_key' }
    }
    $environment = Get-UserEnvironment
    $scoopConfig = Join-Path $env:USERPROFILE '.config\scoop'
    $scoopConfigBefore = Test-Path -LiteralPath $scoopConfig
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    # What the rows changed, for the finally: a row's block runs in a scope of its own.
    $state = @{ CargoCopy = $false; LocalManifestsBefore = $null; WingetManifest = $null; Held = [Collections.Generic.List[object]]::new(); AsrBefore = $null }
    $server = $null
    try {
        # The release's URLs, on this machine: http://127.0.0.1:<port>/v<version>/<zip>.
        $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
        $listener.Start()
        $port = $listener.LocalEndpoint.Port
        $listener.Stop()
        $serverScript = Join-Path $channels 'loopback-server.ps1'
        Set-Content -LiteralPath $serverScript -Value $loopbackServer -Encoding utf8
        $server = Start-Process -FilePath $pwsh -ArgumentList (Format-Arguments @('-NoProfile', '-File', $serverScript, $port, $serve)) `
            -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $channels 'server-out.txt') -RedirectStandardError (Join-Path $channels 'server-err.txt')
        $base = "http://127.0.0.1:$port"
        $first = "$base/v$version/$(Split-Path -Leaf $zips[$version].path)"
        $up = $false
        foreach ($attempt in 1..20) {
            try { $null = Invoke-WebRequest -Uri $first -UseBasicParsing; $up = $true; break } catch { Start-Sleep -Seconds 1 }
        }
        if (-not $up) { throw "the loopback server did not answer on port $port" }

        Invoke-Row $Result 'l8_scoop' {
            $r = [ordered]@{}
            $installer = Join-Path $channels 'install-scoop.ps1'
            Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/ScoopInstaller/Install/1e2f334083d609986d8c8bc9e31ae8e87c39fab4/install.ps1' -OutFile $installer -UseBasicParsing
            if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne '94F983B190438311E006B957DB7C8422709E0BA62A6C2AC04E278164108F2512') {
                throw 'install.ps1 is not the script read on 15 October 2026; nothing of it runs'
            }
            # The job's user is elevated in every program it starts.
            $r.install = Get-Tail (& { $ErrorActionPreference = 'Continue'; & $installer -ScoopDir $scoopRoot -RunAsAdmin *>&1 | Out-String })
            $r.installed = Test-Path -LiteralPath $scoop
            $r.scoop_version = Invoke-Tool $scoop @('--version')
            # The bucket a release writes, as a local repository, its URL and hash this machine's.
            $bucket = Join-Path $channels 'bucket'
            New-Item -ItemType Directory -Force -Path (Join-Path $bucket 'bucket') | Out-Null
            git -C $bucket init -q -b main
            $template = Get-Content -Raw -LiteralPath (Join-Path $root 'packaging\scoop\pitboard.json')
            $arch = if ($Triple -eq 'aarch64-pc-windows-msvc') { 'arm64' } else { '64bit' }
            foreach ($v in $version, $next) {
                $manifest = $template.Replace('@VERSION@', $v) -replace '@SHA256_[A-Z0-9_]+@', ('0' * 64) | ConvertFrom-Json
                $manifest.architecture.$arch.url = "$base/v$v/$(Split-Path -Leaf $zips[$v].path)"
                $manifest.architecture.$arch.hash = $zips[$v].sha256.ToLowerInvariant()
                $zips[$v].scoop = $manifest | ConvertTo-Json -Depth 8
            }
            Set-Content -LiteralPath (Join-Path $bucket 'bucket\pitboard.json') -Value $zips[$version].scoop -Encoding utf8
            git -C $bucket add -A
            git -C $bucket -c user.name=runner-facts -c user.email=runner-facts@localhost commit -q -m "pitboard $version"
            # Scoop 0.6.0 refuses a repository named without a forward slash, so a folder goes as file:///.
            $r.bucket_add = Invoke-Tool $scoop @('bucket', 'add', 'pitboard-l8', ([Uri] $bucket).AbsoluteUri)
            $r.install_pitboard = Invoke-Tool $scoop @('install', 'pitboard-l8/pitboard')
            $shim = Join-Path $scoopRoot 'shims\pitboard.exe'
            $current = Join-Path $scoopRoot 'apps\pitboard\current'
            $r.shim = Test-Path -LiteralPath $shim
            $r.shim_version = if ($r.shim) { (& $shim --version | Out-String).Trim() } else { $null }
            $r.current_link = (Get-Item -LiteralPath $current -ErrorAction SilentlyContinue).LinkType
            $installed = Join-Path $current 'pitboard.exe'
            $r.installed_is_the_build = (Test-Path -LiteralPath $installed) -and (Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -eq $exeHash
            $r
        }

        Invoke-Row $Result 'l2_path' {
            $r = [ordered]@{ developer_mode = Get-DevMode; winget = [bool] $winget }
            if ($winget) {
                $r.winget_version = (& $winget.Source --version | Out-String).Trim()
                $settings = & $winget.Source settings export | Out-String | ConvertFrom-Json
                $state.LocalManifestsBefore = [bool] $settings.adminSettings.LocalManifestFiles
                $r.local_manifests_before = $state.LocalManifestsBefore
                if (-not $state.LocalManifestsBefore) { $r.enable = Invoke-Tool $winget.Source @('settings', '--enable', 'LocalManifestFiles') }
                $manifests = Join-Path $channels "winget\$version"
                Write-WingetManifest $manifests $version "$base/v$version/$(Split-Path -Leaf $zips[$version].path)" $zips[$version].sha256
                $r.validate = Invoke-Tool $winget.Source @('validate', '--manifest', $manifests)
                $state.WingetManifest = $manifests
                $r.install = Invoke-Tool $winget.Source @('install', '--manifest', $manifests, '--accept-package-agreements', '--accept-source-agreements', '--disable-interactivity')
                $link = Get-Item -LiteralPath (Join-Path $links 'pitboard.exe') -ErrorAction SilentlyContinue
                $r.link = if ($link) { [ordered]@{ type = $link.LinkType; target = Get-PathLabel (Split-Path -Parent $link.Target) $places } } else { $null }
            }
            # The MSI's bin, which it adds to the user PATH, stood in for by a folder added last.
            New-Item -ItemType Directory -Force -Path $msiBin | Out-Null
            Copy-Item -LiteralPath $Pitboard -Destination (Join-Path $msiBin 'pitboard.exe')
            Add-UserPath $msiBin
            # Where cargo binstall and cargo install put it.
            if ((Test-Path -LiteralPath $cargoBin) -and -not (Test-Path -LiteralPath (Join-Path $cargoBin 'pitboard.exe'))) {
                Copy-Item -LiteralPath $Pitboard -Destination (Join-Path $cargoBin 'pitboard.exe')
                $state.CargoCopy = $true
            }
            $r.cargo_bin = $state.CargoCopy
            $entries = Get-NewPath
            $r.order = @($entries | Where-Object { Test-Path -LiteralPath (Join-Path $_.Dir 'pitboard.exe') } |
                    ForEach-Object { [ordered]@{ label = Get-PathLabel $_.Dir $places; from = $_.From } })
            $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:SystemRoot 'System32\where.exe'))
            $start.ArgumentList.Add('pitboard')
            $start.Environment['PATH'] = ($entries.Dir -join ';')
            $start.UseShellExecute = $false
            $start.RedirectStandardOutput = $true
            $where = [Diagnostics.Process]::Start($start)
            $found = $where.StandardOutput.ReadToEnd()
            $where.WaitForExit()
            $r.where = @($found -split "\r?\n" | Where-Object { $_ } | ForEach-Object { Get-PathLabel (Split-Path -Parent $_) $places })
            $r
        }

        Invoke-Row $Result 'l3_update_running' {
            $r = [ordered]@{}
            $current = Join-Path $scoopRoot 'apps\pitboard\current\pitboard.exe'
            if (Test-Path -LiteralPath $current) {
                $bucket = Join-Path $channels 'bucket'
                Set-Content -LiteralPath (Join-Path $bucket 'bucket\pitboard.json') -Value $zips[$next].scoop -Encoding utf8
                git -C $bucket -c user.name=runner-facts -c user.email=runner-facts@localhost commit -q -a -m "pitboard $next"
                $r.scoop_sync = Invoke-Tool $scoop @('update')
                $process = Start-Held $current $homes
                $state.Held.Add($process)
                $r.scoop_running = -not $process.HasExited
                $r.scoop_update_running = Invoke-Tool $scoop @('update', 'pitboard')
                $r.scoop_current_after_running = Split-Path -Leaf (Get-Item -LiteralPath (Join-Path $scoopRoot 'apps\pitboard\current')).Target
                Stop-Held $process
                $r.scoop_update_stopped = Invoke-Tool $scoop @('update', 'pitboard')
                $r.scoop_current_after_stopped = Split-Path -Leaf (Get-Item -LiteralPath (Join-Path $scoopRoot 'apps\pitboard\current')).Target
            }
            $link = Get-Item -LiteralPath (Join-Path $links 'pitboard.exe') -ErrorAction SilentlyContinue
            if ($winget -and $link) {
                $manifests = Join-Path $channels "winget\$next"
                Write-WingetManifest $manifests $next "$base/v$next/$(Split-Path -Leaf $zips[$next].path)" $zips[$next].sha256
                # Each zip holds its build in pitboard-v<version>-<triple>, so the link's folder names
                # the version it leads to.
                $linked = { Split-Path -Leaf (Split-Path -Parent (Get-Item -LiteralPath (Join-Path $links 'pitboard.exe') -ErrorAction SilentlyContinue).Target) }
                $process = Start-Held $link.Target $homes
                $state.Held.Add($process)
                $r.winget_running = -not $process.HasExited
                $r.winget_upgrade_running = Invoke-Tool $winget.Source @('upgrade', '--manifest', $manifests, '--accept-package-agreements', '--accept-source-agreements', '--disable-interactivity')
                $r.winget_link_after_running = & $linked
                $r.winget_old_left_after_running = Test-Path -LiteralPath $link.Target
                Stop-Held $process
                $r.winget_upgrade_stopped = Invoke-Tool $winget.Source @('upgrade', '--manifest', $manifests, '--accept-package-agreements', '--accept-source-agreements', '--disable-interactivity')
                if ($r.winget_upgrade_stopped.exit -eq '0x00000000') { $state.WingetManifest = $manifests }
                $r.winget_link_after_stopped = & $linked
            }
            $r
        }

        # Defender in audit for every rule, then the zip's pitboard.exe marked as downloaded and
        # run three ways. Audit mode logs 1122 and blocks nothing; block mode would log 1121.
        Invoke-Row $Result 'l4_defender' {
            $status = Get-MpComputerStatus
            $preference = Get-MpPreference
            # The prevalence and ransomware rules ask the cloud, so its state says whether they could act.
            $r = [ordered]@{
                real_time = $status.RealTimeProtectionEnabled
                behavior_monitor = $status.BehaviorMonitorEnabled
                tamper_protected = $status.IsTamperProtected
                maps_reporting = $preference.MAPSReporting
                cloud_block_level = $preference.CloudBlockLevel
                submit_samples_consent = $preference.SubmitSamplesConsent
                engine = $status.AMEngineVersion
            }
            $state.AsrBefore = Get-AsrRules
            $r.rules_before = $state.AsrBefore.Count
            $since = Get-Date
            Add-MpPreference -AttackSurfaceReductionRules_Ids $asrRules -AttackSurfaceReductionRules_Actions (@('AuditMode') * $asrRules.Count)
            $after = Get-AsrRules
            $r.rules_in_audit = @($asrRules | Where-Object { $after[$_] -eq 2 }).Count
            $folder = Join-Path $channels 'l4'
            Expand-Archive -LiteralPath $zips[$version].path -DestinationPath $folder
            $exe = Join-Path $folder "pitboard-v$version-$Triple\pitboard.exe"
            Set-Content -LiteralPath $exe -Stream Zone.Identifier -Value "[ZoneTransfer]`r`nZoneId=3"
            $r.runs = [ordered]@{}
            foreach ($run in @(@('version', @('--version')), @('bare', @()), @('completions', @('completions', 'bash')))) {
                $start = [Diagnostics.ProcessStartInfo]::new($exe)
                foreach ($argument in $run[1]) { $start.ArgumentList.Add($argument) }
                foreach ($variable in $homes.Keys) { $start.Environment[$variable] = $homes[$variable] }
                $start.UseShellExecute = $false
                $start.RedirectStandardOutput = $true
                $start.RedirectStandardError = $true
                $process = [Diagnostics.Process]::Start($start)
                $errors = $process.StandardError.ReadToEndAsync()
                $null = $process.StandardOutput.ReadToEnd()
                $process.WaitForExit()
                $null = $errors.Result
                $r.runs[$run[0]] = '0x{0:X8}' -f $process.ExitCode
            }
            Start-Sleep -Seconds 15
            $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'Microsoft-Windows-Windows Defender/Operational'; Id = 1121, 1122; StartTime = $since } -ErrorAction SilentlyContinue)
            $r.events = @($events | ForEach-Object {
                    $message = $_.Message
                    [ordered]@{
                        id = $_.Id
                        rule = if ($message -match 'ID:\s*([0-9A-Fa-f-]{36})') { $Matches[1].ToLowerInvariant() } else { $null }
                        path = if ($message -match 'Path:\s*(.+)') { Split-Path -Leaf $Matches[1].Trim() } else { $null }
                        process = if ($message -match 'Process Name:\s*(.+)') { Split-Path -Leaf $Matches[1].Trim() } else { $null }
                    }
                })
            $r
        }
    } finally {
        foreach ($process in $state.Held) { Stop-Held $process }
        $undo = [ordered]@{}
        if ($null -ne $state.AsrBefore) {
            $undo.asr_restored = Invoke-Fact {
                Remove-MpPreference -AttackSurfaceReductionRules_Ids $asrRules
                foreach ($id in $state.AsrBefore.Keys) {
                    Add-MpPreference -AttackSurfaceReductionRules_Ids $id -AttackSurfaceReductionRules_Actions $state.AsrBefore[$id]
                }
                ((Get-AsrRules) | ConvertTo-Json -Compress) -eq ($state.AsrBefore | ConvertTo-Json -Compress)
            }
        }
        if ($state.WingetManifest) {
            # By id, winget finds no package installed from a local manifest (0x8A150014 in run 38725515950).
            $undo.winget_uninstall = Invoke-Fact { Invoke-Tool $winget.Source @('uninstall', '--manifest', $state.WingetManifest, '--disable-interactivity') }
            $undo.winget_left_by_uninstall = @(& $wingetLeft)
            Remove-Item -Force -LiteralPath (Join-Path $links 'pitboard.exe') -ErrorAction SilentlyContinue
            & $wingetPackages | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
            & $wingetKeys | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
        }
        if ($winget -and $state.LocalManifestsBefore -eq $false) {
            $undo.local_manifests = Invoke-Fact { Invoke-Tool $winget.Source @('settings', '--disable', 'LocalManifestFiles') }
        }
        if (Test-Path -LiteralPath $scoop) {
            $undo.scoop_uninstall = Invoke-Fact { Invoke-Tool $scoop @('uninstall', 'pitboard') }
        }
        if ($state.CargoCopy) { Remove-Item -Force -LiteralPath (Join-Path $cargoBin 'pitboard.exe') -ErrorAction SilentlyContinue }
        if (-not $scoopConfigBefore) { Remove-Item -Recurse -Force -LiteralPath $scoopConfig -ErrorAction SilentlyContinue }
        $undo.environment_restored = Invoke-Fact { Restore-UserEnvironment $environment }
        if ($server) { Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue }
        Remove-Item -Recurse -Force -LiteralPath $channels -ErrorAction SilentlyContinue
        $left = [Collections.Generic.List[string]]::new()
        if ($state.CargoCopy -and (Test-Path -LiteralPath (Join-Path $cargoBin 'pitboard.exe'))) { $left.Add('cargo_bin') }
        foreach ($label in & $wingetLeft) { $left.Add($label) }
        if (Test-Path -LiteralPath $channels) { $left.Add('scratch') }
        $undo.left = @($left)
        $Result.undo = $undo
        Save-Report
    }
}

# RF-T8: probe tasks of the job user's with the renewal's daily trigger, each pair one with
# StartWhenAvailable and one without: on and off begin 5 minutes before they are registered;
# held-on and held-off begin 3 minutes after, registered disabled and enabled once that start
# has passed. Read near the end of the run, since Task Scheduler starts a missed start after
# a delay, 10 minutes by default.
$missedStarts = 'on', 'off', 'held-on', 'held-off'

function Register-MissedStart($Result, [string] $In) {
    $script:missedStartRegistered = Get-Date
    foreach ($kind in $missedStarts) {
        $out = Join-Path $In "pitboard-probe-swa-$kind.json"
        $register = @('task', '--scratch', $In, '--label', "swa-$kind", '--action', 'register', '--args', "--out $out logon", '--allow-battery')
        if ($kind -like 'held-*') { $register += '--daily-in-minutes', '3', '--disabled' } else { $register += '--daily-since-minutes', '5' }
        if ($kind -like '*on') { $register += '--start-when-available' }
        $Result[$kind] = [ordered]@{ register = Invoke-Probe "swa-$kind-register" $register }
    }
}

function Enable-MissedStart($Result, [string] $In) {
    $wait = ($script:missedStartRegistered.AddMinutes(4) - (Get-Date)).TotalSeconds
    if ($wait -gt 0) { Start-Sleep -Seconds ([int] [math]::Ceiling($wait)) }
    $script:missedStartEnabled = Get-Date
    $Result.enabled_after_minutes = [math]::Round(($script:missedStartEnabled - $script:missedStartRegistered).TotalMinutes, 1)
    foreach ($kind in 'held-on', 'held-off') {
        $Result[$kind].enable = Invoke-Probe "swa-$kind-enable" @('task', '--scratch', $In, '--label', "swa-$kind", '--action', 'enable')
    }
}

function Read-MissedStart($Result, [string] $In) {
    $since = if ($script:missedStartEnabled) { $script:missedStartEnabled } else { $script:missedStartRegistered }
    $deadline = $since.AddMinutes(14)
    $awaited = @('on', 'held-on' | ForEach-Object { Join-Path $In "pitboard-probe-swa-$_.json" })
    while (($awaited | Where-Object { -not (Test-Path -LiteralPath $_) }) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 5 }
    $Result.read_after_minutes = [math]::Round(((Get-Date) - $script:missedStartRegistered).TotalMinutes, 1)
    foreach ($kind in $missedStarts) {
        $out = Join-Path $In "pitboard-probe-swa-$kind.json"
        $Result[$kind].read = Invoke-Probe "swa-$kind-read" @('task', '--label', "swa-$kind", '--action', 'read')
        $Result[$kind].ran = Test-Path -LiteralPath $out
        if ($Result[$kind].ran) {
            $wrote = (Get-Item -LiteralPath $out).LastWriteTime
            $Result[$kind].ran_after_seconds = [int] ($wrote - $script:missedStartRegistered).TotalSeconds
            if ($kind -like 'held-*' -and $script:missedStartEnabled) {
                $Result[$kind].ran_after_enabling_seconds = [int] ($wrote - $script:missedStartEnabled).TotalSeconds
            }
        }
        $Result[$kind].delete = Invoke-Probe "swa-$kind-delete" @('task', '--scratch', $In, '--label', "swa-$kind", '--action', 'delete')
    }
}

# RF-T7: Claude Code's remote token path names no drive. A dummy at \home\claude\.claude\remote
# on an NTFS VHD of its own, read by Bun as Claude Code reads it, from a folder on that volume
# and from one on C:, then the VHD detached and removed.
function Measure-RemoteTokenFile($Result, [string] $Bun) {
    $dir = 'C:\pitboard-probe-t7'
    $onC = Join-Path $dir 'cwd'
    New-Item -ItemType Directory -Force -Path $onC | Out-Null
    $file = Join-Path $dir 'pitboard-probe-t7.vhdx'
    $letter = Get-FreeLetter
    $dummy = 'pitboard-probe: a dummy, never a token'
    $Result.c_has_home_claude = Test-Path -LiteralPath 'C:\home\claude'
    $create = Join-Path $dir 'create.txt'
    @(
        "create vdisk file=`"$file`" maximum=64 type=expandable",
        "select vdisk file=`"$file`"",
        'attach vdisk',
        'create partition primary',
        'format fs=ntfs quick label=PBT7',
        "assign letter=$letter"
    ) | Set-Content -Path $create -Encoding ascii
    try {
        $made = diskpart /s $create | Out-String
        $Result.diskpart_exit = $LASTEXITCODE
        $Result.mounted = Test-Path -Path "$($letter):\"
        if (-not $Result.mounted) {
            $Result.diskpart_tail = ($made -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -Last 4) -join ' | '
            return
        }
        $remote = "$($letter):\home\claude\.claude\remote"
        New-Item -ItemType Directory -Force -Path $remote | Out-Null
        [IO.File]::WriteAllText((Join-Path $remote '.oauth_token'), "$dummy`n")
        $onVolume = "$($letter):\pitboard-probe-t7"
        New-Item -ItemType Directory -Force -Path $onVolume | Out-Null
        $helper = Join-Path $toolHelpers 'remote-token.mjs'
        foreach ($from in @(@('from_the_volume', $onVolume), @('from_c', $onC))) {
            $o = Join-Path $scratch "t7-$($from[0])-out.txt"
            $process = Start-Process -FilePath $Bun -ArgumentList (Format-Arguments @($helper, $dummy)) -WorkingDirectory $from[1] -NoNewWindow -PassThru `
                -RedirectStandardOutput $o -RedirectStandardError (Join-Path $scratch "t7-$($from[0])-err.txt")
            $null = $process.Handle
            $waited = Wait-Bounded $process 60
            $Result[$from[0]] = [ordered]@{ timed_out = $waited.TimedOut; exit = $waited.Exit; report = (Read-Json $o) }
        }
    } finally {
        $detach = Join-Path $dir 'detach.txt'
        @("select vdisk file=`"$file`"", 'detach vdisk') | Set-Content -Path $detach -Encoding ascii
        diskpart /s $detach *> $null
        $Result.detach_exit = $LASTEXITCODE
        Remove-Item -Recurse -Force -Path $dir -ErrorAction SilentlyContinue
        $Result.removed = -not (Test-Path -Path $dir)
    }
}

# Anything a row left behind: the probe's tasks, a \Pitboard\ folder the run made, the tools.
function Remove-Leftovers([bool] $PitboardFolderBefore) {
    Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -like 'pitboard-probe-*' } |
        Unregister-ScheduledTask -Confirm:$false -ErrorAction SilentlyContinue
    $service = New-Object -ComObject 'Schedule.Service'
    $service.Connect()
    $root = $service.GetFolder('\')
    foreach ($folder in @('pitboard-probe') + $(if ($PitboardFolderBefore) { @() } else { @('Pitboard') })) {
        try { $root.DeleteFolder($folder, 0) } catch { $null = $_ }
    }
    Remove-Item -Recurse -Force -Path $tools -ErrorAction SilentlyContinue
}

function Test-PitboardTaskFolder {
    $service = New-Object -ComObject 'Schedule.Service'
    $service.Connect()
    try { $null = $service.GetFolder('\Pitboard'); $true } catch { $false }
}

$pitboardFolderBefore = Test-PitboardTaskFolder
$report.pitboard_task_folder_before = $pitboardFolderBefore
Set-Content -Path $marker -Value 'ci runner, disposable'
try {
    $report.job_user = Invoke-Probe 'runner-facts' @('runner-facts', '--scratch', $scratch)
    $report.policies = Invoke-Fact { Get-Policies }
    $report.start_when_available = [ordered]@{}
    $missedStart = Join-Path $scratch 'swa'
    try { Register-MissedStart $report.start_when_available $missedStart } catch { $report.start_when_available.error = $_.Exception.Message }
    Save-Report
    $report.vhd = [System.Collections.Generic.List[object]]::new()
    try { Get-Vhds $report.vhd } catch { $report.vhd_error = $_.Exception.Message }
    Save-Report

    $report.tools = [ordered]@{}
    $bun = $null
    try { $bun = Install-ProbeTools $report.tools $BunVersion $BunZip $BunSha256 } catch { $report.tools.error = $_.Exception.Message }
    Save-Report
    $report.remote_token_file = [ordered]@{}
    if ($bun) {
        try { Measure-RemoteTokenFile $report.remote_token_file $bun } catch { $report.remote_token_file.error = $_.Exception.Message }
        Save-Report
    }
    if ($script:missedStartRegistered) {
        try { Enable-MissedStart $report.start_when_available $missedStart } catch { $report.start_when_available.enable_error = $_.Exception.Message }
        Save-Report
    }

    $report.fresh_standard_user = [ordered]@{}
    $report.non_ascii_user = [ordered]@{}
    $nonAsciiMeasured = $false
    $report.refs = [ordered]@{}
    $report.smb_share = [ordered]@{}
    $report.cf8 = [ordered]@{}
    $report.openssh = [ordered]@{}
    $fresh = $null
    try {
        $fresh = New-ThrowawayUser 'fresh' ('pbprobe' + (Get-Random -Minimum 10000 -Maximum 99999)) @($usersSid) $report.fresh_standard_user
        Save-Report
        if ($fresh.ProfileDir) {
            Measure-FreshUser $fresh $report.fresh_standard_user $bun $ProbeX64
            try { Measure-ReFs $fresh $report.refs $bun } catch { $report.refs.error = $_.Exception.Message }
            Save-Report
            try { Measure-SmbShare $fresh $report.smb_share } catch { $report.smb_share.error = $_.Exception.Message }
            Save-Report
            try { Measure-SparseIdentity $fresh $report.cf8 } catch { $report.cf8.error = $_.Exception.Message }
            Save-Report
            $opensshHome = $null
            try {
                $opensshHome = Install-OpenSsh $report.openssh $OpenSshTag $OpenSshZip $OpenSshSha256
                Measure-OpenSsh $fresh $report.openssh
            } catch {
                $report.openssh.error = $_.Exception.Message
            } finally {
                if ($opensshHome) { Uninstall-OpenSsh $opensshHome $report.openssh }
            }
            Save-Report
            Measure-NonAsciiUser $report.non_ascii_user $bun $fresh
            $nonAsciiMeasured = $true
            Measure-PasswordChanges $fresh $report.fresh_standard_user
        }
    } catch {
        $report.fresh_standard_user.error = $_.Exception.Message
    } finally {
        if ($fresh) { Remove-ThrowawayUser $fresh $report.fresh_standard_user }
        Save-Report
    }

    if (-not $nonAsciiMeasured) { Measure-NonAsciiUser $report.non_ascii_user $bun $null }
    $report.fresh_admin = [ordered]@{}
    Measure-FreshAdmin $report.fresh_admin

    $report.job_user_rows = [ordered]@{}
    try { Measure-JobUser $report.job_user_rows } catch { $report.job_user_rows.error = $_.Exception.Message }

    $report.channels = [ordered]@{}
    try { Measure-Channels $report.channels } catch { $report.channels.error = $_.Exception.Message }

    if ($script:missedStartRegistered) {
        try { Read-MissedStart $report.start_when_available $missedStart } catch { $report.start_when_available.read_error = $_.Exception.Message }
        Save-Report
    }
} finally {
    try { Remove-Leftovers $pitboardFolderBefore } catch { $report.leftovers_error = $_.Exception.Message }
    $report.tools_removed = -not (Test-Path -Path $tools)
    Remove-Item -Force -Path $marker -ErrorAction SilentlyContinue
    $report.unreadable_reports = @($script:unreadable)
    Save-Report
}
Get-Content -Raw -Path $Out
if ($null -eq $report.job_user.report) {
    throw 'the probe wrote no runner-facts report'
}
if ($report.job_user.report.ok -eq $false) {
    throw "the probe refused its runner-facts report: $($report.job_user.report.error)"
}
if ($script:unreadable.Count -gt 0) {
    throw "ConvertFrom-Json could not read $($script:unreadable -join ', '); the report keeps each one's parse error and text"
}
exit 0
