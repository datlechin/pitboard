#!/bin/sh
# linux-runner-facts.sh <part> [option...] --out <file>: one part of what linux-facts.yml
# measures, as a JSON report saved after each row, with homes, user names, the host name and the
# machine id taken out.
#   user-manager --pitboard <file>: a throwaway user's own systemd running that pitboard.
#   inotify --pitboard <file>: what inotify says of its save of state.json on four file systems.
#   installs --stand-in <file> --claude-code <version> --codex <version>: npm's chain to each
#     tool's program, and /proc's names for the stand-in started in its place. Neither tool runs.
#   display-setup: as root in a container, the display part's packages and the user it runs as.
#   display: as that user, headless mutter, GTK's renderer, AT-SPI and screenshots.
# The first three run only on a GitHub-hosted runner and the display parts only in a container:
# they make users, mount file systems and install packages, and each undoes in a trap what it
# made. A fact that cannot be gathered is recorded with its exit and output.
set -eu

snapshot=20261028T000000Z
throwaway=pbfacts
self=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")

usage() {
  echo "usage: linux-runner-facts.sh <user-manager|inotify|installs|display-setup|display> [option...] --out <file>" >&2
  exit 2
}

[ "$#" -gt 0 ] || usage
part=$1
shift
out='' pitboard='' stand_in='' claude_code='' codex=''
# display-mutter, which display-session starts inside mutter, takes the scale alone.
[ "$part" = display-mutter ] || while [ "$#" -gt 0 ]; do
  [ "$#" -ge 2 ] || usage
  case $1 in
    --out) out=$2 ;;
    --pitboard) pitboard=$2 ;;
    --stand-in) stand_in=$2 ;;
    --claude-code) claude_code=$2 ;;
    --codex) codex=$2 ;;
    *) usage ;;
  esac
  shift 2
done

absolute() {
  case $1 in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "$PWD" "$1" ;;
  esac
}

on_a_runner() {
  [ "${GITHUB_ACTIONS:-}" = true ] && [ "${RUNNER_ENVIRONMENT:-}" = github-hosted ]
}

# sd_booted(3)'s test, as the core's: a machine systemd started, which no container is that
# Docker, Podman or an image build runs.
started_by_systemd() {
  [ -d /run/systemd/system ]
}

# start_report <folder>: the report as gathered stays in a scratch folder made in <folder>, and
# --out gets its redacted copy.
start_report() {
  [ -n "$out" ] || usage
  out=$(absolute "$out")
  scratch=$(mktemp -d "$1/linux-facts.XXXXXX")
  raw=$scratch/report.json
  hide=$scratch/hide.json
  jq -n -c '[]' >"$hide"
  hiding "$HOME" '<job-home>'
  hiding "$(id -un)" '<job-user>'
  hiding "$(uname -n)" '<host>'
  if [ -r /etc/machine-id ]; then
    hiding "$(cat /etc/machine-id)" '<machine-id>'
  fi
  jq -n --arg part "$part" --arg arch "$(uname -m)" --arg kernel "$(uname -r)" \
    --arg os "$(sed -n 's/^PRETTY_NAME=//p' /etc/os-release | tr -d '"')" \
    '{part: $part, arch: $arch, kernel: $kernel, os: $os}' >"$raw"
  save
}

# hiding <text> <placeholder>: the report never holds <text>, only <placeholder> in its place.
hiding() {
  jq -c --arg text "$1" --arg as "$2" 'if $text == "" then . else . + [[$text, $as]] end' \
    "$hide" >"$hide.next"
  mv "$hide.next" "$hide"
}

# The longest text first, so a home goes before the user name inside it.
save() {
  jq --slurpfile hide "$hide" '
    ($hide[0] | sort_by(.[0] | -length)) as $pairs
    | walk(if type == "string" then reduce $pairs[] as $p (.; split($p[0]) | join($p[1])) else . end)
  ' "$raw" >"$out.next"
  mv "$out.next" "$out"
}

# row <key>: the JSON value on standard input becomes the report's <key>.
row() {
  jq --arg key "$1" --slurpfile value /dev/stdin '.[$key] = $value[0]' "$raw" >"$raw.next"
  mv "$raw.next" "$raw"
  save
}

# ran <command...>: the command run with its input closed and its output to files, as
# {exit, out, err}. A command that fails is a fact like any other.
ran() {
  if "$@" >"$scratch/ran.out" 2>"$scratch/ran.err" </dev/null; then code=0; else code=$?; fi
  jq -n --argjson exit "$code" --rawfile out "$scratch/ran.out" --rawfile err "$scratch/ran.err" \
    '{exit: $exit, out: $out, err: $err}'
}

# As ran, with its output read as JSON where it is, as pitboard's --json is.
ran_json() {
  ran "$@" | jq '.out = (.out as $text | try ($text | fromjson) catch $text)'
}

# { fact; fact; } | facts <name...>: the facts on standard input as one object, by name.
facts() {
  jq -n --args '[inputs] as $values | [$ARGS.positional, $values] | transpose
    | map({key: .[0], value: .[1]}) | from_entries' "$@"
}

