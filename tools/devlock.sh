#!/usr/bin/env bash
# Check bench devices and the Pixel Blaze oracle in and out for development.
#
#   tools/devlock.sh checkout <device> [-m "why"] [--ttl HOURS] [--force]
#   tools/devlock.sh checkin  <device> [--force]
#   tools/devlock.sh checkin  --all            # everything this session holds
#   tools/devlock.sh status   [device]         # table of every device, or one
#   tools/devlock.sh check    <device>         # exit 0 = free or mine, 1 = foreign
#   tools/devlock.sh devices                   # the registry (name, aliases, ip)
#   tools/devlock.sh selftest                  # exercise the lock in a temp dir
#
# Every bench device is ONE shared unit and the Pixel Blaze oracle is ONE
# device (CLAUDE.md "Environment boundaries"). Two sessions driving the
# same unit at once has already cost a reverted OTA, a black panel and a
# wrong brightness, so a device is checked out by whoever drives it and
# checked back in when that task is done. All development is on this one
# machine, so the lock is a plain file per device under
#
#   $LUXEL_DEVLOCK_DIR, default ${XDG_STATE_HOME:-~/.local/state}/luxel/devlocks
#
# (outside every worktree and every scratchpad — the point is that every
# session, every worktree and Jeremy's own shell see the same directory).
#
# A lock records who holds it: the Claude session id and its process id
# when run under Claude Code, otherwise user@host; plus the worktree, the
# branch, the reason and the time. Creation is atomic (hard link onto the
# final name), so two concurrent checkouts cannot both win.
#
# A lock is STALE when its holder's process is dead or its TTL has passed.
# `checkout` takes a stale lock over and says so; a LIVE foreign lock is a
# refusal — stop and ask Jeremy whose the device is, never --force past it
# on your own. A human checkout (no Claude pid) has no TTL unless given.
#
# Scripts that drive a device call `check` (deploy.sh, ota-push.sh, the
# oracle harness): a foreign live lock aborts them, an unlocked device
# only prints a hint. DEVLOCK_SKIP=1 bypasses that guard for Jeremy's own
# shell; agents never set it.
#
# Lock file format: `key=value` lines — device, ip, holder, session, pid,
# user, host, worktree, branch, reason, since (epoch), since_iso, ttl_s.
set -euo pipefail

# ------------------------------------------------------------ registry
# name|aliases (comma-separated)|ip|note. The name is the lock file's name;
# any alias or the IP resolves to it. Add a line when a device joins the
# bench — checkout refuses names it does not know so a typo cannot create a
# lock that protects nothing.
REGISTRY='
dev-unit|dev,pbv3,luxel-4ae0d4|192.168.0.205|Pixel Blaze v3 dev unit (serial flashing is Jeremy-only)
athom|athom-rig,luxel-797e10|192.168.0.183|Athom rig (Jeremy live setup: 144 px, brightness is his)
seengreat-1|seengreat,panel,s3,luxel-f6b0a8|192.168.0.238|Seengreat HUB75 S3 unit 1 (/dev/ttyACM0)
seengreat-2|unit2,luxel-ea6f70|192.168.0.142|Seengreat HUB75 S3 unit 2 (/dev/ttyACM1)
oracle|pb,pixelblaze,pb-oracle|192.168.0.140|the real Pixel Blaze oracle (black box, websocket only)
'

LOCK_DIR="${LUXEL_DEVLOCK_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/luxel/devlocks}"
DEFAULT_TTL_H="${LUXEL_DEVLOCK_TTL_H:-8}"   # agent locks expire after this; 0 = never

die() { echo "devlock: $*" >&2; exit 2; }

usage() { sed -n '2,12p' "$0"; exit "${1:-2}"; }

# resolve <name-or-alias-or-ip> → sets DEV_NAME DEV_IP DEV_NOTE
resolve() {
  local q="${1,,}" line name aliases ip note
  while IFS='|' read -r name aliases ip note; do
    [ -n "$name" ] || continue
    if [ "$q" = "$name" ] || [ "$q" = "$ip" ] || [[ ",$aliases," == *",$q,"* ]]; then
      DEV_NAME=$name DEV_IP=$ip DEV_NOTE=$note
      return 0
    fi
  done <<<"$REGISTRY"
  echo "devlock: unknown device '$1' — known devices:" >&2
  list_devices >&2
  echo "devlock: add a REGISTRY line in tools/devlock.sh for a new unit" >&2
  return 1
}

