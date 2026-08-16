<script lang="ts">
  import { onMount } from "svelte";
  import {
    loadFirmwareSource,
    normalizeArch,
    type BoardImage,
    type FirmwareSource,
  } from "./lib/releases";
  import {
    normalizeAddress,
    probeDevice,
    pushAssets,
    uploadToWled,
    waitForLuxel,
    type Probe,
    type WaitProgress,
  } from "./lib/device";

  // ── step 1: firmware source ──
  let source: FirmwareSource | null = null;
  let sourceState: "loading" | "ready" | "none" = "loading";

  onMount(async () => {
    source = await loadFirmwareSource();
    sourceState = source ? "ready" : "none";
  });

  // ── step 2: find the device ──
  let addrInput = "";
  let origin: string | null = null;
  let probe: Probe | null = null;
  let probing = false;

  async function checkDevice() {
    origin = normalizeAddress(addrInput);
    if (!origin) return;
    probing = true;
    probe = null;
    resetFlash();
    probe = await probeDevice(origin);
    probing = false;
  }

  // What the probe tells us about which chips can be right.
  $: arch = probe?.kind === "wled" ? normalizeArch(probe.arch) : null;
  $: archUnsupported =
    arch !== null && arch !== "esp32" && arch !== "esp32c3" ? arch : null;

  // ── step 3: pick a board ──
  let boardId = "";
  $: boards = source?.boards ?? [];
  $: compatibleBoards = arch ? boards.filter((b) => b.chip === arch) : boards;
  $: {
    // keep the selection valid as probe results change
    if (boardId && !compatibleBoards.some((b) => b.id === boardId)) boardId = "";
    const only = compatibleBoards.length === 1 ? compatibleBoards[0] : undefined;
    if (!boardId && only) boardId = only.id;
  }
  $: board = boards.find((b) => b.id === boardId) ?? null;

  $: deviceReady =
    probe !== null &&
    (probe.kind === "wled" || probe.kind === "reachable") &&
    archUnsupported === null;

  // ── step 4: flash ──
  type FlashState = "idle" | "downloading" | "uploading" | "sent" | "blocked" | "failed";
  let flashState: FlashState = "idle";
  let flashError = "";
  let binFileInput: HTMLInputElement | null = null;

  function resetFlash() {
    flashState = "idle";
    flashError = "";
    waitState = "idle";
    assetsState = "idle";
    luxelUp = null;
  }

  async function imageBlob(b: BoardImage): Promise<Blob | null> {
    if (source?.canFetchBinaries) {
      flashState = "downloading";
      try {
        const res = await fetch(b.url);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return await res.blob();
      } catch (e) {
        flashState = "failed";
        flashError = `could not download ${b.file}: ${String(e)}`;
        return null;
      }
    }
    const f = binFileInput?.files?.[0];
    if (!f) {
      flashState = "failed";
      flashError = `download ${b.file} with the link above, then choose the file here`;
      return null;
    }
    // wrong-board images brick nothing (the boot guard rolls back), but
    // catch the obvious slip before it costs a recovery cycle
    if (
      f.name !== b.file &&
      !confirm(`selected "${f.name}" but the ${b.name} image is "${b.file}" — flash anyway?`)
    )
      return null;
    return f;
  }

  async function flash() {
    if (!origin || !board) return;
    flashError = "";
    const blob = await imageBlob(board);
    if (!blob) return;
    flashState = "uploading";
    const sent = await uploadToWled(origin, blob, board.file);
    if (!sent) {
      flashState = "blocked";
      return;
    }
    flashState = "sent";
    await startWatching();
  }

  // ── step 5: wait for Luxel ──
  type WaitState = "idle" | "waiting" | "up" | "timeout";
  let waitState: WaitState = "idle";
  let waitProgress: WaitProgress = { elapsedMs: 0, budgetMs: 180_000 };
  let luxelUp: { version: string; slot: string } | null = null;

  async function startWatching() {
    if (!origin) return;
    waitState = "waiting";
    luxelUp = await waitForLuxel(origin, (p) => (waitProgress = p));
    waitState = luxelUp ? "up" : "timeout";
  }

  // ── step 6: web app assets ──
  type AssetsState = "idle" | "pushing" | "done" | "failed";
  let assetsState: AssetsState = "idle";
  let luxaFileInput: HTMLInputElement | null = null;

  async function sendAssets() {
    if (!origin || !source) return;
    let buf: ArrayBuffer;
    if (source.canFetchBinaries) {
      assetsState = "pushing";
      try {
        const res = await fetch(source.luxa.url);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        buf = await res.arrayBuffer();
      } catch {
        assetsState = "failed";
        return;
      }
    } else {
      const f = luxaFileInput?.files?.[0];
      if (!f) {
        assetsState = "failed";
        return;
      }
      assetsState = "pushing";
      buf = await f.arrayBuffer();
    }
    assetsState = (await pushAssets(origin, buf)) ? "done" : "failed";
  }

  const fmtKB = (n?: number) => (n ? `${Math.round(n / 1024)} KB` : "");