# waits <seconds> <command...>: whether the command succeeds within that many seconds.
waits() {
  seconds=$1
  shift
  while ! "$@" 2>/dev/null; do
    [ "$seconds" -gt 0 ] || return 1
    seconds=$((seconds - 1))
    sleep 1
  done
}

installed() {
  dpkg-query -W -f '${Status}' "$1" 2>/dev/null | grep -q '^install ok installed$'
}

# Installs those of the packages the image lacks, and lists them in $added for the trap. Never
# in a pipeline, whose subshell would keep $added to itself.
add_packages() {
  missing=''
  for package in "$@"; do
    installed "$package" || missing="$missing $package"
  done
  echo "missing:$missing"
  [ -n "$missing" ] || return 0
  # shellcheck disable=SC2086 # a list of package names
  sudo -n env DEBIAN_FRONTEND=noninteractive apt-get install --update -y --no-install-recommends \
    $missing || return
  added="$added$missing"
}

remove_added() {
  if [ -n "${added:-}" ]; then
    # shellcheck disable=SC2086 # a list of package names
    sudo -n env DEBIAN_FRONTEND=noninteractive apt-get purge -y --auto-remove $added \
      >/dev/null 2>&1 || true
  fi
}

on_a_runner_only() {
  if ! on_a_runner; then
    echo "linux-runner-facts.sh $part runs only on a GitHub-hosted runner: it makes users, mounts file systems and installs packages" >&2
    exit 1
  fi
}

# A command as a terminal of the throwaway user has it: that user's home and runtime folder,
# none of the job's variables, sudo's own included.
as_user() {
  sudo -n -u "$throwaway" -- env -i HOME="$user_home" USER="$throwaway" LOGNAME="$throwaway" \
    SHELL=/bin/sh LANG=C.UTF-8 PATH=/usr/local/bin:/usr/bin:/bin XDG_RUNTIME_DIR="/run/user/$uid" \
    "$@"
}

user_systemctl() {
  as_user systemctl --user --no-pager "$@"
}

watch_running() {
  [ "$(user_systemctl show pitboard-watch.service --property=MainPID --value)" != 0 ]
}

units_folder() {
  as_user ls -lAR "$1"
}

user_gone() {
  ! pgrep -u "$throwaway" >/dev/null
}

process_path() {
  sudo -n cat "/proc/$1/environ" | tr '\0' '\n' | grep '^PATH='
}

# A drop-in of the throwaway user's that has systemd refuse to start Pitboard's timer by hand.
refuse_start() {
  as_user mkdir -p "$user_home/.config/systemd/user/pitboard-renew.timer.d"
  printf '[Unit]\nRefuseManualStart=yes\n' |
    as_user tee "$user_home/.config/systemd/user/pitboard-renew.timer.d/refuse.conf" >/dev/null
  user_systemctl daemon-reload
}

allow_start() {
  as_user rm -rf "$user_home/.config/systemd/user/pitboard-renew.timer.d"
  user_systemctl daemon-reload
}

unit_sums() {
  as_user sha256sum "$user_home/.config/systemd/user/pitboard-renew.service" \
    "$user_home/.config/systemd/user/pitboard-renew.timer"
}

user_manager_done() {
  set +e
  if [ -n "${uid:-}" ]; then
    as_user "$user_home/.local/bin/pitboard" --json uninstall --yes >/dev/null 2>&1
    sudo -n loginctl disable-linger "$throwaway"
    sudo -n loginctl terminate-user "$throwaway"
    sudo -n systemctl stop "user@$uid.service"
    waits 30 user_gone
    sudo -n pkill -KILL -u "$throwaway"
    sudo -n userdel --remove "$throwaway" 2>/dev/null
  fi
  rm -rf "$scratch"
}

