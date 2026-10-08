<#
.SYNOPSIS
The runner facts of W11, gathered on a Windows CI leg as the job's user. The runner is
disposable: this drops the probe's throwaway marker, makes and mounts two small VHDs, makes a
throwaway standard user, turns Developer Mode on and grants that user the symbolic-link right
for one run each, and undoes each of those again, exactly.

.DESCRIPTION
Everything goes into one JSON report at -Out, written whatever happens. A fact that cannot be
gathered is recorded as an error in the report and does not fail the step. The step fails
when the probe's own report is missing, or when any report the probe printed cannot be
parsed: both are bugs in the probe. An unparsed report is recorded with its parse error and
its text, which holds nothing secret (the probe redacts what it prints), and the step fails
after the report is written. Nothing a program prints to standard output is parsed but the
probe's reports, so children's output cannot break the report. No SID and no user name is
written into it.
#>
param(
    [Parameter(Mandatory)] [string] $Probe,
    [Parameter(Mandatory)] [string] $Triple,
    [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = 'Stop'

# A GitHub-hosted runner is thrown away after the job.
if (-not ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted')) {
    throw 'runner-facts.ps1 runs only on a GitHub-hosted runner: it makes a Windows account, mounts disks and turns on Developer Mode and the symbolic-link right for a run'
}

$Probe = (Resolve-Path -Path $Probe).Path
$scratch = Join-Path $env:RUNNER_TEMP 'pitboard-probe-scratch'
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
# The probe looks for its marker in the folder FOLDERID_Profile names, which is what this
# returns.
$profileDir = [Environment]::GetFolderPath('UserProfile')
$marker = Join-Path $profileDir 'pitboard-probe-throwaway.marker'
# The files of the probe's reports that ConvertFrom-Json could not read. The step fails on
# any.
$script:unreadable = [System.Collections.Generic.List[string]]::new()

# A report the probe wrote, parsed; $null when there is none. One that does not parse is
# kept as its parse error and its text, and named in $script:unreadable.
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

# Run the probe with its report written to a file, and hand the report back. The probe
# writes only a file whose name begins pitboard-probe-.
function Invoke-Probe([string] $Label, [string[]] $ProbeArgs) {
    $file = Join-Path $scratch "pitboard-probe-report-$Label.json"
    Remove-Item -Force -Path $file -ErrorAction SilentlyContinue
    & $Probe --out $file @ProbeArgs *> $null
    [ordered]@{ exit = $LASTEXITCODE; report = (Read-Json $file) }
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
        developer_mode = if (Test-Path $devKey) {
            (Get-ItemProperty -Path $devKey -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
        } else { $null }
        winget_present = $null -ne (Get-Command -Name 'winget' -ErrorAction SilentlyContinue)
        scoop_present = $null -ne (Get-Command -Name 'scoop' -ErrorAction SilentlyContinue)
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
            $letter = [char[]](68..90) | Where-Object { -not (Test-Path -Path "$($_):\") } | Select-Object -Last 1
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

# W12 and W13's mechanism: can the job start a process as a fresh local standard user, and
# what can that user do. Each run's standard output goes to a file the job reads; a run that
# cannot reach the window station exits 0xC0000142. Each part writes into $Result as it
# goes, so what was gathered survives a later part's failure.
#
# CI run 37660991344 started these with -LoadUserProfile and every symlink run refused with
# "cannot read the real profile": the marker was found through FOLDERID_Profile, so it was
# FOLDERID_LocalAppData the guard could not read. Start-Process hands a child the job user's
# environment unless told otherwise, -Credential or not; the user's Local AppData value is
# %USERPROFILE%\AppData\Local, so it expanded to the job user's folder, which a standard user
# cannot open, and SHGetKnownFolderPath fails on a folder it cannot verify. So once the first
# run has made the profile, the runs are given the user's own profile variables with
# -Environment (PowerShell 7.4 on); the report says which environment each run had, and the
# homes run shows what each known folder read in it.
function Get-FreshUser($Result) {
    $name = 'pbprobe' + (Get-Random -Minimum 10000 -Maximum 99999)
    $secure = ConvertTo-SecureString -String ([guid]::NewGuid().ToString('N') + 'Aa1!') -AsPlainText -Force
    $user = New-LocalUser -Name $name -Password $secure -AccountNeverExpires -PasswordNeverExpires
    Add-LocalGroupMember -Group 'Users' -Member $name -ErrorAction SilentlyContinue
    $sid = $user.SID.Value
    $cred = [pscredential]::new($name, $secure)
    $runDir = 'C:\pitboard-probe-runas'
    $userEnv = $null
    $Result.start_process_takes_environment = (Get-Command -Name 'Start-Process').Parameters.ContainsKey('Environment')

    $runAs = {
        param([string] $Label, [string[]] $ProbeArgs)
        $stdout = Join-Path $scratch "runas-$Label-out.txt"
        $stderr = Join-Path $scratch "runas-$Label-err.txt"
        $start = @{
            FilePath = $Probe
            ArgumentList = $ProbeArgs
            Credential = $cred
            WorkingDirectory = $runDir
            LoadUserProfile = $true
            PassThru = $true
            Wait = $true
            RedirectStandardOutput = $stdout
            RedirectStandardError = $stderr
        }
        if ($userEnv) { $start.Environment = $userEnv }
        try {
            $p = Start-Process @start
            [ordered]@{
                started = $true
                environment = if ($userEnv) { 'the user''s own profile variables' } else { 'the job user''s' }
                exit = $p.ExitCode
                exit_hex = ('0x{0:X8}' -f $p.ExitCode)
                report = (Read-Json $stdout)
            }
        } catch {
            [ordered]@{ started = $false; error = $_.Exception.Message }
        }
    }

    try {
        New-Item -ItemType Directory -Force -Path $runDir | Out-Null
        icacls $runDir /grant "${name}:(OI)(CI)M" *> $null
        $Result.logon = & $runAs 'logon' @('logon')
        $Result.unsigned_probe_ran = ($Result.logon.started -and $Result.logon.exit -eq 0)
        $userProfile = (Get-CimInstance -ClassName Win32_UserProfile | Where-Object { $_.SID -eq $sid }).LocalPath
        $Result.profile_made = [bool] $userProfile
        if ($userProfile) {
            Set-Content -Path (Join-Path $userProfile 'pitboard-probe-throwaway.marker') -Value 'ci runner, disposable'
            if ($Result.start_process_takes_environment) {
                $userEnv = @{
                    USERPROFILE = $userProfile
                    HOMEDRIVE = Split-Path -Path $userProfile -Qualifier
                    HOMEPATH = Split-Path -Path $userProfile -NoQualifier
                    APPDATA = Join-Path $userProfile 'AppData\Roaming'
                    LOCALAPPDATA = Join-Path $userProfile 'AppData\Local'
                    TEMP = Join-Path $userProfile 'AppData\Local\Temp'
                    TMP = Join-Path $userProfile 'AppData\Local\Temp'
                    USERNAME = $name
                }
            }
        }
        $Result.tokens = & $runAs 'tokens' @('tokens')
        $Result.homes = & $runAs 'homes' @('homes')
        $Result.symlink_as_is = & $runAs 'sym-as-is' @('symlink', '--scratch', (Join-Path $runDir 'as-is'))

        # Developer Mode for one run. The key is made only when it is missing (New-Item -Force
        # on an existing key would replace it and drop its other values), and the value and
        # its kind are put back exactly as they were, or removed with the key when the run
        # made them.
        try {
            $devKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
            $devBefore = Get-DevMode
            try {
                if (-not $devBefore.key) { New-Item -Path $devKey | Out-Null }
                Set-ItemProperty -Path $devKey -Name 'AllowDevelopmentWithoutDevLicense' -Value 1 -Type DWord
                $Result.symlink_developer_mode = & $runAs 'sym-devmode' @('symlink', '--scratch', (Join-Path $runDir 'devmode'))
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

        # The symbolic-link right for one run, added for this user beside the accounts that
        # already held it, then set back to exactly those accounts, which is nobody when the
        # export had no line for it; read back and compared.
        try {
            $holders = Get-SymlinkHolders (Join-Path $scratch 'pitboard-probe-rights-before.inf')
            $Result.right_held_by_anyone_before = [bool] $holders
            $grant = if ($holders) { "$holders,*$sid" } else { "*$sid" }
            $Result.right_granted_exit = Set-SymlinkHolders $grant 'grant'
            try {
                $Result.symlink_right_granted = & $runAs 'sym-right' @('symlink', '--scratch', (Join-Path $runDir 'right'))
            } finally {
                $Result.right_restore_exit = Set-SymlinkHolders $holders 'restore'
                $Result.right_restored_exactly = (
                    (Get-SymlinkHolders (Join-Path $scratch 'pitboard-probe-rights-after.inf')) -eq $holders)
            }
        } catch {
            $Result.right_error = $_.Exception.Message
        }
    } finally {
        Remove-LocalUser -Name $name -ErrorAction SilentlyContinue
        Get-CimInstance -ClassName Win32_UserProfile | Where-Object { $_.SID -eq $sid } |
            Remove-CimInstance -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force -Path $runDir -ErrorAction SilentlyContinue
    }
}

$report = [ordered]@{ triple = $Triple }
Set-Content -Path $marker -Value 'ci runner, disposable'
try {
    $report.job_user = Invoke-Probe 'runner-facts' @('runner-facts', '--scratch', $scratch)
    $report.policies = Invoke-Fact { Get-Policies }
    $report.vhd = [System.Collections.Generic.List[object]]::new()
    try { Get-Vhds $report.vhd } catch { $report.vhd_error = $_.Exception.Message }
    $report.fresh_standard_user = [ordered]@{}
    try { Get-FreshUser $report.fresh_standard_user } catch { $report.fresh_standard_user.error = $_.Exception.Message }
} finally {
    Remove-Item -Force -Path $marker -ErrorAction SilentlyContinue
    $report.unreadable_reports = @($script:unreadable)
    $report | ConvertTo-Json -Depth 64 | Set-Content -Path $Out -Encoding utf8
}
Get-Content -Raw -Path $Out
if ($null -eq $report.job_user.report) {
    throw 'the probe wrote no runner-facts report'
}
if ($report.job_user.report.ok -eq $false) {
    throw "the probe refused its runner-facts report: $($report.job_user.report.error)"
}
if ($script:unreadable.Count -gt 0) {
    throw "ConvertFrom-Json could not read the probe's $($script:unreadable -join ', '); the report keeps each one's parse error and text"
}
exit 0
