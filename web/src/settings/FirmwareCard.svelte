<script lang="ts">
  // Advanced › Firmware & recovery — the version, and the two actions that
  // reboot the device. Both are caps-gated (`caps.ota` / `caps.reboot`): the
  // mirror advertises neither (unless `--accept-ota` says otherwise), so on a
  // mirror this row is a version line and nothing else — absent, never
  // disabled (§5.7).
  //
  // Neither reboots from a bare button: each goes through `confirm({reboot})`,
  // which renders the standing "the device reboots to apply this" line.
  //
  // Update… takes a `.luxr` release package (lib/luxr.ts) and installs BOTH
  // halves — firmware, then the web assets built from the same commit — in
  // one action (Gitea #643). A bare `.bin` is still accepted, because every
  // existing release artifact and every local `nix build` is one; the
  // difference is that the console then says out loud that the console
  // itself was NOT updated, which is the state the Athom went dark in.
  import AsyncButton from "../components/AsyncButton.svelte";
  import {
    device,
    deviceBoard,
    deviceCaps,
    deviceSlot,
    deviceVersion,
    refreshStatus,
  } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { note, notes, reportApiError } from "../stores/notify";
  import { boardMismatch, installRelease, readUpload, type Upload } from "../lib/install";
  import { LuxrError } from "../lib/luxr";

  let fileInput: HTMLInputElement | undefined;
  let busy = false;
  let progressText = "";
  let progressPct = 0;

  async function startApMode(): Promise<boolean> {
    const ok = await confirm({
      title: "Reboot into the setup access point?",
      body: "The device leaves this network for one boot and comes back as an open AP (luxel-…, http://192.168.4.1/). Rejoin this network by saving WiFi from the AP, or just reboot it again.",
      confirmLabel: "Reboot into AP",
      reboot: true,
    });
    if (!ok) return false;
    const r = await $device?.startApMode();
    note(
      "ap",
      r?.ok ? 'rebooting into AP "luxel-…" — connect to it at 192.168.4.1' : "failed",
      8000,
    );
    return r?.ok === true;
  }

  /** The confirm dialog's body, which is where the honest difference between
   *  a package and a bare image belongs — the user is about to authorise an
   *  OTA either way, and this is the last point at which "the console will
   *  still be the old one" is cheap to say. */
  function confirmBody(up: Upload, size: number): string {
    if (up.kind === "package")
      return (
        `${Math.round(up.pkg.app.length / 1024)} KB of firmware (v${up.pkg.version}, built for ` +
        `${up.pkg.board}) is written to the inactive OTA slot and the device reboots into it; ` +
        `the ${Math.round(up.pkg.assets.length / 1024)} KB web app from the same release is ` +
        `installed after it comes back, and this page reloads. A bad image rolls back on the ` +
        `next boot.`
      );
    return (
      `${Math.round(size / 1024)} KB is written to the inactive OTA slot and the device reboots ` +
      `into it. This is a firmware image ONLY — the web app on the device stays as it is, and ` +
      `if the new firmware reads a newer bytecode format, the console it serves will not be ` +
      `able to compile for it. Install the matching web assets afterwards, or use the ` +
      `release's .luxr package instead. A bad image rolls back on the next boot.`
    );
  }

  /** Install a release: `.luxr` package (firmware + its web assets) or a bare
   *  app image. */
  async function onImagePicked(e: Event): Promise<void> {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = ""; // so picking the same file twice fires again
    if (!file || !$device) return;

    let up: Upload;
    try {
      up = await readUpload(new Uint8Array(await file.arrayBuffer()));
    } catch (err) {
      const why = err instanceof LuxrError ? err.message : String(err);
      reportApiError(why, { scope: "ota", subject: file.name });
      return;
    }

    // #389's lesson, one step earlier than ota-push.sh's image grep: a
    // wrong-board image installs cleanly and differs only in its pin map.
    if (up.kind === "package") {
      const bad = boardMismatch(up.pkg.board, $deviceBoard || undefined);
      if (bad) {
        reportApiError(bad, { scope: "ota", subject: file.name });
        return;
      }
    }

    const ok = await confirm({
      title: `Install ${file.name}?`,
      body: confirmBody(up, file.size),
      confirmLabel: up.kind === "package" ? "Install & reboot" : "Install firmware only",
      reboot: true,
    });
    if (!ok) return;

    busy = true; // the button steps aside for `fw-busy` while this runs
    progressText = "reading…";
    progressPct = 0;
    try {
      const r = await installRelease(
        $device,
        up,
        { version: $deviceVersion, slot: $deviceSlot },
        (p) => {
          progressText = p.text;
          progressPct = p.pct;
        },
      );
      if (!r.ok) {
        note("ota", `failed: ${r.error ?? "rejected"}`, 0);
        return;
      }
      await refreshStatus();
      if (r.assetsInstalled) {
        note("ota", `installed v${r.version ?? "?"} — reloading the console…`, 0);
        location.reload();
      } else {
        note(
          "ota",
          `installed v${r.version ?? "?"}. The web app on the device was NOT updated — ` +
            `install the matching web assets if this release changed them.`,
          0,
        );
      }
    } catch (err) {
      note("ota", `failed: ${String(err)}`, 0);
    } finally {
      busy = false;
      progressText = "";
    }
  }