user_manager() {
  on_a_runner_only
  [ -n "$pitboard" ] || usage
  if id -u "$throwaway" >/dev/null 2>&1; then
    echo "a user $throwaway is there already, and linux-runner-facts.sh removes only one it made" >&2
    exit 1
  fi
  pitboard=$(absolute "$pitboard")
  start_report "$RUNNER_TEMP"
  cd /
  trap user_manager_done EXIT
  trap 'exit 130' INT TERM
  user_home=/home/$throwaway
  hiding "$user_home" '<home>'
  hiding "$throwaway" '<user>'
  program=$user_home/.local/bin/pitboard

  { ran systemctl --version; ran dpkg-query -W dbus-user-session systemd; } | facts systemd packages |
    row system
  sudo -n useradd --create-home --shell /bin/sh "$throwaway"
  uid=$(id -u "$throwaway")
  sudo -n loginctl enable-linger "$throwaway"
  # logind starts the user's manager once linger is on, with no session.
  if waits 60 sudo -n test -S "/run/user/$uid/systemd/private"; then started=true; else started=false; fi
  {
    jq -n --argjson started "$started" '$started'
    ran loginctl show-user "$throwaway" --property=Linger --property=State --property=Sessions
    ran systemctl is-active "user@$uid.service"
    ran sudo -n test -S "/run/user/$uid/bus"
  } | facts started user manager session_bus | row linger
  ran user_systemctl show --property=Version --property=UnitPath --property=Environment |
    row manager
  ran user_systemctl show-environment | row manager_environment

  # Its folders made as the user: install -D would make them root's.
  as_user mkdir -p "$user_home/.local/bin"
  sudo -n install -o "$throwaway" -g "$throwaway" -m 0755 "$pitboard" "$program"
  ran as_user "$program" --version | row version
  ran_json as_user "$program" --json vault | row vault
  ran_json as_user "$program" --json schedule install | row schedule_install
  {
    ran units_folder "$user_home/.config/systemd/user"
    ran as_user cat "$user_home/.config/systemd/user/pitboard-renew.service"
    ran as_user cat "$user_home/.config/systemd/user/pitboard-renew.timer"
    ran user_systemctl list-timers --all pitboard-renew.timer
    ran user_systemctl show pitboard-renew.timer --property=LoadState --property=ActiveState \
      --property=UnitFileState --property=TimersCalendar --property=NextElapseUSecRealtime \
      --property=Persistent --property=RandomizedDelayUSec
    ran user_systemctl show pitboard-renew.service --property=LoadState --property=UnitFileState \
      --property=WantedBy --property=After --property=ConditionResult
  } | facts folder service timer timers timer_state service_state | row schedule_units
  ran_json as_user "$program" --json schedule status | row schedule_status
  {
    ran user_systemctl start pitboard-renew.service
    ran user_systemctl show pitboard-renew.service --property=ActiveState --property=Result \
      --property=ExecMainStatus --property=ConditionResult
  } | facts start state | row renewal_run

  ran_json as_user "$program" --json watch install | row watch_install
  waits 10 watch_running || true
  watch_pid=$(user_systemctl show pitboard-watch.service --property=MainPID --value)
  {
    ran as_user cat "$user_home/.config/systemd/user/pitboard-watch.service"
    ran user_systemctl show pitboard-watch.service --property=ActiveState --property=SubState \
      --property=UnitFileState --property=Restart --property=WantedBy
    ran process_path "$watch_pid"
  } | facts service state path | row watch_units

  # A service whose Pitboard is gone, as after its package is removed.
  as_user mv "$program" "$program.away"
  {
    ran user_systemctl start pitboard-renew.service
    ran user_systemctl show pitboard-renew.service --property=ActiveState --property=Result \
      --property=ConditionResult
    ran user_systemctl is-failed pitboard-renew.service
    ran user_systemctl restart pitboard-watch.service
    ran user_systemctl show pitboard-watch.service --property=ActiveState --property=SubState \
      --property=Result --property=ConditionResult
    ran user_systemctl is-failed pitboard-watch.service
  } | facts renewal_start renewal renewal_is_failed watch_restart watch watch_is_failed |
    row program_gone
  as_user mv "$program.away" "$program"
  ran user_systemctl restart pitboard-watch.service | row watch_back

  # Where the user manager reads no units, or runs them with another home.
  as_user mkdir -p "$user_home/elsewhere" "$user_home/other"
  {
    ran_json as_user env XDG_CONFIG_HOME="$user_home/elsewhere" "$program" --json schedule install
    ran units_folder "$user_home/elsewhere"
  } | facts refused written | row config_elsewhere
  {
    ran_json as_user env HOME="$user_home/other" "$program" --json schedule install
    ran units_folder "$user_home/other"
  } | facts refused written | row home_elsewhere

  {
    ran_json as_user "$program" --json uninstall --yes
    ran units_folder "$user_home/.config/systemd/user"
    ran units_folder "$user_home/.local/share/systemd/timers"
    ran user_systemctl list-unit-files 'pitboard-*'
    ran user_systemctl list-timers --all
  } | facts uninstall folder timer_stamps unit_files timers | row uninstall

  # A timer masked first, as `systemctl --user mask` and `mask --runtime` mask it.
  for mask in mask runtime; do
    if [ "$mask" = runtime ]; then set -- --runtime; else set --; fi
    {
      ran user_systemctl mask "$@" pitboard-renew.timer
      ran_json as_user "$program" --json schedule install
      ran units_folder "$user_home/.config/systemd/user"
      ran user_systemctl show pitboard-renew.timer --property=LoadState --property=ActiveState \
        --property=UnitFileState --property=FragmentPath
      ran_json as_user "$program" --json schedule uninstall
      ran user_systemctl unmask "$@" pitboard-renew.timer
    } | facts mask install folder timer uninstall unmask | row "masked_$mask"
  done

  # A start systemd refuses: the timer's own drop-in refuses a manual start. First where
  # nothing was installed, then over an installed schedule.
  refuse_start
  {
    ran_json as_user "$program" --json schedule install
    ran units_folder "$user_home/.config/systemd/user"
    ran user_systemctl is-enabled pitboard-renew.timer pitboard-renew.service
  } | facts install folder enabled | row refused_start
  allow_start
  ran_json as_user "$program" --json schedule install | row refused_over_installed_first
  before=$(unit_sums || true)
  refuse_start
  {
    ran_json as_user "$program" --json schedule install
    jq -n --arg before "$before" --arg after "$(unit_sums || true)" '$before == $after'
    ran units_folder "$user_home/.config/systemd/user"
    ran user_systemctl is-enabled pitboard-renew.timer pitboard-renew.service
    ran user_systemctl is-active pitboard-renew.timer
  } | facts install units_unchanged folder enabled active | row refused_over_installed
  allow_start
  ran_json as_user "$program" --json uninstall --yes | row last_uninstall
}