list_devices() {
  local name aliases ip note
  printf '  %-12s %-15s %-28s %s\n' NAME IP ALIASES NOTE
  while IFS='|' read -r name aliases ip note; do
    [ -n "$name" ] || continue
    printf '  %-12s %-15s %-28s %s\n' "$name" "$ip" "$aliases" "$note"
  done <<<"$REGISTRY"
}

# Identity of the caller. Under Claude Code the session id + pid identify
# one session across every worktree it touches; a human gets user@host.
me_session="${CLAUDE_CODE_SESSION_ID:-}"
me_pid="${CLAUDE_PID:-}"
me_user="${USER:-$(id -un)}"
me_host="$(hostname 2>/dev/null || echo localhost)"
if [ -n "$me_session" ]; then
  me_holder="claude:${me_session}"
else
  me_holder="${LUXEL_DEVLOCK_HOLDER:-${me_user}@${me_host}}"
fi
me_worktree="$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")"
me_branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo -)"

lock_path() { echo "$LOCK_DIR/$1.lock"; }

# read_lock <file> → sets L_* from the key=value lines (missing keys = "")
read_lock() {
  local k v
  L_device= L_ip= L_holder= L_session= L_pid= L_user= L_host= L_worktree= L_branch= L_reason= L_since= L_since_iso= L_ttl_s=
  while IFS='=' read -r k v; do
    case "$k" in
      device|ip|holder|session|pid|user|host|worktree|branch|reason|since|since_iso|ttl_s)
        printf -v "L_$k" '%s' "$v" ;;
    esac
  done <"$1"
}

# why_stale → prints a reason and returns 0 when the lock just read is stale
why_stale() {
  local now; now=$(date +%s)
  if [ -n "$L_pid" ] && ! kill -0 "$L_pid" 2>/dev/null; then
    echo "holder pid $L_pid is gone"; return 0
  fi
  if [ -n "$L_ttl_s" ] && [ "$L_ttl_s" != 0 ] && [ -n "$L_since" ] \
     && [ $((now - L_since)) -gt "$L_ttl_s" ]; then
    echo "ttl of $((L_ttl_s / 3600)) h passed"; return 0
  fi
  return 1
}

is_mine() { [ "$L_holder" = "$me_holder" ]; }

age_str() {
  local s=$(( $(date +%s) - ${1:-0} ))
  if [ "$s" -lt 60 ]; then echo "${s}s"
  elif [ "$s" -lt 3600 ]; then echo "$((s / 60))m"
  else echo "$((s / 3600))h$(( (s % 3600) / 60 ))m"; fi
}

describe_lock() {   # one line about the lock just read
  local who="$L_holder"
  [ -n "$L_pid" ] && who="$who pid $L_pid"
  echo "held by $who since $L_since_iso ($(age_str "$L_since") ago) on $L_branch — ${L_reason:-no reason given}"
}

write_lock() {   # write_lock <device> <reason> <ttl_s>  (atomic; fails if exists)
  local f tmp
  f=$(lock_path "$1")
  mkdir -p "$LOCK_DIR"
  tmp=$(mktemp "$LOCK_DIR/.$1.XXXXXX")
  {
    echo "device=$1"
    echo "ip=$DEV_IP"
    echo "holder=$me_holder"
    echo "session=$me_session"
    echo "pid=$me_pid"
    echo "user=$me_user"
    echo "host=$me_host"
    echo "worktree=$me_worktree"
    echo "branch=$me_branch"
    echo "reason=$2"
    echo "since=$(date +%s)"
    echo "since_iso=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "ttl_s=$3"
  } >"$tmp"
  if ln "$tmp" "$f" 2>/dev/null; then rm -f "$tmp"; return 0; fi
  rm -f "$tmp"; return 1
}