</script>

<main>
  <h1>Convert a WLED device to <span class="lux">Luxel</span></h1>
  <p class="intro">
    Luxel installs itself <em>through WLED's own update page</em> — no wires, no serial
    adapter. The device keeps its WiFi settings and comes back on the same address.
    <a href="https://github.com/GoogleBot42/luxel" target="_blank" rel="noreferrer">What is Luxel?</a>
  </p>
  <p class="beta" data-role="beta-note">
    ⚠ The takeover is <strong>beta</strong>. It has recovery built in (a failed install
    rolls back to WLED by itself), but keep physical access to the device: in rare cases
    the first boot panics once and needs a few extra seconds — or a power cycle — to
    sort itself out.
  </p>

  <!-- ── 1 · firmware ── -->
  <section data-role="fw-source">
    <h2>1 · Firmware</h2>
    {#if sourceState === "loading"}
      <p class="dim">Looking up the latest release…</p>
    {:else if sourceState === "none"}
      <p class="err">
        Couldn't reach a firmware source (no bundled firmware next to this page, and the
        GitHub API didn't answer). Check your internet connection, or fetch images by hand
        from <a href="https://github.com/GoogleBot42/luxel/releases" target="_blank" rel="noreferrer">the releases page</a>
        and reload.
      </p>
    {:else if source}
      <p>
        Luxel <strong>v{source.version}</strong>
        {#if source.mode === "bundled"}
          <span class="tag">bundled with this page</span>
        {:else}
          <span class="tag">from GitHub releases</span>
        {/if}
        — <a href={source.releaseUrl} target="_blank" rel="noreferrer">release notes</a>
      </p>
      {#if !source.canFetchBinaries}
        <p class="dim">
          This copy of the installer isn't hosted next to the firmware files, so your
          browser can't download them for you — you'll download two files by hand below.
          Everything else is unchanged.
        </p>
      {/if}
    {/if}
  </section>

  <!-- ── 2 · device ── -->
  <section data-role="device">
    <h2>2 · Your WLED device</h2>
    <p class="dim">
      The address it has on your network — the same one you open WLED with.
    </p>
    <form
      on:submit|preventDefault={checkDevice}
      class="row"
    >
      <input
        data-role="ip-input"
        placeholder="192.168.1.50"
        bind:value={addrInput}
        disabled={sourceState !== "ready"}
      />
      <button data-role="probe-btn" disabled={sourceState !== "ready" || probing || normalizeAddress(addrInput) === null}>
        {probing ? "Checking…" : "Check device"}
      </button>
    </form>

    {#if probe}
      <p data-role="probe-result">
        {#if probe.kind === "wled"}
          <span class="ok">✓</span> Found <strong>{probe.name}</strong> — WLED
          {probe.version}, chip <code>{probe.arch}</code>.
        {:else if probe.kind === "luxel"}
          <span class="ok">✓</span> This device already runs
          <strong>Luxel v{probe.version}</strong> — nothing to convert.
          <a href={origin} target="_blank" rel="noreferrer">Open its console</a>, or use
          step 5 below to (re)send the web app to it.
        {:else if probe.kind === "reachable"}
          <span class="ok">✓</span> Something answered at <code>{origin}</code>, but this
          browser isn't allowed to read what it is (older WLED versions don't permit
          cross-site reads). If it's your WLED device, pick its chip type yourself below —
          open <a href={`${origin}/json/info`} target="_blank" rel="noreferrer"><code>{origin}/json/info</code></a>
          in a new tab and look for <code>"arch"</code>.
        {:else if probe.blocked}
          <span class="bad">✗</span> The browser refused to contact the device: this page
          is served over HTTPS and the device is plain-HTTP on your LAN. Use Chrome (which
          can ask permission for local-network access), or follow the
          <em>manual steps</em> below — they work from any browser.
        {:else}
          <span class="bad">✗</span> Nothing answered at <code>{origin}</code>. Check the
          address and that you're on the same network.
        {/if}
      </p>
      {#if archUnsupported}
        <p class="err" data-role="arch-stop">
          {#if archUnsupported === "esp8266"}
            This is an <strong>ESP8266</strong> device. Luxel needs an ESP32-class chip —
            ESP8266 can't run it, and never will (not enough RAM). Sorry!
          {:else}
            Chip <code>{archUnsupported}</code> has no Luxel build yet — only classic
            ESP32 and ESP32-C3 are built today. Watch the
            <a href="https://github.com/GoogleBot42/luxel/releases" target="_blank" rel="noreferrer">releases</a>
            for new targets.
          {/if}
        </p>
      {/if}
    {/if}
  </section>

  <!-- ── 3+4 · board & flash ── -->
  {#if deviceReady && source}
    <section data-role="flash">
      <h2>3 · Flash it</h2>
      <label class="row">
        <span>This device is {arch ? "a" : "an…"}</span>
        <select data-role="board-select" bind:value={boardId}>
          <option value="" disabled>pick your board…</option>
          {#each compatibleBoards as b (b.id)}
            <option value={b.id}>{b.name}</option>
          {/each}
        </select>
      </label>
      {#if board}
        {#if !source.canFetchBinaries}
          <p>
            <a href={board.url} download>Download {board.file}</a>
            <span class="dim">({fmtKB(board.size)})</span>, then choose it here:
            <input type="file" accept=".bin" bind:this={binFileInput} data-role="bin-file" />
          </p>
        {/if}
        <p class="row">
          <button
            class="primary"
            data-role="flash-btn"
            disabled={flashState === "downloading" || flashState === "uploading" || waitState === "waiting" || waitState === "up"}
            on:click={flash}
          >
            {#if flashState === "downloading"}Downloading…{:else if flashState === "uploading"}Uploading to WLED…{:else}Install Luxel v{source.version}{/if}
          </button>
          {#if flashState === "failed"}<span class="err">{flashError}</span>{/if}
        </p>
        {#if flashState === "blocked"}
          <p class="err">
            The browser wouldn't send the upload (cross-site restrictions). Use the manual
            steps below — same result.
          </p>
        {/if}
        <details data-role="manual-steps" open={flashState === "blocked"}>
          <summary>Manual steps (works in any browser)</summary>
          <ol>
            <li>
              {#if source.canFetchBinaries}
                Download <a href={board.url} download>{board.file}</a>.
              {:else}
                Download {board.file} with the link above.
              {/if}
            </li>
            <li>
              Open your device's update page:
              <a href={`${origin}/update`} target="_blank" rel="noreferrer"><code>{origin}/update</code></a>
            </li>
            <li>Choose the downloaded file and press <em>Update!</em> (if WLED asks for an
              OTA passphrase, it's the one set in its security settings).</li>
            <li>
              Come back here and
              <button on:click={startWatching} disabled={waitState === "waiting"} data-role="watch-btn">watch for Luxel</button>
            </li>
          </ol>
        </details>
      {/if}
    </section>
  {/if}

  <!-- ── 4 · progress ── -->
  {#if waitState !== "idle"}
    <section data-role="progress">
      <h2>4 · The takeover</h2>
      {#if waitState === "waiting"}
        <p data-role="wait-status">
          Waiting for Luxel to come up at <code>{origin}</code> —
          {Math.round(waitProgress.elapsedMs / 1000)}s
          <span class="dim">(WLED writes the image, reboots, Luxel repartitions and
          reboots again, then rejoins your WiFi — usually 1–2 minutes)</span>
        </p>
        <progress value={waitProgress.elapsedMs} max={waitProgress.budgetMs}></progress>
      {:else if waitState === "up" && luxelUp}
        <p data-role="takeover-ok">
          <span class="ok">✓</span> <strong>Luxel v{luxelUp.version}</strong> is up at
          <code>{origin}</code>.
        </p>
      {:else if waitState === "timeout"}
        <div class="err" data-role="timeout-help">
          <p>Luxel didn't answer within 3 minutes. In likely order:</p>
          <ul>
            <li>
              <strong>It's in setup mode.</strong> If WLED had no saved WiFi settings to
              inherit, Luxel starts an open access point instead — look for a WiFi network
              named <code>luxel-XXXX</code>, join it, and finish setup at
              <code>http://192.168.4.1/</code>.
            </li>
            <li>
              <strong>The upload never took.</strong> If WLED has an OTA passphrase set,
              its update page silently rejects unauthenticated uploads — use the manual
              steps and enter the passphrase.
            </li>
            <li>
              <strong>It's wedged on first boot.</strong> Rarely the first Luxel boot
              panics once; it normally recovers alone, but a power cycle (off, wait 5 s,
              on) helps it along. A genuinely failed install rolls back to WLED by itself
              after 3 boots.
            </li>
          </ul>
          <p>
            <button on:click={startWatching} data-role="rewatch-btn">Keep watching</button>
          </p>
        </div>
      {/if}
    </section>
  {/if}

  <!-- ── 5 · web app ── -->
  {#if source && (waitState === "up" || probe?.kind === "luxel")}
    <section data-role="assets">
      <h2>5 · The web app</h2>
      <p class="dim">
        The firmware image is all engine — the control app ({fmtKB(source.luxa.size)})
        lives in a separate flash region and is sent over the network. Until it lands, the
        device serves a bare fallback page.
      </p>
      {#if !source.canFetchBinaries}
        <p>
          <a href={source.luxa.url} download>Download {source.luxa.file}</a>, then choose
          it here: <input type="file" accept=".luxa" bind:this={luxaFileInput} data-role="luxa-file" />
        </p>
      {/if}
      <p class="row">
        <button
          class="primary"
          data-role="assets-btn"
          disabled={assetsState === "pushing" || assetsState === "done"}
          on:click={sendAssets}
        >
          {assetsState === "pushing" ? "Sending…" : "Send the web app"}
        </button>
        {#if assetsState === "failed"}
          <span class="err">
            Push failed — retry, or from a terminal:
            <code>curl --data-binary @{source.luxa.file} {origin}/api/assets</code>
          </span>
        {/if}
      </p>
      {#if assetsState === "done"}
        <p data-role="done">
          <span class="ok">✓</span> All done —
          <a href={origin} target="_blank" rel="noreferrer" data-role="done-link">
            open your Luxel at <code>{origin}</code>
          </a> 🎉
        </p>
      {/if}
    </section>
  {/if}

  <footer>
    <a href="index.html">Luxel playground</a> ·
    <a href="https://github.com/GoogleBot42/luxel" target="_blank" rel="noreferrer">GitHub</a>
    · Works on ESP32 and ESP32-C3 devices with at least 4 MB flash.
  </footer>
</main>

<style>
  main {
    max-width: 720px;
    margin: 0 auto;
    padding: 24px 16px 48px;
  }
  h1 {
    font-size: 24px;
    font-weight: 600;
  }
  .lux {
    color: var(--accent);
  }
  h2 {
    font-size: 16px;
    font-weight: 600;
    margin: 0 0 8px;
  }
  section {
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px 16px;
    margin: 14px 0;
  }
  .intro {
    color: var(--text-dim);
  }
  .beta {
    border: 1px solid var(--warn);
    border-radius: 8px;
    padding: 8px 12px;
    color: var(--warn);
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .dim {
    color: var(--text-dim);
  }
  .ok {
    color: #6fc36f;
  }
  .bad,
  .err {
    color: var(--error);
  }
  .err code,
  p code {
    color: var(--text);
  }
  .tag {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 1px 8px;
    font-size: 12px;
    color: var(--text-dim);
  }
  button.primary {
    background: var(--accent);
    color: #14161a;
    border-color: var(--accent);
    font-weight: 600;
  }
  button.primary:disabled {
    opacity: 0.6;
    cursor: default;
  }
  progress {
    width: 100%;
    accent-color: var(--accent);
  }
  details {
    margin-top: 8px;
    color: var(--text-dim);
  }
  details a,
  details code {
    color: var(--text);
  }
  summary {
    cursor: pointer;
  }
  a {
    color: var(--accent);
  }
  footer {
    margin-top: 24px;
    color: var(--text-dim);
    font-size: 12px;
  }
</style>