# The Pitboard directory another computer wrote: `pitboard adopt` takes it over with one save.
plant() {
  mkdir -p "$1"
  printf '{"schema":9,"machine":"another computer","accounts":[],"vault":{"form":"files"}}\n' \
    >"$1/state.json"
  chmod 0600 "$1/state.json"
}

# watch_save <pitboard directory> <watched folder...>: inotifywait on each folder while
# pitboard, with every home fresh and on a bus of its own, takes over that directory.
watch_save() {
  written=$1
  shift
  plant "$written"
  n=0
  pids=''
  for watched in "$@"; do
    n=$((n + 1))
    mkdir -p "$watched"
    : >"$scratch/events.$n"
    : >"$scratch/ready.$n"
    inotifywait -m --format '%e %f' -o "$scratch/events.$n" "$watched" 2>"$scratch/ready.$n" &
    pids="$pids $!"
    waits 10 grep -q 'Watches established' "$scratch/ready.$n" || true
  done
  fresh=$(mktemp -d "$scratch/homes.XXXXXX")
  for folder in home config data cache state runtime claude codex; do
    mkdir "$fresh/$folder"
  done
  chmod 0700 "$fresh/runtime"
  ran_json env -i PATH=/usr/bin:/bin LANG=C.UTF-8 HOME="$fresh/home" PITBOARD_HOME="$written" \
    CLAUDE_CONFIG_DIR="$fresh/claude" CODEX_HOME="$fresh/codex" XDG_CONFIG_HOME="$fresh/config" \
    XDG_DATA_HOME="$fresh/data" XDG_CACHE_HOME="$fresh/cache" XDG_STATE_HOME="$fresh/state" \
    XDG_RUNTIME_DIR="$fresh/runtime" dbus-run-session -- "$pitboard" --json adopt >"$scratch/adopt.json"
  sleep 2
  # shellcheck disable=SC2086 # a list of process ids
  kill $pids 2>/dev/null || true
  wait
  n=0
  for watched in "$@"; do
    n=$((n + 1))
    jq -n --arg folder "$watched" --rawfile events "$scratch/events.$n" \
      --rawfile ready "$scratch/ready.$n" \
      '{folder: $folder, events: ($events | split("\n") | map(select(. != ""))), said: $ready}'
  done | jq -s --slurpfile adopt "$scratch/adopt.json" '{adopt: $adopt[0], watched: .}'
}

file_system() {
  jq -n --arg statfs "$(stat -f -c %T "$1")" --arg mount "$(findmnt -n -o FSTYPE,SOURCE --target "$1")" \
    '{statfs: $statfs, mount: $mount}'
}

inotify_done() {
  set +e
  for pid in ${pids:-}; do kill "$pid" 2>/dev/null; done
  for mounted in "$base/nfs" "$base/btrfs" "$base/tmpfs"; do
    mountpoint -q "$mounted" && sudo -n umount "$mounted"
  done
  [ -n "${exported:-}" ] && sudo -n exportfs -u "127.0.0.1:$base/export"
  [ "${nfs_was_active:-true}" = false ] && sudo -n systemctl stop nfs-server
  remove_added
  rm -rf --one-file-system "$scratch"
}

# mounted <key> <folder> <made>: the facts in <made> of making a file system, then, where one was
# mounted at <folder>, what it is and what inotify said of a save there.
mounted() {
  if mountpoint -q "$2"; then
    { file_system "$2"; watch_save "$2/pitboard" "$2/pitboard"; } | facts file_system save |
      jq -s '.[0] + .[1]' "$3" -
  else
    cat "$3"
  fi | row "$1"
}