cmd_checkout() {
  local dev= reason= ttl_h= force=0
  while [ $# -gt 0 ]; do
    case "$1" in
      -m|--reason) reason="${2:?-m needs a reason}"; shift 2 ;;
      --ttl) ttl_h="${2:?--ttl needs hours}"; shift 2 ;;
      --force) force=1; shift ;;
      -*) die "unknown checkout flag $1" ;;
      *) [ -z "$dev" ] || die "one device per checkout"; dev=$1; shift ;;
    esac
  done
  [ -n "$dev" ] || usage
  resolve "$dev" || exit 2
  if [ -z "$ttl_h" ]; then
    # agents expire; a human's checkout stands until checked in
    if [ -n "$me_pid" ]; then ttl_h=$DEFAULT_TTL_H; else ttl_h=0; fi
  fi
  local ttl_s=$(( ttl_h * 3600 ))
  [ -n "$reason" ] || [ "$force" = 0 ] || die "--force needs -m \"why\" (it is a record of breaking someone's lock)"
  local f; f=$(lock_path "$DEV_NAME")
  local tries=0
  while ! write_lock "$DEV_NAME" "$reason" "$ttl_s"; do
    [ -f "$f" ] || { tries=$((tries + 1)); [ "$tries" -lt 5 ] || die "cannot create $f"; continue; }
    read_lock "$f"
    if is_mine; then
      # re-checkout by the same session: refresh the reason/ttl, keep `since`
      local keep_since=$L_since keep_iso=$L_since_iso
      rm -f "$f"
      write_lock "$DEV_NAME" "${reason:-$L_reason}" "$ttl_s" || die "lost a race re-locking $DEV_NAME"
      sed -i "s/^since=.*/since=$keep_since/; s/^since_iso=.*/since_iso=$keep_iso/" "$f"
      echo "devlock: $DEV_NAME already yours — refreshed (${reason:-$L_reason})"
      return 0
    fi
    local stale
    if stale=$(why_stale); then
      echo "devlock: $DEV_NAME lock was STALE ($stale) — taking it over. It was: $(describe_lock)"
      rm -f "$f"; continue
    fi
    if [ "$force" = 1 ]; then
      echo "devlock: FORCING $DEV_NAME away from a LIVE holder: $(describe_lock)" >&2
      rm -f "$f"; continue
    fi
    echo "devlock: $DEV_NAME ($DEV_IP) is BUSY — $(describe_lock)" >&2
    echo "devlock: a live lock is another session's work: STOP and ask Jeremy whose the device is (CLAUDE.md). Do not --force it yourself." >&2
    exit 1
  done
  echo "devlock: checked out $DEV_NAME ($DEV_IP) as $me_holder${reason:+ — $reason}$( [ "$ttl_s" = 0 ] && echo ' (no expiry)' || echo " (expires in ${ttl_h} h)")"
}

