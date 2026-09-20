<script lang="ts">
  // Wall clock / time zone — what `clockHour()` patterns read (mockup S3,
  // `Clock & time zone · UTC-6 · synced`).
  //
  // Three things the raw `±hours` number field got wrong (Jeremy,
  // 2026-09-19): a zone is a PLACE, not an offset; there was no way to ask
  // for a sync; and the device time was printed in `en-US` 24-hour regardless
  // of who was looking. All three are fixed on the client — the wire is
  // still `tzMinutes`, because a device with 2 KB of NVS is not going to
  // carry the IANA database (Gitea #538).
  import { offsetLabel, zoneLabel, zonesByRegion, zoneOffsetMinutes } from "../lib/settingsCaps";
  import { clockStatus, device, refreshClock, syncDeviceClock } from "../stores/device";
  import { note, notes, reportApiError } from "../stores/notify";

  /** Where the chosen ZONE NAME lives. The device stores only the offset, so
   *  the name is the browser's memory of which place that offset came from —
   *  per browser, like every other display preference. */
  const ZONE_KEY = "luxel.clock.zone";

  /** Every zone this browser knows, grouped by region for the `<optgroup>`s.
   *  `supportedValuesOf` is Chrome 99+/Firefox 115+; an engine without it
   *  falls back to the zone the browser is actually in, which is the one
   *  answer that is always right. */
  function allZones(): string[] {
    const sv = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] })
      .supportedValuesOf;
    const list = typeof sv === "function" ? sv("timeZone") : [];
    return list.length > 0 ? list : [browserZone()];
  }

  function browserZone(): string {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
    } catch {
      return "UTC";
    }
  }

  const ZONES = allZones();
  const REGIONS = zonesByRegion(ZONES);

  /** The zone the user last picked here, else the browser's own — and, when
   *  the device's stored offset disagrees with both, no zone at all (the
   *  offset came from somewhere else and naming a place would be a lie). */
  function initialZone(tzMinutes: number): string {
    let stored = "";
    try {
      stored = localStorage.getItem(ZONE_KEY) ?? "";
    } catch {
      stored = "";
    }
    if (stored && ZONES.includes(stored) && zoneOffsetMinutes(stored) === tzMinutes) return stored;
    const here = browserZone();
    if (zoneOffsetMinutes(here) === tzMinutes) return here;
    return "";
  }

  let zone = "";
  let picked = false;
  /** Pick the zone once, from the FIRST clock reading — re-deriving it on
   *  every 0.5 Hz poll would fight a user who chose a zone whose offset
   *  matches several places. */
  $: if (!picked && $clockStatus) {
    picked = true;
    zone = initialZone($clockStatus.tzMinutes);
  }

  function onZoneChange(e: Event): void {
    const want = (e.target as HTMLSelectElement).value;
    zone = want;
    try {
      localStorage.setItem(ZONE_KEY, want);
    } catch {
      /* private mode: the offset still reaches the device, only the name is lost */
    }
    void (async () => {
      const r = await $device?.setClock(zoneOffsetMinutes(want));
      if (r && r.ok === false)
        reportApiError(r.error ?? "rejected", { scope: "clock", field: "clock-tz" });
      void refreshClock();
    })();
  }

  async function syncNow(): Promise<void> {
    note("clock", "asking the device to sync…");
    const ok = await syncDeviceClock();
    note("clock", ok ? "sync requested" : "the device did not answer", 4000);
  }

  /**
   * The device's wall clock, in the LOOKING user's locale.
   *
   * `/api/clock` `local` is already shifted by the device's own offset, so
   * the UTC instant is `local - tzMinutes`; formatting THAT in the chosen
   * zone gives the wall clock the fixture is actually keeping, with the
   * date, the separators and the 12/24-hour choice the browser's locale
   * asks for. Without a named zone there is nothing to format it in, so it
   * is rendered as the UTC instant it literally is.
   */
  function deviceTime(local: number, tzMinutes: number, z: string): string {
    const utc = new Date((local - tzMinutes * 60) * 1000);
    try {
      return utc.toLocaleString(undefined, {
        timeZone: z || "UTC",
        dateStyle: "medium",
        timeStyle: "medium",
      });
    } catch {
      return utc.toISOString().replace("T", " ").slice(0, 19);
    }
  }

  $: offset = offsetLabel($clockStatus?.tzMinutes ?? 0);
</script>

<div class="field">
  <span class="flabel">Device time</span>
  <div class="fctl row g10">
    <span class="mono dtime" data-role="clock-status">
      {#if $clockStatus?.synced}
        {deviceTime($clockStatus.local, $clockStatus.tzMinutes, zone)}
      {:else}
        not NTP-synced yet
      {/if}
    </span>
    <button class="btn sm" data-role="clock-sync" on:click={() => void syncNow()}>Sync now</button>
    {#if $notes.clock}<span class="dim hint" data-role="clock-note">{$notes.clock}</span>{/if}
  </div>
</div>

<div class="field">
  <span class="flabel">Time zone</span>
  <div class="fctl row g10">
    <select class="zonesel" data-role="clock-tz" value={zone} on:change={onZoneChange}>
      {#if zone === ""}
        <!-- the device's stored offset matches no zone we can name; saying so
             is honest, and picking any zone below replaces it -->
        <option value="">{offset} (no zone set)</option>
      {/if}
      {#each REGIONS as g (g.region)}
        <optgroup label={g.region}>
          {#each g.zones as z (z)}<option value={z}>{zoneLabel(z)}</option>{/each}
        </optgroup>
      {/each}
    </select>
    <span class="dim hint" data-role="clock-offset">
      {offset} · {$clockStatus?.synced ? "synced" : "not synced"} — drives clockHour()
    </span>
  </div>
</div>

<style>
  .dtime {
    font-size: 13px;
  }

  .zonesel {
    width: 260px;
    max-width: 100%;
  }
</style>