inotify() {
  on_a_runner_only
  [ -n "$pitboard" ] || usage
  pitboard=$(absolute "$pitboard")
  start_report "$RUNNER_TEMP"
  cd /
  added=''
  trap inotify_done EXIT
  trap 'exit 130' INT TERM
  base=$scratch/fs
  made=$scratch/made.json
  mkdir "$base"
  ran add_packages dbus-daemon inotify-tools btrfs-progs nfs-kernel-server >"$scratch/packages.json"
  row packages <"$scratch/packages.json"
  ran "$pitboard" --version | row version

  mkdir "$base/disk"
  { file_system "$base/disk"; watch_save "$base/disk/pitboard" "$base/disk/pitboard"; } |
    facts file_system save | row runner_disk

  mkdir "$base/tmpfs"
  ran sudo -n mount -t tmpfs -o "size=64m,mode=0755,uid=$(id -u),gid=$(id -g)" pitboard-facts \
    "$base/tmpfs" | facts mount >"$made"
  mounted tmpfs "$base/tmpfs" "$made"

  mkdir "$base/btrfs"
  truncate -s 512M "$base/btrfs.img"
  {
    ran mkfs.btrfs -q "$base/btrfs.img"
    ran sudo -n mount -o loop "$base/btrfs.img" "$base/btrfs"
    ran sudo -n chown "$(id -u):$(id -g)" "$base/btrfs"
  } | facts mkfs mount chown >"$made"
  mounted btrfs "$base/btrfs" "$made"

  # One export of the runner's disk, mounted back over the loopback: a save through the mount
  # is this machine's, and a save in the exported folder stands for another machine's.
  mkdir "$base/export" "$base/nfs"
  if systemctl is-active --quiet nfs-server 2>/dev/null; then nfs_was_active=true; else nfs_was_active=false; fi
  exported=true
  {
    ran sudo -n systemctl start nfs-server
    ran sudo -n exportfs -o rw,sync,no_subtree_check,insecure "127.0.0.1:$base/export"
    ran sudo -n mount -t nfs -o vers=4.2 "127.0.0.1:$base/export" "$base/nfs"
  } | facts server export mount >"$made"
  if mountpoint -q "$base/nfs"; then
    {
      file_system "$base/nfs"
      watch_save "$base/nfs/here" "$base/nfs/here" "$base/export/here"
      watch_save "$base/export/elsewhere" "$base/nfs/elsewhere" "$base/export/elsewhere"
    } | facts file_system saved_through_the_mount saved_in_the_export |
      jq -s '.[0] + .[1]' "$made" -
  else
    cat "$made"
  fi | row nfs
}