cmd_checkin() {
  local dev= force=0 all=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --force) force=1; shift ;;
      --all) all=1; shift ;;
      -*) die "unknown checkin flag $1" ;;
      *) dev=$1; shift ;;
    esac
  done
  if [ "$all" = 1 ]; then
    local f n=0
    for f in "$LOCK_DIR"/*.lock; do
      [ -f "$f" ] || continue
      read_lock "$f"
      if is_mine; then rm -f "$f"; echo "devlock: checked in $L_device"; n=$((n + 1)); fi
    done
    [ "$n" -gt 0 ] || echo "devlock: nothing of yours was checked out"
    return 0
  fi
  [ -n "$dev" ] || usage
  resolve "$dev" || exit 2
  local f; f=$(lock_path "$DEV_NAME")
  [ -f "$f" ] || { echo "devlock: $DEV_NAME was not checked out"; return 0; }
  read_lock "$f"
  if ! is_mine && [ "$force" = 0 ]; then
    if why_stale >/dev/null; then
      echo "devlock: clearing a STALE $DEV_NAME lock: $(describe_lock)"
    else
      echo "devlock: $DEV_NAME is not yours — $(describe_lock)" >&2
      echo "devlock: checkin --force only if Jeremy said so" >&2
      exit 1
    fi
  fi
  rm -f "$f"
  echo "devlock: checked in $DEV_NAME"
}

cmd_status() {
  local only=""
  if [ $# -gt 0 ]; then resolve "$1" || exit 2; only=$DEV_NAME; fi
  local name aliases ip note f state
  printf '%-12s %-15s %s\n' DEVICE IP STATE
  while IFS='|' read -r name aliases ip note; do
    [ -n "$name" ] || continue
    [ -z "$only" ] || [ "$name" = "$only" ] || continue
    f=$(lock_path "$name")
    if [ -f "$f" ]; then
      read_lock "$f"
      state="BUSY  $(describe_lock)"
      is_mine && state="MINE  $(describe_lock)"
      local stale; stale=$(why_stale) && state="STALE ($stale) $(describe_lock)"
    else
      state="free"
    fi
    printf '%-12s %-15s %s\n' "$name" "$ip" "$state"
  done <<<"$REGISTRY"
  echo "(lock dir: $LOCK_DIR)"
}

# check <device>: for scripts. 0 = free or mine (free prints a hint),
# 1 = held live by someone else. DEVLOCK_SKIP=1 → always 0, silently.
cmd_check() {
  [ "${DEVLOCK_SKIP:-0}" = 1 ] && return 0
  [ $# -eq 1 ] || usage
  # A device the registry does not know (a devkit on a fresh IP) cannot be
  # locked, so it cannot be contended either — note it and let the caller go.
  if ! resolve "$1" 2>/dev/null; then
    echo "devlock: note — '$1' is not in the device registry (tools/devlock.sh), so it is not lockable; add it if it is a shared unit" >&2
    return 0
  fi
  local f; f=$(lock_path "$DEV_NAME")
  if [ ! -f "$f" ]; then
    echo "devlock: note — $DEV_NAME ($DEV_IP) is not checked out; \`tools/devlock.sh checkout $DEV_NAME -m why\` claims it for this session" >&2
    return 0
  fi
  read_lock "$f"
  is_mine && return 0
  if why_stale >/dev/null; then
    echo "devlock: note — $DEV_NAME has a STALE lock ($(why_stale)); checkout takes it over" >&2
    return 0
  fi
  echo "devlock: REFUSING — $DEV_NAME ($DEV_IP) is $(describe_lock)" >&2
  echo "devlock: stop and ask Jeremy whose the device is; DEVLOCK_SKIP=1 is for his shell, not yours" >&2
  return 1
}

cmd_selftest() {
  # `t` is global: the EXIT trap outlives this function's locals. And no
  # pipefail here — `grep -q` closing a pipe early would fail the writer.
  set +o pipefail
  t=$(mktemp -d)
  trap 'rm -rf "$t"' EXIT
  local me="$0" fails=0
  ok()  { echo "  ok   $1"; }
  bad() { echo "  FAIL $1"; fails=$((fails + 1)); }
  export LUXEL_DEVLOCK_DIR="$t/locks"
  # 1. free → checkout → mine; alias and IP resolve to the same lock
  "$me" checkout athom -m "selftest" >/dev/null && ok "checkout" || bad "checkout"
  "$me" check 192.168.0.183 2>/dev/null && ok "check by ip is mine" || bad "check by ip"
  "$me" status | grep -q '^athom .*MINE' && ok "status MINE" || bad "status MINE"
  # 2. a live foreign lock refuses checkout, check and checkin
  (
    export CLAUDE_CODE_SESSION_ID=other-session CLAUDE_PID=$$   # this shell: alive
    ! "$me" checkout athom-rig -m x >/dev/null 2>&1
  ) && ok "foreign live lock refuses checkout" || bad "foreign live refuse"
  ( export CLAUDE_CODE_SESSION_ID=other-session CLAUDE_PID=$$; ! "$me" check athom 2>/dev/null ) \
    && ok "foreign live lock fails check" || bad "foreign check"
  ( export CLAUDE_CODE_SESSION_ID=other-session CLAUDE_PID=$$; ! "$me" checkin athom >/dev/null 2>&1 ) \
    && ok "foreign live lock refuses checkin" || bad "foreign checkin"
  [ -f "$t/locks/athom.lock" ] && ok "lock survived the refusals" || bad "lock lost"
  # 3. same session re-checkout refreshes, keeps since
  local since1; since1=$(sed -n 's/^since=//p' "$t/locks/athom.lock")
  sleep 1
  "$me" checkout athom -m "again" >/dev/null && ok "re-checkout" || bad "re-checkout"
  [ "$(sed -n 's/^since=//p' "$t/locks/athom.lock")" = "$since1" ] && ok "since kept" || bad "since kept"
  grep -q '^reason=again$' "$t/locks/athom.lock" && ok "reason refreshed" || bad "reason refreshed"
  # 4. checkin, then a dead-pid lock is stale and gets taken over
  "$me" checkin athom >/dev/null && ok "checkin" || bad "checkin"
  [ ! -f "$t/locks/athom.lock" ] && ok "lock removed" || bad "lock removed"
  ( export CLAUDE_CODE_SESSION_ID=dead-session CLAUDE_PID=2147483000; "$me" checkout oracle -m dead >/dev/null )
  "$me" status | grep -q '^oracle .*STALE' && ok "dead pid reads STALE" || bad "dead pid STALE"
  "$me" checkout pb -m take 2>&1 | grep -q 'STALE' && ok "stale lock taken over" || bad "stale takeover"
  "$me" check oracle 2>/dev/null && ok "now mine" || bad "now mine"
  # 5. ttl expiry: backdate `since` past the ttl
  sed -i 's/^since=.*/since=1/' "$t/locks/oracle.lock"
  "$me" status oracle | grep -q 'STALE (ttl' && ok "ttl expiry reads STALE" || bad "ttl expiry"
  # 6. a human lock (no pid) never expires by pid and has no ttl
  ( unset CLAUDE_CODE_SESSION_ID CLAUDE_PID; "$me" checkout seengreat-2 -m eyes >/dev/null )
  grep -q '^ttl_s=0$' "$t/locks/seengreat-2.lock" && ok "human lock has no ttl" || bad "human ttl"
  ( export CLAUDE_CODE_SESSION_ID=x CLAUDE_PID=$$; ! "$me" check seengreat-2 2>/dev/null ) \
    && ok "human lock blocks an agent" || bad "human lock blocks"
  # 7. --force needs -m, then breaks a live lock; checkin --all clears only mine
  ! "$me" checkout seengreat-2 --force >/dev/null 2>&1 && ok "--force without -m refused" || bad "--force no -m"
  "$me" checkout seengreat-2 --force -m "Jeremy said" >/dev/null 2>&1 && ok "--force takes it" || bad "--force"
  ( export CLAUDE_CODE_SESSION_ID=other CLAUDE_PID=$$; "$me" checkout dev -m theirs >/dev/null )
  "$me" checkin --all | grep -c 'checked in' | grep -q '^2$' && ok "checkin --all clears mine (2)" || bad "checkin --all"
  [ -f "$t/locks/dev-unit.lock" ] && ok "checkin --all left theirs" || bad "checkin --all left theirs"
  # 8. unknown device refused; DEVLOCK_SKIP bypasses check
  ! "$me" checkout toaster >/dev/null 2>&1 && ok "unknown device refused" || bad "unknown device"
  ( export CLAUDE_CODE_SESSION_ID=x CLAUDE_PID=$$ DEVLOCK_SKIP=1; "$me" check dev-unit ) && ok "DEVLOCK_SKIP bypasses" || bad "DEVLOCK_SKIP"
  # 9. two concurrent checkouts: exactly one wins
  rm -f "$t/locks/dev-unit.lock"
  local wins=0 i
  for i in 1 2 3 4 5 6; do
    ( export CLAUDE_CODE_SESSION_ID=race-$i CLAUDE_PID=$$; "$me" checkout dev-unit -m race >/dev/null 2>&1 && echo win ) &
  done | grep -c win >"$t/wins" || true
  wait
  wins=$(cat "$t/wins")
  [ "$wins" = 1 ] && ok "race: one of six won" || bad "race: $wins winners"
  echo "devlock selftest: $fails failure(s)"
  [ "$fails" = 0 ]
}

cmd="${1:-}"; shift || true
case "$cmd" in
  checkout) cmd_checkout "$@" ;;
  checkin|release) cmd_checkin "$@" ;;
  status|ls) cmd_status "$@" ;;
  check) cmd_check "$@" ;;
  devices) list_devices ;;
  selftest) cmd_selftest ;;
  -h|--help|help|"") usage 0 ;;
  *) die "unknown command '$cmd' (try --help)" ;;
esac
