<script lang="ts">
  // Bytecode-format skew, and the store repair it implies (Gitea #643).
  //
  // Mounted once in the shell, directly under ErrorBar, so it is on screen on
  // every tab including the full-screen editor — the place a user is most
  // likely to hit the failure (every save refused) is the place they are
  // least likely to go looking for a Settings row.
  //
  // It renders nothing at all in the normal case: two versions that agree, a
  // store with nothing stale. Everything below is a failure surface.
  import { bcSkew, skewBanner, type BcSkew } from "../lib/bcskew";
  import { healStaleStore, healSummary, type HealReport } from "../lib/heal";
  import { parseLuxr, isLuxr } from "../lib/luxr";
  import {
    device,
    deviceBcFormat,
    devicePatterns,
    deviceRunningId,
    deviceVmerr,
    refreshDevicePatterns,
    refreshPlaylist,
  } from "../stores/device";
  import { compileToBytecode, luxel } from "../stores/pattern";

  /** The format THIS bundle compiles — 0 until the wasm is loaded, and 0 on
   *  a luxel.wasm older than `lx_bc_format`, both of which read "unknown". */
  $: bundleFormat = $luxel?.bcFormat() ?? 0;
  $: skew = bcSkew(bundleFormat, $deviceBcFormat || undefined) as BcSkew;
  $: banner = skewBanner(skew, bundleFormat, $deviceBcFormat || undefined);

  // ---- the self-heal ----
  //
  // Armed by evidence, not by a timer: a row the device flagged `stale`, or
  // the device's own `vmerr` complaining about a format it cannot read. It
  // runs at most once per (device, evidence) — `healedFor` is the guard that
  // makes a 1 Hz status poll not start eleven repairs.
  let healing = false;
  let healedFor = "";
  let progress = "";
  let report: HealReport | null = null;

  $: staleRows = $devicePatterns.filter((p) => p.stale);
  $: evidence =
    skew === "match" && $device
      ? staleRows.length > 0
        ? `rows:${staleRows.map((p) => p.id).join(",")}`
        : /bytecode format v\d+ \(this build reads v\d+\)/.test($deviceVmerr ?? "")
          ? `vmerr:${$deviceRunningId}`
          : ""
      : "";
  $: if (evidence && evidence !== healedFor && !healing) void heal(evidence);

  async function heal(key: string): Promise<void> {
    const d = $device;
    if (!d) return;
    healing = true;
    healedFor = key;
    report = null;
    progress = "checking the stored patterns…";
    try {
      const r = await healStaleStore(d, compileToBytecode, {
        runningId: $deviceRunningId,
        onProgress: (done, total, name) => {
          progress = `${total} stored pattern${total === 1 ? " was" : "s were"} compiled for an older engine — recompiling ${done + 1} of ${total} (${name})…`;
        },
      });
      report = r;
      // The library rows carry the `stale` flags and the playlist carries the
      // per-item verdicts; both are stale themselves now.
      await refreshDevicePatterns();
      await refreshPlaylist();
    } catch (e) {
      report = {
        found: 0,
        repaired: 0,
        failed: [{ id: "", name: "the store", why: String(e) }],
        reactivated: null,
      };
    } finally {
      healing = false;
      progress = "";
    }
  }

  /** Dismiss the finished report (the banner itself is not dismissible —
   *  the condition is, by fixing it). */
  function clearReport(): void {
    report = null;
  }

  // ---- the inline web-asset upload ----
  //
  // On the `bundle-older` banner only, because that is the one case where
  // the fix is a file the user already has: the `.luxr` (or the bare `.luxa`)
  // from the release the firmware came from. Installing it replaces the
  // console being looked at, hence the reload.
  let assetInput: HTMLInputElement | undefined;
  let sending = false;
  let sendError = "";

  async function onAssetsPicked(e: Event): Promise<void> {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !$device) return;
    sending = true;
    sendError = "";
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      // A `.luxr` carries both halves; take only the assets — the firmware
      // is already the newer one, which is the whole problem.
      const archive = isLuxr(bytes) ? (await parseLuxr(bytes)).assets : bytes;
      if (archive.length === 0) {
        sendError = "that package carries no web assets";
        return;
      }
      const r = await $device.assetsUpload(archive.slice().buffer);
      if (!r.ok) {
        sendError = r.error ?? "the device refused the web assets";
        return;
      }
      location.reload();
    } catch (err) {
      sendError = String(err);
    } finally {
      sending = false;
    }
  }
</script>

{#if banner}
  <div class="bcbar" data-role={banner.id}>
    <span class="txt" data-role="bc-banner-text">{banner.text}</span>
    {#if banner.offerAssets}
      <input
        bind:this={assetInput}
        type="file"
        accept=".luxr,.luxa,application/octet-stream"
        hidden
        data-role="bc-assets-file"
        on:change={(e) => void onAssetsPicked(e)}
      />
      {#if sending}
        <span class="dim" data-role="bc-assets-busy">sending…</span>
      {:else}
        <button data-role="bc-assets-upload" on:click={() => assetInput?.click()}>
          Install web assets…
        </button>
      {/if}
    {/if}
  </div>
  {#if sendError}
    <div class="bcbar err" data-role="bc-assets-error">{sendError}</div>
  {/if}
{/if}

{#if healing}
  <div class="bcbar" data-role="bc-healing">
    <span class="txt" data-role="bc-healing-text">{progress}</span>
  </div>
{:else if report && report.found > 0}
  <div class="bcbar" class:ok={report.failed.length === 0} data-role="bc-healed">
    <span class="txt" data-role="bc-healed-text">{healSummary(report)}</span>
    <button data-role="bc-healed-dismiss" on:click={clearReport}>Dismiss</button>
  </div>
{/if}

<style>
  /* The house warn recipe (editor-frame.css `.banner.warn`, Settings'
     `.capstrip`), in the shell rather than a page rail. */
  .bcbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    font-size: 13px;
    line-height: 1.45;
    border-bottom: 1px solid var(--warn);
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    color: #ecd9a8;
  }
  .bcbar.err {
    border-bottom-color: var(--error);
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: #f0c3c3;
  }
  .bcbar.ok {
    border-bottom-color: var(--ok);
    background: color-mix(in srgb, var(--ok) 12%, transparent);
    color: #cfe8d6;
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
  .dim {
    color: var(--text-dim);
  }
  button {
    flex: none;
  }
  @media (max-width: 600px) {
    .bcbar {
      align-items: flex-start;
      flex-wrap: wrap;
    }
  }
</style>