# chain <program> <PATH>: each link from what the shell finds on PATH to the file at its end,
# and what that file is.
chain() {
  found=$(PATH=$2 && command -v "$1") || {
    jq -n --arg program "$1" '{program: $program, found: null}'
    return
  }
  at=$found
  links=$scratch/links
  : >"$links"
  while [ -L "$at" ]; do
    to=$(readlink "$at")
    jq -n -c --arg link "$at" --arg to "$to" '{link: $link, to: $to}' >>"$links"
    case $to in
      /*) at=$to ;;
      *) at=$(dirname "$at")/$to ;;
    esac
  done
  jq -n --arg program "$1" --arg found "$found" --slurpfile links "$links" --arg last "$at" \
    --arg real "$(realpath "$at")" --arg kind "$(file -b "$at")" \
    --arg first "$(head -n 1 "$at" | tr -d '\0' | cut -c 1-200)" \
    --arg interpreter "$(readelf -lW "$at" 2>/dev/null | sed -n 's/.*Requesting program interpreter: \(.*\)]$/\1/p')" '
    {program: $program, found: $found, links: $links, last: $last, real: $real, kind: $kind,
     interpreter: (if $interpreter == "" then null else $interpreter end),
     first_line: (if ($first | startswith("#!")) then $first else null end)}'
}

# The script a stand-in plays in the place of a tool's program: it writes its process id,
# records how it was started, and waits for a line.
stand_in_script() {
  jq -n -c --arg pid "$scratch/started.pid" --arg record "$scratch/started.json" \
    '{Plays: {args: null, steps: [{WritesItsPid: $pid}, {Records: {at: $record, variables: []}},
      "WaitsForALine"]}}'
}

# stand_in_at <file>: the stand-in in the place of <file>, with its script beside it.
stand_in_at() {
  cp "$stand_in" "$1.part"
  chmod 0755 "$1.part"
  mv "$1.part" "$1"
  stand_in_script >"$1.stand-in.json"
}

# started <command...>: starts it on a line it waits for, and reads /proc for the process the
# stand-in says it is, before letting it go.
started() {
  rm -f "$scratch/started.pid" "$scratch/started.json" "$scratch/line"
  mkfifo "$scratch/line"
  "$@" <"$scratch/line" >"$scratch/started.out" 2>"$scratch/started.err" &
  launched=$!
  exec 3>"$scratch/line"
  pid='' comm='' exe='' cmdline=''
  if waits 20 test -s "$scratch/started.pid"; then
    pid=$(cat "$scratch/started.pid")
    comm=$(cat "/proc/$pid/comm" 2>/dev/null) || true
    exe=$(readlink "/proc/$pid/exe" 2>/dev/null) || true
    cmdline=$(tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null) || true
  fi
  # A program that never started reads no line, and the write then fails rather than ends this.
  trap '' PIPE
  echo >&3 2>/dev/null || true
  exec 3>&-
  trap - PIPE
  if wait "$launched"; then code=0; else code=$?; fi
  touch "$scratch/started.json"
  jq -n --arg pid "$pid" --arg comm "$comm" --arg exe "$exe" --arg cmdline "$cmdline" \
    --argjson exit "$code" --slurpfile record "$scratch/started.json" \
    --rawfile err "$scratch/started.err" '
    {seen: ($pid != ""), comm: $comm, exe: $exe, cmdline: $cmdline, exit: $exit,
     record: ($record[0] // null), err: $err}'
}

# npm with a home and a cache in the scratch folder, so nothing of it outlives the part.
npm_run() {
  env -i PATH="$npm_path" HOME="$scratch/home" npm_config_cache="$scratch/cache" \
    npm_config_update_notifier=false npm "$@"
}

installs_done() {
  remove_added
  rm -rf "$scratch"
}

installs() {
  on_a_runner_only
  [ -n "$stand_in" ] && [ -n "$claude_code" ] && [ -n "$codex" ] || usage
  for version in "$claude_code" "$codex"; do
    if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
      echo "a version is three numbers, such as 2.1.312, not $version" >&2
      exit 2
    fi
  done
  stand_in=$(absolute "$stand_in")
  start_report "$RUNNER_TEMP"
  cd /
  added=''
  trap installs_done EXIT
  trap 'exit 130' INT TERM
  prefix=$scratch/prefix
  mkdir "$prefix" "$scratch/home" "$scratch/cache" "$scratch/interpreter"
  system_path=/usr/local/bin:/usr/bin:/bin
  if ! command -v npm >/dev/null; then
    ran add_packages nodejs npm >"$scratch/packages.json"
    row packages <"$scratch/packages.json"
  fi
  npm_path=$(dirname "$(command -v npm)"):$system_path
  { ran node --version; ran npm_run --version; } | facts node npm | row npm
  # The global layout, in a prefix of its own: npm runs Claude Code's postinstall, which puts
  # its native program in place of bin/claude.exe; Codex's package has no script.
  ran npm_run install --global --prefix "$prefix" --no-audit --no-fund \
    "@anthropic-ai/claude-code@$claude_code" | row claude_code_install
  ran npm_run install --global --prefix "$prefix" --no-audit --no-fund "@openai/codex@$codex" |
    row codex_install

  path=$prefix/bin:$system_path
  chain claude "$path" >"$scratch/claude.json"
  row claude_code_chain <"$scratch/claude.json"
  chain codex "$path" >"$scratch/codex.json"
  row codex_chain <"$scratch/codex.json"
  native=$(find "$prefix/lib/node_modules/@openai" -path '*/vendor/*/bin/codex' -type f | head -n 1)
  if [ -n "$native" ]; then
    jq -n --arg path "$native" --arg kind "$(file -b "$native")" '{path: $path, kind: $kind}'
  else
    jq -n 'null'
  fi | row codex_native

  # Claude Code: the shell runs `claude`, which leads to the native program.
  claude_last=$(jq -r '.last // empty' "$scratch/claude.json")
  if [ -n "$claude_last" ]; then
    stand_in_at "$claude_last"
    started env -i PATH="$path" HOME="$scratch/home" sh -c 'exec claude' | row claude_code_started
  fi

  # Codex: the shell runs `codex`, a script for `node` through env, which then starts the native
  # program by its path. A stand-in plays node, so no script of Codex's runs.
  codex_last=$(jq -r '.last // empty' "$scratch/codex.json")
  if [ -n "$codex_last" ]; then
    cp "$stand_in" "$scratch/interpreter/node"
    chmod 0755 "$scratch/interpreter/node"
    printf '"Interprets"\n' >"$scratch/interpreter/node.stand-in.json"
    { head -n 1 "$codex_last"; stand_in_script; } >"$scratch/codex.js"
    # In place, so the file keeps the mode npm gave it.
    cat "$scratch/codex.js" >"$codex_last"
    started env -i PATH="$scratch/interpreter:$path" HOME="$scratch/home" sh -c 'exec codex' |
      row codex_started
  fi
  if [ -n "$native" ]; then
    stand_in_at "$native"
    started env -i PATH="$path" HOME="$scratch/home" "$native" | row codex_native_started
  fi
}

display_setup() {
  if [ "$(id -u)" -ne 0 ] || started_by_systemd; then
    echo "linux-runner-facts.sh display-setup runs as root in a container: it installs packages and makes a user" >&2
    exit 1
  fi
  [ -n "$out" ] || usage
  export DEBIAN_FRONTEND=noninteractive
  # apt reads the snapshot over https, which the image cannot check before it has the
  # certificates, taken from the archive the image names.
  apt-get install --update -y --no-install-recommends ca-certificates
  apt-get install --update -y --no-install-recommends --snapshot "$snapshot" adwaita-icon-theme \
    at-spi2-core dbus-daemon file fonts-adwaita gir1.2-atspi-2.0 jq libegl-mesa0 libgl1-mesa-dri \
    libglib2.0-bin libgtk-4-bin mesa-utils mutter python3-gi
  # dbus-daemon refuses a uid the password database lacks.
  id -u "$throwaway" >/dev/null 2>&1 ||
    useradd --user-group --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin \
      "$throwaway"
  install -d -o "$throwaway" -g "$throwaway" "$(dirname "$(absolute "$out")")"
}

display_done() {
  rm -rf "$scratch"
}

# The window the screenshots and the popover's DoAction use: GTK's own widgets alone, since
# gtk4-builder-tool loads no libadwaita.
write_ui() {
  cat >"$1" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkWindow" id="window">
    <property name="title">Linux facts</property>
    <property name="default-width">360</property>
    <property name="child">
      <object class="GtkBox">
        <property name="orientation">vertical</property>
        <property name="spacing">12</property>
        <property name="margin-top">18</property>
        <property name="margin-bottom">18</property>
        <property name="margin-start">18</property>
        <property name="margin-end">18</property>
        <child>
          <object class="GtkLabel">
            <property name="label">Five-hour limit, 62% used</property>
            <property name="xalign">0</property>
          </object>
        </child>
        <child>
          <object class="GtkLevelBar">
            <property name="value">0.62</property>
          </object>
        </child>
        <child>
          <object class="GtkCheckButton">
            <property name="label">Switch before a limit</property>
            <property name="active">True</property>
          </object>
        </child>
        <child>
          <object class="GtkMenuButton" id="menu">
            <property name="label">Menu</property>
            <property name="popover">
              <object class="GtkPopover">
                <property name="child">
                  <object class="GtkLabel">
                    <property name="label">Inside the popover</property>
                  </object>
                </property>
              </object>
            </property>
          </object>
        </child>
      </object>
    </property>
  </object>
</interface>
EOF
}

# What the display part asks over D-Bus and AT-SPI, through GLib's and libatspi's own bindings.
write_client() {
  cat >"$1" <<'EOF'
import json
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib


def display_config():
    return Gio.DBusProxy.new_for_bus_sync(
        Gio.BusType.SESSION, Gio.DBusProxyFlags.NONE, None,
        "org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig", None)


def state(proxy):
    serial, monitors, logical, _ = proxy.call_sync(
        "GetCurrentState", None, Gio.DBusCallFlags.NONE, -1, None).unpack()
    return serial, monitors, logical


def scale(want):
    proxy = display_config()
    serial, monitors, _ = state(proxy)
    (connector, _, _, _), modes, _ = monitors[0]
    mode = next(m for m in modes if m[6].get("is-current"))
    applied = None
    if want != 1.0:
        config = [(0, 0, want, 0, True, [(connector, mode[0], {})])]
        try:
            proxy.call_sync(
                "ApplyMonitorsConfig",
                GLib.Variant("(uua(iiduba(ssa{sv}))a{sv})", (serial, 1, config, {})),
                Gio.DBusCallFlags.NONE, -1, None)
            applied = True
        except GLib.Error as error:
            applied = error.message
    _, _, logical = state(proxy)
    return {"connector": connector, "mode": mode[0], "supported": list(mode[5]),
            "applied": applied, "scales": [m[2] for m in logical]}


def nodes(root):
    stack = [root]
    while stack:
        node = stack.pop()
        yield node
        for i in reversed(range(node.get_child_count())):
            child = node.get_child_at_index(i)
            if child is not None:
                stack.append(child)


def showing(node):
    return node.get_state_set().contains(Atspi.StateType.SHOWING)


def popover(label, inside):
    desktop = Atspi.get_desktop(0)
    found = {"apps": [], "named": [], "button": None, "actions": [], "did": None,
             "seconds": None, "inside": None}
    app = None
    deadline = time.monotonic() + 20
    while app is None and time.monotonic() < deadline:
        apps = [desktop.get_child_at_index(i) for i in range(desktop.get_child_count())]
        found["apps"] = [a.get_name() for a in apps if a is not None]
        app = next((a for a in apps if a is not None and a.get_child_count() > 0), None)
        time.sleep(0.2)
    if app is None:
        return found
    button = None
    while button is None and time.monotonic() < deadline:
        named = [n for n in nodes(app) if n.get_name() == label]
        found["named"] = [{"role": n.get_role_name(),
                           "actions": n.get_n_actions() if n.get_action_iface() else None}
                          for n in named]
        button = next((n for n in named if n.get_action_iface() and n.get_n_actions() > 0), None)
        time.sleep(0.2)
    if button is None:
        return found
    found["button"] = button.get_role_name()
    # Python reaches Action's get_name only by its older name: Accessible's get_name hides it.
    found["actions"] = [button.get_action_name(i) for i in range(button.get_n_actions())]
    started = time.monotonic()
    found["did"] = button.do_action(0)
    while time.monotonic() < started + 10:
        shown = next((n for n in nodes(app) if n.get_name() == inside and showing(n)), None)
        if shown is not None:
            parent = shown.get_parent()
            found["inside"] = {"role": shown.get_role_name(),
                               "parent": parent.get_role_name() if parent else None}
            found["seconds"] = round(time.monotonic() - started, 2)
            break
        time.sleep(0.1)
    return found


if sys.argv[1] == "scale":
    print(json.dumps(scale(float(sys.argv[2]))))
else:
    print(json.dumps(popover(sys.argv[2], sys.argv[3])))
EOF
}

display() {
  if [ "$(id -u)" -eq 0 ]; then
    echo "linux-runner-facts.sh display runs as a user, as the Linux app's tests do" >&2
    exit 1
  fi
  if started_by_systemd; then
    echo "linux-runner-facts.sh display runs only in a container that display-setup made ready" >&2
    exit 1
  fi
  start_report "${TMPDIR:-/tmp}"
  trap display_done EXIT
  trap 'exit 130' INT TERM
  fresh=$scratch/fresh
  hiding "$fresh" '<fresh>'
  for folder in home config data cache state runtime tmp work; do
    mkdir -p "$fresh/$folder"
  done
  chmod 0700 "$fresh/runtime"
  write_ui "$fresh/work/window.ui"
  write_client "$fresh/work/client.py"
  dpkg-query -W at-spi2-core libgtk-4-1 libgl1-mesa-dri mutter python3-gi |
    jq -R -s --arg snapshot "$snapshot" '{snapshot: $snapshot, packages: (split("\n")
      | map(select(. != "") | split("\t") | {key: .[0], value: .[1]}) | from_entries)}' |
    row versions
  if env -i PATH=/usr/bin:/bin LANG=C.UTF-8 HOME="$fresh/home" XDG_CONFIG_HOME="$fresh/config" \
    XDG_DATA_HOME="$fresh/data" XDG_CACHE_HOME="$fresh/cache" XDG_STATE_HOME="$fresh/state" \
    XDG_RUNTIME_DIR="$fresh/runtime" TMPDIR="$fresh/tmp" GSETTINGS_BACKEND=memory \
    LINUX_FACTS_RAW="$raw" LINUX_FACTS_HIDE="$hide" LINUX_FACTS_OUT="$out" \
    LINUX_FACTS_WORK="$fresh/work" dbus-run-session -- sh "$self" display-session \
    >"$fresh/work/session.out" 2>"$fresh/work/session.err"; then code=0; else code=$?; fi
  # The private bus's daemon says there which services it started for the session.
  jq -n --argjson exit "$code" --rawfile out "$fresh/work/session.out" \
    --rawfile err "$fresh/work/session.err" '{exit: $exit, out: $out, err: $err}' | row session
}

# Inside display's private bus: mutter twice, a 1280x800 monitor at scale 1, then a 2560x1600
# one at scale 2, which mutter does not offer at 1280x800.
display_session() {
  inner
  for scale in 1 2; do
    if [ "$scale" = 1 ]; then size=1280x800; else size=2560x1600; fi
    if timeout 600 mutter --headless --wayland --no-x11 --virtual-monitor "$size" -- \
      sh "$self" display-mutter "$scale" >"$work/mutter-$scale.out" 2>"$work/mutter-$scale.err"
    then code=0; else code=$?; fi
    jq -n --argjson exit "$code" --rawfile err "$work/mutter-$scale.err" '{exit: $exit, err: $err}' |
      row "mutter_$scale"
  done
}

shot() {
  renderer=$1
  png=$2
  if [ "$renderer" = default ]; then set -- env -u GSK_RENDERER; else set -- env GSK_RENDERER="$renderer"; fi
  if timeout 60 "$@" gtk4-builder-tool screenshot --force "$work/window.ui" "$png" \
    >"$work/shot.out" 2>"$work/shot.err"; then code=0; else code=$?; fi
  if [ -f "$png" ]; then
    sum=$(sha256sum "$png" | cut -d ' ' -f 1)
    kind=$(file -b "$png")
  else
    sum='' kind=''
  fi
  jq -n --argjson exit "$code" --arg sha256 "$sum" --arg kind "$kind" --rawfile err "$work/shot.err" \
    '{exit: $exit, sha256: $sha256, kind: $kind, err: $err}'
}

# Inside mutter, which gives its command WAYLAND_DISPLAY.
display_mutter() {
  inner
  scale=$1
  ran_json python3 -I "$work/client.py" scale "$scale" | row "monitor_$scale"
  if [ "$scale" = 1 ]; then
    {
      ran env GSK_DEBUG=renderer GDK_DEBUG=opengl gtk4-builder-tool screenshot --force \
        "$work/window.ui" "$work/probe.png"
      ran env GSK_RENDERER=opengl GDK_DEBUG=opengl gtk4-builder-tool screenshot --force \
        "$work/window.ui" "$work/probe.png"
      ran eglinfo -B -p wayland
    } | facts chosen opengl eglinfo | row renderer
  fi
  for renderer in default opengl cairo; do
    for run in 1 2; do
      shot "$renderer" "$work/shot-$scale-$renderer-$run.png"
    done | jq -s '{first: .[0], second: .[1], same: (.[0].sha256 != "" and .[0].sha256 == .[1].sha256)}'
  done | jq -n '[inputs] | {default: .[0], opengl: .[1], cairo: .[2]}' | row "screenshots_$scale"
  if [ "$scale" = 1 ]; then
    GSK_RENDERER=opengl gtk4-builder-tool preview "$work/window.ui" \
      >"$work/preview.out" 2>"$work/preview.err" &
    preview=$!
    ran_json timeout 60 python3 -I "$work/client.py" popover Menu "Inside the popover" |
      row popover
    kill "$preview" 2>/dev/null || true
    wait "$preview" 2>/dev/null || true
  fi
}

# The internal parts write the report the display part started.
inner() {
  raw=$LINUX_FACTS_RAW
  hide=$LINUX_FACTS_HIDE
  out=$LINUX_FACTS_OUT
  work=$LINUX_FACTS_WORK
  scratch=$work
}

case $part in
  user-manager) user_manager ;;
  inotify) inotify ;;
  installs) installs ;;
  display-setup) display_setup ;;
  display) display ;;
  display-session) display_session ;;
  display-mutter) display_mutter "$1" ;;
  *) usage ;;
esac
