---
name: device-lock
description: Checking a bench device or the Pixel Blaze oracle out before driving it and back in when done — the local lock (tools/devlock.sh) that keeps two sessions off one shared unit. Use before the first request to any device, when a deploy script refuses with "devlock", or when something on a device looks like another session's doing.
---

Every bench device is one shared unit and there is one Pixel Blaze oracle.
All development happens on this one machine, so the lock is a plain file
per device in a directory every worktree, session and Jeremy's shell share:
`${XDG_STATE_HOME:-~/.local/state}/luxel/devlocks` (`LUXEL_DEVLOCK_DIR`
overrides). `tools/devlock.sh` is the whole mechanism; its header has the
details. Run it from the repo root; it needs only bash, so it works outside
`nix develop` too.

## Procedure

1. **Check out before the first request to a device**, with the reason:

   ```
   tools/devlock.sh checkout seengreat -m "#942 builtinbench on unit 2"
   ```

   Names, aliases or the IP all resolve (`tools/devlock.sh devices` lists
   them: `dev-unit`, `athom`, `seengreat-1`, `seengreat-2`, `oracle`). An
   agent's lock expires after 8 h (`--ttl HOURS`, 0 = never); a checkout by
   a human shell has no expiry. Re-running checkout on your own lock just
   refreshes the reason.

2. **Read `status` when it refuses.** `BUSY` with a live holder is another
   session's work — STOP and ask Jeremy whose the device is (CLAUDE.md).
   `STALE` (holder pid gone, or ttl passed) is taken over by the next
   checkout automatically and says so. `--force -m "Jeremy said"` breaks a
   live lock and is only ever done on his word.

3. **Keep doing the `pgrep -fa <ip>` check** from CLAUDE.md as well: the
   lock covers sessions that use it, `pgrep` catches a script a dead or
   older session left running.

4. **Check in when the task is done** — the point where the grant ends
   (jeremy-device-handback memory: a follow-up question is not a re-grant):

   ```
   tools/devlock.sh checkin seengreat     # or: checkin --all
   ```

   A dead session's lock goes stale on its own, so forgetting is not
   fatal, but `checkin --all` at the end of any device task is the habit.

## What enforces it

`tools/deploy.sh`, `tools/ota-push.sh` and the oracle harness
(`tools/oracle/pb.mjs` `PB.connect`) run `tools/devlock.sh check <ip>`
first: a live foreign lock aborts them, an unlocked device prints a hint
and proceeds, a device the registry does not know is a note (it cannot be
contended if it cannot be locked). `DEVLOCK_SKIP=1` bypasses the guard
and is for Jeremy's own shell, never an agent. Tools that talk HTTP
directly (`hw-bench.mjs`, `opbench.mjs`, `patbench.mjs`, …) are not
guarded — the checkout in step 1 is what covers them.

## Adding a device

A new unit is one `REGISTRY` line in `tools/devlock.sh` (name, aliases,
IP, note). `checkout` refuses unknown names so a typo cannot create a lock
that protects nothing.

## Subagents

A device brief hands the subagent a device the PARENT checked out — the
subagent runs under the same `CLAUDE_CODE_SESSION_ID`, so the lock is
already "mine" for it. Never have two subagents on one device anyway
(CLAUDE.md "Subagents").