</script>

<div class="field">
  <span class="flabel">Version</span>
  <div class="fctl">
    <span class="mono" data-role="fw-version">
      {$deviceVersion ? `v${$deviceVersion}` : "—"}
      {#if $deviceSlot}<span class="dim">· {$deviceSlot}</span>{/if}
    </span>
    {#if $deviceBoard}
      <span class="dim hint" data-role="fw-board">{$deviceBoard}</span>
    {/if}
  </div>
</div>

{#if $deviceCaps?.ota}
  <div class="field">
    <span class="flabel"></span>
    <div class="fctl row g10">
      <input
        bind:this={fileInput}
        type="file"
        accept=".luxr,.bin,application/octet-stream"
        hidden
        data-role="fw-file"
        on:change={(e) => void onImagePicked(e)}
      />
      <!-- a push in flight is a PROGRESS state, not a capability gate: the
           button steps aside for the progress line rather than greying out
           (§5.7, Gitea #529) -->
      {#if busy}
        <span class="dim hint" data-role="fw-busy">{progressText}</span>
        <progress data-role="fw-progress" value={progressPct} max="1"></progress>
      {:else}
        <button data-role="fw-update" on:click={() => fileInput?.click()}>Update…</button>
      {/if}
      <span class="dim hint">
        pick the <span class="mono">.luxr</span> release package for this board — it carries the
        firmware <strong>and</strong> the matching web app, and the device
        <strong>reboots</strong> between them. A bare
        <span class="mono">luxel.bin</span> installs firmware only.
      </span>
      {#if $notes.ota}<span class="dim hint" data-role="fw-note">{$notes.ota}</span>{/if}
    </div>
  </div>
{/if}

{#if $deviceCaps?.reboot}
  <div class="field">
    <span class="flabel"></span>
    <div class="fctl row g10">
      <!-- The confirm dialog is INSIDE the action, so the spinner covers the
           dialog too — which is the point: no second reboot can be started
           from behind it, and the wait between "Reboot into AP" and the
           device going away had no indicator at all before (#738). -->
      <AsyncButton
        cls=""
        dataRole="apmode"
        label="Reboot into setup AP"
        doneLabel="Rebooting…"
        action={startApMode}
      />
      <span class="dim hint">
        one boot only — good for re-provisioning; it comes back as a station afterwards
      </span>
      {#if $notes.ap}<span class="dim hint" data-role="apmode-note">{$notes.ap}</span>{/if}
    </div>
  </div>
{/if}

<style>
  progress {
    flex: none;
    width: 140px;
    height: 6px;
  }
</style>
