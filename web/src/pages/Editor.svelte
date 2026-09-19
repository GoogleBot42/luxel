<script lang="ts">
  // The pattern editor: toolbar (file actions), the code pane, the playback
  // bar, and the right-rail inspector (banners, debugger, preview, controls,
  // pins, vars). It owns the local WASM engine, the render loop and the
  // device push — the local-preview-plus-push model: the preview always runs
  // on the local engine, and the device is a sink we send code + controls to.
  import { createEventDispatcher, onDestroy, tick } from "svelte";
  import Controls from "../components/Controls.svelte";
  import Debugger from "../components/Debugger.svelte";
  import CodeEditor from "../components/Editor.svelte";
  import PinPanel from "../components/PinPanel.svelte";
  import Preview from "../components/Preview.svelte";
  import VarWatcher from "../components/VarWatcher.svelte";
  import MapEditor from "./MapEditor.svelte";
  import { MicSource, toSensorBoardFrame } from "../lib/audio";
  import { lxpEnvelope } from "../lib/device";
  import {
    Engine,
    type Control,
    type DebugSnapshot,
    type DeviceModel,
    type Diagnostic,
    type StepKind,
  } from "../lib/luxel";
  import {
    clearDeviceMap,
    device,
    deviceEngineHeap,
    deviceError,
    deviceHeapFree,
    deviceMap,
    devicePatterns,
    devicePixels,
    deviceVmerr,
    installDeviceGridMap,
    installDeviceMapCoords,
    isPlayground,
    playlist,
    queuePlaylistSave,
    refreshDevicePatterns,
    refreshStatus,
  } from "../stores/device";
  import { confirm, promptText } from "../stores/dialog";
  import {
    cubeLattice,
    deriveRig,
    layout,
    markPatternLoaded,
    markRigChosen,
    markSourcePasted,
    pixelCount,
    pixelTotal,
    takeRigDerivePending,
  } from "../stores/geometry";
  import { banners, clearNote, note, notes, setBanner } from "../stores/notify";
  import {
    compileToBytecode,
    controlValues,
    deleteFromLocalLibrary,
    devicePatternId,
    dirty,
    encodeShare,
    exampleName,
    exportEpe,
    findSaved,
    hints,
    luxel,
    mapSrc,
    NEW_PATTERN,
    parseEpe,
    patternName,
    previewFps,
    runtimeError,
    saved,
    saveToLocalLibrary,
    source,
  } from "../stores/pattern";

  /** The editor is the visible surface (drives keyboard shortcuts). */
  export let active = false;

  const dispatch = createEventDispatcher<{ open: void }>();

  let editor: CodeEditor;
  let preview: Preview;
  let mapRef: MapEditor;
  let fileInput: HTMLInputElement;

  let engine: Engine | undefined;
  let compileError: Diagnostic | null = null;
  let controls: Control[] = [];
  let readouts = new Map<string, number>();
  let vars: Record<string, number | number[]> = {};
  /** Digital pins the running pattern touches — the engine can't be asked
   *  statically (pin numbers are runtime values), so this is polled off
   *  `lx_pins_used` and the pin panel is shown only when it's non-empty
   *  (Gitea #205). `pinLatched` is owned here, not by the panel, because a
   *  recompile builds a fresh VM with nothing driven. */
  let pins: number[] = [];
  let pinLevels: Record<number, boolean> = {};
  let pinIdleHigh: Record<number, boolean> = {};
  let pinLatched: Record<number, boolean> = {};
  /** Analog pins the pattern samples (`analogRead`/`touchRead`) and the value
   *  each slider is parked at, 0..1 (Gitea #206). Polled the same way, and
   *  cleared on recompile for the same reason: a fresh VM reads 0 everywhere. */
  let analogPins: number[] = [];
  let analogValues: Record<number, number> = {};
  let targetFps = 60;
  let running = true;
  let debugMode = false;
  let breakpoints: number[] = [];
  let dbg: DebugSnapshot = { paused: false };
  /** Fetching/activating a pattern for the editor — cover the editor with a
   *  loading screen so a stale last-opened script never flashes first. */
  let patternLoading = false;
  /** File-actions overflow menu (import/export). */
  let menuOpen = false;
  let importError = "";
  /** Which document the left editor shows: the pattern or the map program. */
  let subTab: "pattern" | "map" = "pattern";
  let mapCompileError: Diagnostic | null = null;
  let mapDebugMode = false;
  let mapDbg: DebugSnapshot = { paused: false };

  let debounce: ReturnType<typeof setTimeout> | undefined;
  let pushDebounce: ReturnType<typeof setTimeout> | undefined;
  let raf = 0;
  let lastT = 0;
  let lastPoll = 0;

  // the map sub-tab exists only while a 2D map is the active layout
  $: if ($layout.kind !== "map" && subTab === "map") subTab = "pattern";

  /** The device's vmerr, but only when it is a capacity rejection — other
   *  runtime errors are the local engine's business and already have a banner.
   *  Matches the firmware's wording (`firmware/src/main.rs`) and the mirror's.
   *
   *  Two shapes reach here. The load-time refusal ("pattern too large for this
   *  device") comes from the floor check, and so does the array-arena BYTE
   *  budget. The array ELEMENT ledger is the third: `array(pixelCount)` that
   *  fits the editor's preview layout and not the device's real pixel count
   *  fails during the pattern's init, and the device then renders black with
   *  nothing but this vmerr to say why (Gitea #420). Mirrors
   *  `luxel_core::vm::is_array_budget_error`. */
  $: deviceRejectedForSize =
    $deviceVmerr && /too large for this device|array element budget exceeded/.test($deviceVmerr)
      ? $deviceVmerr
      : "";

  // ---- capacity warning (Gitea #15) ----
  // Different ESP32s have different heap. A pattern that runs happily in the
  // playground can be rejected by the device it is pushed to, and the device's
  // rejection is ASYNCHRONOUS — /api/code returns 200 and the render task then
  // records a "pattern too large" vmerr — so without this the editor would
  // just silently keep showing the old pattern on the strip.
  //
  // Prediction: the wasm build models the firmware's own load sequence under a
  // counting allocator (`Luxel.deviceModel`), against the device's reported
  // free heap. Confirmation: after each push we read /api/status back, so the
  // device's own verdict replaces our guess the moment it exists.
  //
  // The model predicts the LIVE push (POST /api/code) — what this editor does
  // on every recompile, and the more expensive of the firmware's two load
  // paths: the upload envelope stays resident across a COPYING decode, where
  // a pattern activated from the device's own library is decoded straight off
  // the flash mapping with its code borrowed (Gitea #276/#300). When the live
  // push is the only thing that doesn't fit, the banner says so, because
  // "save it to the device" is then a real fix (Gitea #287).

  /** The local prediction for the source currently in the editor. */
  let capacity: { level: "over" | "tight"; text: string; detail: string } | null = null;

  // Re-model on every successful compile (the engine is rebuilt) and on every
  // fresh heap reading from the 1 Hz status poll. Dependencies are named in
  // the block itself — a `$:` only tracks what appears in its own syntax
  // (.claude/rules/web.md).
  $: {
    engine;
    compileError;
    $device;
    $deviceHeapFree;
    $deviceEngineHeap;
    $devicePixels;
    if (!compileError) checkCapacity();
  }

  const kb = (bytes: number): string => `${Math.round(bytes / 1024)} KB`;

  /** Model the pattern currently in the preview engine against the connected
   *  device and set (or clear) `capacity`. Silent in the playground, and
   *  silent on any device that can't report its free heap — an unknown budget
   *  is not a small one, and a guess here would cry wolf on every pattern. */
  function checkCapacity(): void {
    const lx = $luxel;
    if (!lx || !engine || !$device || $deviceHeapFree <= 0 || $devicePixels <= 0) {
      capacity = null;
      return;
    }
    const bytecode = engine.bytecode();
    // The device holds the WHOLE upload (name + source + bytecode) while it
    // decodes, so the envelope length — not the blob length — is what its heap
    // actually sees. Model against the device's own pixel count, never the
    // preview layout's: the strip length is hardware truth.
    const envelopeLen = lxpEnvelope("", $source, bytecode).length;
    const m: DeviceModel | null = lx.deviceModel(
      bytecode,
      envelopeLen,
      $devicePixels,
      $deviceHeapFree,
      $deviceEngineHeap,
    );
    if (!m) {
      capacity = null;
      return;
    }
    // "Saving it would fit" is only worth saying when it is actually true and
    // actually different — the stored path skips the envelope and borrows the
    // program's code from flash instead of copying it.
    const savingHelps = m.fit === "over" && m.storedFit !== "over";
    const detail =
      `a live push models ${kb(m.resident)} resident (peaking at ${kb(m.peak)}) ` +
      `at ${$devicePixels} px; saved to the device's library it would be ` +
      `${kb(m.storedResident)} (peak ${kb(m.storedPeak)}). The device reports ` +
      `${kb($deviceHeapFree)} free with its current pattern's ${kb($deviceEngineHeap)} ` +
      `still loaded, frees that first, and keeps ${kb(m.floor)} for itself — ` +
      `leaving ${kb(m.headroom)} for this pattern.` +
      (m.vmerr ? ` Device verdict: ${m.vmerr}` : "");
    const savingHint = savingHelps
      ? ` — saving it to the device's library would fit (${kb(m.storedResident)})`
      : "";
    if (m.vmerr) {
      // An array budget ran out before the floor check could even run — a
      // different failure from "the whole load doesn't fit", and worth saying
      // so, because the fix is smaller arrays rather than a smaller pattern.
      // Which budget matters: the byte arena is a size the user can read off
      // in KB, while the PB-compat ELEMENT ledger is a count that depends on
      // the device's pixel count, and blowing it makes the pattern load and
      // then render BLACK rather than be refused (Gitea #420). The exact
      // figures are in `detail` ("Device verdict: …").
      const elementLedger = /array element budget exceeded/.test(m.vmerr);
      capacity = {
        level: "over",
        text: elementLedger
          ? `this pattern needs more array elements than this device allows at ${$devicePixels} px — it would load and render black`
          : `this pattern's arrays exceed this device's array memory budget (${kb(m.budget)} available)`,
        detail,
      };
    } else if (m.fit === "over" && m.peak > m.base) {
      // The RESIDENT engine would have fitted; the upload itself doesn't.
      // Naming the upload is the difference between "rewrite your pattern"
      // and "save it to the device instead".
      capacity = {
        level: "over",
        text: `this pattern's upload is likely too large for this device (${kb(m.peak)} needed to receive it, ${kb(m.base)} free)${savingHint}`,
        detail,
      };
    } else if (m.fit === "over") {
      capacity = {
        level: "over",
        text: `this pattern is likely too large for this device (${kb(m.resident)} needed, ${kb(m.headroom)} free)${savingHint}`,
        detail,
      };
    } else if (m.fit === "tight") {
      capacity = {
        level: "tight",
        text: `close to this device's limit (${kb(m.resident)} of ${kb(m.headroom)} usable)`,
        detail,
      };
    } else {
      capacity = null;
    }
  }

  // ---- compile + push ----

  /** Rebuild the local preview engine from `source`. Device-independent: the
   *  preview always runs on the local WASM engine, even on a device. */
  export function recompile(): void {
    const lx = $luxel;
    if (!lx) return;
    const result = lx.compile($source, pixelCount());
    if (result instanceof Engine) {
      engine?.free();
      engine = result;
      compileError = null;
      runtimeError.set(null);
      if (takeRigDerivePending()) {
        // Once per load, never per keystroke (#372). A changed rig changes the
        // pixel count, so the engine is rebuilt at the new geometry — the
        // pending flag is already cleared, so this recurses exactly once.
        if (deriveRig(result)) {
          recompile();
          return;
        }
      }
      const l = $layout;
      if (l.kind === "grid") engine.setMapGrid(l.w, l.h);
      if (l.kind === "map") engine.setMap(l.coords);
      engine.setWallClock(Date.now() / 1000);
      controls = engine.controls();
      if (debugMode) {
        engine.debugEnable(true);
        applyBreakpoints();
      }
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      // seed //# defaults, then reapply saved control values (PB persists
      // control state per pattern)
      for (const c of controls) {
        if (c.kind === "showNumber" || c.kind === "gauge" || c.kind === "trigger") continue;
        const d = $hints.get(c.name)?.default;
        if (!(c.name in $controlValues) && d !== undefined) {
          controlValues.set({ ...$controlValues, [c.name]: [d] });
        }
        const savedValues = $controlValues[c.name];
        if (savedValues) engine.setControl(c.name, savedValues);
      }
      // a fresh VM drives nothing, so drop any latched pins and parked
      // analog values with it
      pinLatched = {};
      analogValues = {};
      refreshPins();
      preview?.clear(); // fresh program → fresh history
      // Re-model against the device on every successful compile, not just on
      // push: the warning is then already up while the 500 ms push debounce is
      // still counting down, and it appears for a pattern merely *opened* from
      // the library too — before the user has invested any editing in it.
      checkCapacity();
    } else {
      compileError = result; // keep the old engine running while typing
    }
  }

  /** Send the current source + its compiled LXBC bytecode to the device so
   *  its real LEDs follow the local preview. The device has no compiler —
   *  the local engine's bytecode IS what it executes; a rejection (or
   *  network error) surfaces as a device error. */
  export async function devicePush(): Promise<void> {
    const d = $device;
    if (!d) return;
    if (compileError || !engine) return; // never push a pattern the local compile rejected
    const bc = engine.bytecode();
    try {
      const r = await d.run($source, bc);
      if (!r.ok) deviceError.set(`device rejected the pattern: ${r.error}`);
      else deviceError.set("");
    } catch (e) {
      deviceError.set(`push failed: ${String(e)}`);
      return;
    }
    // Read the device back: a capacity rejection happens on the render task
    // AFTER /api/code has already answered 200, so the vmerr is the only place
    // it surfaces. This also refreshes the headroom the next prediction uses.
    await refreshStatus();
  }

  /** Local recompile (for the preview) plus an immediate device push — used
   *  when a whole new pattern is opened/created (typing debounces separately). */
  function applyEdit(): void {
    recompile();
    if ($device) void devicePush();
  }

  function onSourceChange(e: CustomEvent<string>): void {
    source.set(e.detail);
    dirty.set(true); // the user edited away from the loaded/saved pattern
    // local preview recompiles fast; the device push (over WiFi) is throttled
    clearTimeout(debounce);
    debounce = setTimeout(recompile, 150);
    if ($device) {
      clearTimeout(pushDebounce);
      pushDebounce = setTimeout(() => void devicePush(), 500);
    }
  }

  // The device is unreachable / rejected a push — a condition, not an event,
  // so it goes in the banner list and clears itself when it clears.
  $: setBanner("device-error", $deviceError ? { level: "error", text: $deviceError } : null);

  // ---- boot ----

  /** Device mode: a local engine first so the boot cover lifts onto a live
   *  preview, then the handshake, then the pulled/resumed source. */
  export async function bootDevice(
    connect: (pull: boolean) => Promise<{ ok: boolean; source: string | null }>,
    wipDirty: boolean,
  ): Promise<void> {
    recompile();
    startLoop();
    // A genuinely-unsaved edit is resumed AND pushed so the device runs it too
    // (editor, preview and device all agree). A clean copy instead opens
    // whatever pattern is currently active on the device.
    const r = await connect(!wipDirty);
    if (r.ok) {
      // The rig is reset to the hardware strip here, so it is re-derived from
      // the source (and the device's installed map) on the next compile (#372)
      layout.set({ kind: "strip", pixels: $devicePixels });
      markPatternLoaded();
      if (r.source !== null) {
        source.set(r.source); // show what's running on the device
        dirty.set(false); // editor now matches the running pattern
        patternName.set("");
        exampleName.set("");
        devicePatternId.set("");
      }
      compileError = null;
    }
    recompile(); // rebuild the preview from the pulled/resumed source
    if (wipDirty && $device) await devicePush();
  }

  /** Playground mode. `sharedMap` restores the geometry a share link carried. */
  export function bootPlayground(sharedMap: boolean): void {
    recompile();
    if (sharedMap) {
      // the link carried a map program: run it to restore the geometry
      mapRef?.markMounted();
      mapRef?.recompile(true);
    }
    startLoop();
  }

  function startLoop(): void {
    if (raf === 0) raf = requestAnimationFrame(loop);
  }

  // ---- opening patterns ----

  export function newPattern(): void {
    markPatternLoaded();
    source.set(NEW_PATTERN);
    patternName.set("");
    exampleName.set("");
    devicePatternId.set("");
    importError = "";
    controlValues.set({});
    dirty.set(false); // a fresh template — not yet edited
    subTab = "pattern";
    if ($layout.kind === "map") layout.set({ kind: "strip", pixels: pixelCount() });
    preview?.clear();
    void tick().then(applyEdit);
  }

  export function loadSaved(name: string): void {
    const p = findSaved(name);
    if (!p) return;
    preview?.clear();
    markPatternLoaded();
    patternName.set(p.name);
    exampleName.set("");
    importError = "";
    source.set(p.source);
    controlValues.set({});
    dirty.set(false); // freshly loaded from the library
    void tick().then(applyEdit);
  }

  export function loadGalleryPick(p: {
    name: string;
    kind: "strip" | "grid" | "cloud";
    source: string;
  }): void {
    preview?.clear(); // picking a pattern opens it in the editor
    markPatternLoaded();
    patternName.set(p.name);
    exampleName.set("");
    importError = "";
    devicePatternId.set("");
    if (!$device) {
      layout.set(
        p.kind === "grid"
          ? { kind: "grid", w: 16, h: 16 }
          : p.kind === "cloud"
            ? { kind: "map", coords: cubeLattice(5) } // render3D → rotating cloud
            : { kind: "strip", pixels: 60 },
      );
    }
    source.set(p.source);
    controlValues.set({});
    dirty.set(false); // freshly picked from the gallery
    void tick().then(applyEdit);
  }

  export async function openDevicePattern(id: string): Promise<void> {
    preview?.clear();
    patternLoading = true; // cover the editor until the source is fetched
    try {
      await loadDevicePattern(id); // activates it on the device
      recompile(); // build the local preview (no push — it's already running)
    } finally {
      patternLoading = false;
    }
  }

  async function loadDevicePattern(id: string): Promise<void> {
    const d = $device;
    if (!d) return;
    try {
      const p = await d.patternSource(id);
      let r = await d.activatePattern(id);
      if (!r.ok && r.code === "bc-version") {
        // the stored bytecode predates a firmware format bump (the device
        // can't recompile — it has no compiler): recompile from the stored
        // source, re-save, and retry once
        const bc = compileToBytecode(p.source);
        if (bc) {
          await d.savePattern(p.name, p.source, bc);
          r = await d.activatePattern(id);
        }
      }
      if (!r.ok) {
        deviceError.set(`activate failed: ${r.error}`);
        return;
      }
      devicePatternId.set(id);
      patternName.set(p.name);
      exampleName.set("");
      markPatternLoaded();
      source.set(p.source);
      dirty.set(false); // freshly loaded from the device — matches what's running
      compileError = null;
      // controls come from the local recompile the caller runs next
    } catch (e) {
      deviceError.set(`cannot load pattern: ${String(e)}`);
    }
  }

  // The device streams only source, not which library entry it came from — so
  // a freshly-opened running pattern shows as "untitled". If its source matches
  // a saved device pattern, adopt that name/id (so the header isn't "untitled"
  // and Add-to-playlist works). Runs as device pattern sources stream in.
  // deps passed as args so Svelte tracks devicePatterns (source-fill re-runs it)
  $: matchRunningToLibrary($devicePatterns, $source, $dirty, $devicePatternId, active, $device);
  function matchRunningToLibrary(
    pats: { id: string; name: string; source?: string }[],
    src: string,
    drt: boolean,
    dpid: string,
    edt: boolean,
    dev: unknown,
  ): void {
    if (!dev || !edt || drt || dpid || !src) return;
    const m = pats.find((p) => p.source && p.source.trim() === src.trim());
    if (m) {
      patternName.set(m.name);
      devicePatternId.set(m.id);
    }
  }

  // ---- .epe import / export ----

  export async function importEpeFile(file: File): Promise<void> {
    importError = "";
    try {
      const epe = await parseEpe(file);
      patternName.set(epe.name);
      exampleName.set("");
      devicePatternId.set("");
      markPatternLoaded();
      source.set(epe.source);
      controlValues.set({});
      dirty.set(true); // an imported .epe isn't in the library/device until saved
      dispatch("open"); // a dropped/imported .epe opens straight in the editor
      preview?.clear(); // fresh waterfall for the imported pattern
      await tick();
      applyEdit();
    } catch (e) {
      importError = `.epe import failed: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  function onImportPick(e: Event): void {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (f) void importEpeFile(f);
    input.value = ""; // allow re-importing the same file
  }

  function doExportEpe(): void {
    exportEpe($patternName || $exampleName || "luxel pattern", $source);
  }

  // ---- library ----

  /** Name-then-save. A7 (#468) moves the naming to an inline-editable header;
   *  when it does, only this `promptText` call goes away — the save itself is
   *  already independent of where the name came from. */
  async function saveToLibrary(): Promise<void> {
    const name = await promptText({
      title: $device ? "Save pattern on the device" : "Save pattern",
      label: "Name",
      initial: $patternName || $exampleName || "my pattern",
      placeholder: "my pattern",
      confirmLabel: "Save",
      validate: (v) => (v.trim() === "" ? "a name is required" : null),
    });
    if (name === null) return;
    if ($device) {
      const bc = compileToBytecode($source);
      if (!bc) {
        note("save", "save failed: pattern does not compile", 3000);
        return;
      }
      const r = await $device?.savePattern(name, $source, bc);
      if (r?.ok) {
        patternName.set(name);
        exampleName.set("");
        devicePatternId.set(r.id ?? "");
        dirty.set(false); // now stored on the device
        note("save", "saved to device", 3000);
        await refreshDevicePatterns();
      } else {
        note("save", r && "error" in r ? `save failed: ${r.error}` : "save failed", 3000);
      }
      return;
    }
    saveToLocalLibrary(name, $source);
    patternName.set(name);
    exampleName.set("");
    dirty.set(false); // now stored in the library
    note("save", "saved", 2000);
  }

  async function deleteSaved(): Promise<void> {
    if ($device && $devicePatternId) {
      const id = $devicePatternId; // fix the target before we await the dialog
      const ok = await confirm({
        title: "Delete pattern from the device?",
        body: `"${$patternName}" is removed from the device's stored library. This cannot be undone.`,
        confirmLabel: "Delete",
        danger: true,
      });
      if (!ok) return;
      await $device?.deletePattern(id);
      devicePatternId.set("");
      note("save", "deleted from device", 2000);
      await refreshDevicePatterns();
      return;
    }
    if (!$patternName || !$saved.some((s) => s.name === $patternName)) return;
    const ok = await confirm({
      title: "Delete pattern from the library?",
      body: `"${$patternName}" is removed from this browser's library. This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
    });
    if (!ok) return;
    deleteFromLocalLibrary($patternName);
    note("save", "deleted", 2000);
  }

  /** Append the CURRENT editor pattern (must be saved on the device) with its
   *  current control values — so the same pattern can be added repeatedly with
   *  different params. */
  function addToPlaylist(): void {
    if (!$device || !$devicePatternId) return;
    const id = $devicePatternId;
    const name = $devicePatterns.find((p) => p.id === id)?.name ?? $patternName;
    const itemControls: Record<string, number[]> = {};
    for (const [k, v] of Object.entries($controlValues)) itemControls[k] = v;
    playlist.update((pl) => ({
      ...pl,
      items: [...pl.items, { id, name, sec: null, controls: itemControls }],
    }));
    queuePlaylistSave();
    note("save", "added to playlist", 2000);
  }

  // ---- share links ----

  async function sharePattern(): Promise<void> {
    // a custom map is part of the look — carry its PROGRAM in the link
    const frag = await encodeShare($source, $layout.kind === "map" ? $mapSrc : null);
    history.replaceState(null, "", `#${frag}`);
    const url = location.href;
    try {
      await navigator.clipboard.writeText(url);
      note("share", "link copied", 2500);
    } catch {
      // clipboard needs a secure context — a device over plain http isn't.
      // The dialog's field opens selected, so ⌘/Ctrl-C still works.
      await promptText({
        title: "Copy this link",
        body: "Your browser blocked the clipboard on this page. The link is selected below — copy it, or copy it from the address bar.",
        label: "Link",
        initial: url,
        confirmLabel: "Done",
        cancelLabel: "Close",
        validate: () => null,
      });
      note("share", "link in address bar", 2500);
    }
  }

  // ---- layout editing ----

  function setLayoutKind(e: Event): void {
    const kind = (e.target as HTMLSelectElement).value;
    markRigChosen(); // an explicit pick outranks anything derived (#372)
    if (kind === "map") {
      // "2D map" is how mapping is enabled: reveal + run the map program (the
      // map sub-tab appears because layout.kind becomes "map"). This is the
      // only enable/disable — switching back to strip/grid turns it off.
      subTab = "map";
      if (!mapRef.hasEngine()) mapRef.recompile(false);
      mapRef.run(); // the install event flips layout → map on success
      if ($layout.kind !== "map") recompile();
      return;
    }
    subTab = "pattern";
    // on a device the pixel count is fixed by hardware; layout only rearranges
    const total = $device ? $devicePixels : pixelCount();
    if (kind === "strip") {
      layout.set({ kind: "strip", pixels: total });
    } else if (kind === "grid") {
      const side = Math.max(2, Math.round(Math.sqrt(total)));
      layout.set({ kind: "grid", w: side, h: side });
    }
    recompile(); // rebuild the local preview (a layout change never pushes)
  }

  function setLayoutNum(field: "pixels" | "w" | "h", e: Event): void {
    markRigChosen(); // hand-tuned geometry is an explicit pick too (#372)
    const v = Math.max(1, Math.min(4096, Number((e.target as HTMLInputElement).value) || 1));
    const l = $layout;
    if (l.kind === "strip" && field === "pixels") layout.set({ ...l, pixels: v });
    if (l.kind === "grid" && (field === "w" || field === "h")) {
      layout.set({ ...l, [field]: v });
    }
    recompile(); // rebuild the local preview (a layout change never pushes)
  }

  function onMapInstall(e: CustomEvent<{ coords: number[][]; dims: number }>): void {
    layout.set({ kind: "map", coords: e.detail.coords });
    recompile(); // local preview only — a layout change never pushes to the device
  }

  /** Install the current computed map on the device (device patterns then
   *  render2D with this geometry). */
  function installDeviceMap(): void {
    const l = $layout;
    if (!$device || l.kind !== "map") return;
    const coords = l.coords;
    const dims = (coords[0]?.length ?? 2) >= 3 ? 3 : 2;
    void (async () => {
      if (await installDeviceMapCoords(dims, coords)) {
        note("save", "map installed on the device", 2500);
      }
    })();
  }

  /** Install the current grid layout on the device as a procedural grid —
   *  no coordinates cross the wire, nothing is allocated on the device. */
  function installDeviceGrid(): void {
    const l = $layout;
    if (!$device || l.kind !== "grid") return;
    const { w, h } = l;
    void (async () => {
      if (await installDeviceGridMap(w, h)) {
        note("save", `${w}×${h} grid installed on the device`, 2500);
      }
    })();
  }

  function onClearDeviceMap(): void {
    void (async () => {
      await clearDeviceMap();
      note("save", "device map cleared", 2000);
    })();
  }

  // ---- transport ----

  function onFpsChange(e: Event): void {
    targetFps = Number((e.target as HTMLSelectElement).value);
  }

  function togglePause(): void {
    running = !running;
    lastT = 0; // don't integrate the paused time into delta
  }

  function onControlSet(e: CustomEvent<{ name: string; values: number[] }>): void {
    engine?.setControl(e.detail.name, e.detail.values); // drives the local preview
    if ($device) void $device.setControl(e.detail.name, e.detail.values); // and the strip
  }

  // ---- debugger ----

  /** Install breakpoints and echo the VM-resolved lines back into the
   *  gutter — a click on a comment/blank line snaps to the next executable
   *  line, so the dot always sits where execution will actually stop. */
  function applyBreakpoints(): void {
    if (!engine) return;
    const resolved = [...new Set(engine.setBreakpoints(breakpoints))].sort((a, b) => a - b);
    const current = [...new Set(breakpoints)].sort((a, b) => a - b);
    if (JSON.stringify(resolved) !== JSON.stringify(current)) {
      breakpoints = resolved;
      editor?.setBreakpointLines(resolved);
    }
  }

  function onBreakpoints(e: CustomEvent<number[]>): void {
    breakpoints = e.detail;
    // The device has no stepping debugger — breakpoints must not arm debug
    // mode while connected (it would show a UI that can't actually break).
    if ($device) return;
    if (breakpoints.length > 0 && !debugMode) {
      toggleDebug(); // placing a breakpoint arms the debugger
    } else if (debugMode) {
      applyBreakpoints();
    }
  }

  function toggleDebug(): void {
    debugMode = !debugMode;
    if (!engine) return;
    engine.debugEnable(debugMode);
    if (debugMode) {
      applyBreakpoints();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
    }
  }

  function onPausedRefresh(): void {
    if (!engine) return;
    dbg = engine.debugState();
    editor?.setCurrentLine(dbg.line ?? null);
    preview?.draw(engine.pixels());
  }

  function step(kind: StepKind): void {
    if (!engine || !dbg.paused) return;
    const still = engine.debugStep(kind);
    if (still) {
      onPausedRefresh();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      preview?.draw(engine.pixels());
    }
  }

  function requestBreak(): void {
    engine?.debugPause(); // takes effect at the next executed instruction
  }

  function fmtRaw(raw: number): string {
    return (raw / 65536).toFixed(4).replace(/\.?0+$/, "") || "0";
  }

  function fmtLocal(l: { raw?: number; array?: number; fn?: number }): string {
    if (l.raw !== undefined) return fmtRaw(l.raw);
    if (l.array !== undefined) return `array[${l.array}]`;
    return `fn#${l.fn}`;
  }

  /** Hover inspection: paused → locals (shadowing globals) then globals;
   *  running → live globals. */
  function hoverValue(name: string): string | null {
    if ($device) {
      const v = vars[name];
      return typeof v === "number" ? v.toFixed(4).replace(/\.?0+$/, "") || "0" : null;
    }
    if (!engine) return null;
    if (dbg.paused) {
      const local = dbg.stack?.[0]?.locals.find((l) => l.name === name);
      if (local) return fmtLocal(local);
      const g = dbg.globals?.find((g) => g.name === name);
      return g ? fmtLocal(g) : null;
    }
    const g = engine.globals().find((g) => g.name === name);
    return g ? fmtLocal(g) : null;
  }

  /** Diagnostic spans are UTF-8 byte offsets; CodeMirror wants char offsets. */
  function byteToChar(text: string, byte: number): number {
    const bytes = new TextEncoder().encode(text);
    return new TextDecoder().decode(bytes.subarray(0, Math.min(byte, bytes.length))).length;
  }

  // keep the squiggle in sync with the compile status
  $: if (editor) {
    if (compileError && compileError.start !== undefined && compileError.end !== undefined) {
      editor.setErrorRange({
        from: byteToChar($source, compileError.start),
        to: byteToChar($source, compileError.end),
        message: compileError.message,
      });
    } else {
      editor.setErrorRange(null);
    }
  }

  function jumpToError(): void {
    if (compileError) editor.jumpTo(compileError.line, compileError.col);
  }

  // ---- microphone → sensor patterns (frequencyData etc.) ----
  const mic = new MicSource();
  let micOn = false;
  let sensorInFlight = false;
  let lastSensorPush = 0;

  function toggleMic(): void {
    if (micOn) {
      mic.stop();
      micOn = false;
      return;
    }
    void (async () => {
      try {
        await mic.start();
        micOn = true;
        clearNote("mic");
      } catch {
        note("mic", "microphone unavailable", 4000);
      }
    })();
  }

  // ---- external event injection (preview clicks → readEvent patterns) ----

  const EV_POINTER = 1; // event type carried in [type, x, y, value]
  let evQueue: [number, number, number, number][] = [];
  let evFlushTimer: ReturnType<typeof setTimeout> | null = null;

  function onInject(e: CustomEvent<{ x: number; y: number }>): void {
    engine?.pushEvent(EV_POINTER, e.detail.x, e.detail.y, 1);
    if (!$device) return;
    // batch drags into one POST per ~50 ms instead of a request per move
    evQueue.push([EV_POINTER, e.detail.x, e.detail.y, 1]);
    if (!evFlushTimer) evFlushTimer = setTimeout(flushEvents, 50);
  }

  function flushEvents(): void {
    evFlushTimer = null;
    const batch = evQueue;
    evQueue = [];
    if ($device && batch.length) void $device.sendEvents(batch).catch(() => {});
  }

  // ---- pin injection (pin panel → digitalRead / analogRead) ----

  /** Re-read which pins the pattern touches and what they currently read.
   *  Polled rather than computed once: a `pinMode` at top level shows up at
   *  compile time, but a pin only ever named in `digitalRead` inside
   *  `beforeRender` isn't known until the pattern has run a frame. */
  function refreshPins(): void {
    if (!engine) {
      pins = [];
      return;
    }
    const next = engine.pinsUsed();
    // reassign only on a real change — `pins` keys an `{#each}`
    if (next.length !== pins.length || next.some((p, i) => p !== pins[i])) pins = next;
    const levels: Record<number, boolean> = {};
    const idle: Record<number, boolean> = {};
    for (const p of pins) {
      levels[p] = engine.pinRead(p);
      idle[p] = engine.pinIdleHigh(p);
    }
    pinLevels = levels;
    pinIdleHigh = idle;

    const nextAnalog = engine.analogPinsUsed();
    if (nextAnalog.length !== analogPins.length || nextAnalog.some((p, i) => p !== analogPins[i])) {
      analogPins = nextAnalog;
    }
    // The engine is the source of truth for a slider's position too — a pin
    // the pattern only just started sampling has never been driven, so it
    // reads 0 and the slider must show 0 rather than a stale value.
    const analog: Record<number, number> = {};
    for (const p of analogPins) analog[p] = engine.analogRead(p);
    analogValues = analog;
  }

  /** A press/latch from the pin panel. `level: null` releases the pin back to
   *  its idle level (HIGH under a pull-up) — the injection ABI's driven-vs-idle
   *  model, not a plain HIGH/LOW toggle. Preview-only by design: on a device
   *  the same pins are real pads the firmware syncs every frame (Gitea #177
   *  item 4), so an injected level would be overwritten by the wire. */
  function onPinDrive(e: CustomEvent<{ pin: number; level: boolean | null }>): void {
    engine?.setPin(e.detail.pin, e.detail.level);
    refreshPins();
  }

  /** A slider move from the pin panel: park the analog pin at `value` (0..1),
   *  which is what `analogRead`/`touchRead` then report until it is moved
   *  again (Gitea #206). Preview-only for the same reason as `onPinDrive`. */
  function onAnalogDrive(e: CustomEvent<{ pin: number; value: number }>): void {
    engine?.setAnalogPin(e.detail.pin, e.detail.value);
    refreshPins();
  }

  // ---- render loop ----

  function loop(t: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine || !running) return;
    if (dbg.paused) return; // suspended at a debug stop — step buttons drive
    const minInterval = targetFps > 0 ? 1000 / targetFps - 1 : 0;
    if (lastT !== 0 && t - lastT < minInterval) return;
    const dt = lastT === 0 ? 1000 / (targetFps || 60) : Math.min(t - lastT, 200);
    lastT = t;
    if (micOn) {
      const sf = mic.frame();
      engine.setSensors(sf);
      // in device mode, the mic also stands in for the physical sensor
      // board: stream frames to the strip (throttled, one in flight)
      const d = $device;
      if (d && !sensorInFlight && t - lastSensorPush > 50) {
        lastSensorPush = t;
        sensorInFlight = true;
        d.sendSensors(toSensorBoardFrame(sf))
          .catch(() => {})
          .finally(() => (sensorInFlight = false));
      }
    }
    const px = engine.frame(dt);
    if (engine.debugPaused()) {
      onPausedRefresh();
      return;
    }
    preview?.draw(px);
    previewFps.update((f) => f * 0.9 + (1000 / Math.max(dt, 1)) * 0.1);
    const err = engine.takeError();
    if (err) runtimeError.set(err);
    if (t - lastPoll > 250) {
      lastPoll = t;
      vars = engine.vars();
      const r = new Map<string, number>();
      for (const c of controls) {
        if (c.kind === "showNumber" || c.kind === "gauge") {
          const v = engine.setControl(c.name, []);
          if (v !== null) r.set(c.name, v);
        }
      }
      readouts = r;
      refreshPins();
    }
  }

  /** Editor keyboard shortcuts: ⌘/Ctrl+S save, ⌘/Ctrl+Enter force run + push. */
  function onKeydown(e: KeyboardEvent): void {
    if (!active) return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void saveToLibrary();
    } else if (mod && e.key === "Enter") {
      e.preventDefault();
      applyEdit(); // recompile the preview + push to the device
    }
  }

  export function clearPreview(): void {
    preview?.clear();
  }

  onDestroy(() => {
    cancelAnimationFrame(raf);
    clearTimeout(debounce);
    clearTimeout(pushDebounce);
    engine?.free();
    mapRef?.free();
    mic.stop();
  });
</script>

<svelte:window on:click={() => (menuOpen = false)} on:keydown={onKeydown} />

<main class="editor-view" hidden={!active}>
  {#if patternLoading}
    <!-- cover the editor while a pattern is being fetched/activated so the
         previously-open script never flashes before the real one loads -->
    <div class="pattern-loading" data-role="pattern-loading">
      <span class="spinner"></span>
      {$device ? "loading the pattern from the device…" : "loading pattern…"}
    </div>
  {/if}
  <section class="left">
    <!-- File actions live in a toolbar fixed above the editor (not in the
         header next to the device connection) — they act on the pattern
         being edited. -->
    <div class="editor-toolbar" data-role="editor-toolbar">
      <button
        data-role="save"
        title={$device ? "save the current pattern on the device" : "save to this browser's library"}
        on:click={() => void saveToLibrary()}
      >
        save
      </button>
      {#if $devicePatternId !== "" || $saved.some((s) => s.name === $patternName && $exampleName === "")}
        <button
          data-role="delete"
          title={$devicePatternId ? "remove from the device" : "remove from the library"}
          on:click={() => void deleteSaved()}
        >
          delete
        </button>
      {/if}
      {#if $isPlayground}
        <button
          data-role="share"
          class="primary"
          title="copy a link that carries this pattern in the URL"
          on:click={() => void sharePattern()}
        >
          share
        </button>
      {/if}
      {#if $device && $devicePatternId}
        <button
          data-role="add-to-playlist"
          title="add this pattern (with its current parameters) to the playlist"
          on:click={addToPlaylist}
        >
          + playlist
        </button>
      {/if}
      <span class="overflow">
        <button
          class="more"
          data-role="overflow"
          title="more actions"
          aria-label="more actions"
          on:click|stopPropagation={() => (menuOpen = !menuOpen)}
        >
          ⋯
        </button>
        {#if menuOpen}
          <div class="menu" role="menu">
            <button data-role="epe-import" role="menuitem" on:click={() => fileInput.click()}>
              import .epe…
            </button>
            <button data-role="epe-export" role="menuitem" on:click={doExportEpe}>
              export .epe
            </button>
          </div>
        {/if}
      </span>
      {#if $notes.save}<span class="dim note" data-role="save-note">{$notes.save}</span>{/if}
      {#if $notes.share}<span class="dim note" data-role="share-note">{$notes.share}</span>{/if}
      <input
        class="file-input"
        type="file"
        accept=".epe,.json,application/json"
        bind:this={fileInput}
        on:change={onImportPick}
      />
    </div>

    {#if $layout.kind === "map"}
      <div class="subtabs" data-role="editor-subtabs">
        <button
          data-role="subtab-pattern"
          class="subtab"
          class:active={subTab === "pattern"}
          on:click={() => (subTab = "pattern")}
        >
          pattern
        </button>
        <button
          data-role="subtab-map"
          class="subtab"
          class:active={subTab === "map"}
          on:click={() => {
            subTab = "map";
            if (!mapRef.hasEngine()) mapRef.recompile(!mapDebugMode);
          }}
        >
          map
        </button>
      </div>
    {/if}

    <div class="editor-host">
      <div class="editor-slot" hidden={subTab !== "pattern"}>
        <CodeEditor
          bind:this={editor}
          value={$source}
          {hoverValue}
          on:change={onSourceChange}
          on:paste={markSourcePasted}
          on:breakpoints={onBreakpoints}
        />
      </div>
      <MapEditor
        bind:this={mapRef}
        bind:compileError={mapCompileError}
        bind:debugMode={mapDebugMode}
        bind:dbg={mapDbg}
        showPane={$layout.kind === "map"}
        visible={subTab === "map"}
        liveApply={$layout.kind === "map"}
        on:install={onMapInstall}
      />
    </div>

    <div class="playback">
      <!-- Layout controls how the preview is arranged. In the playground it
           also sets how many pixels the local engine runs; on a device the
           pixel count is fixed by hardware and this only rearranges the live
           stream (strip row / grid / 2D map) — "2D map" is a local preview
           aid, not uploaded to the device. Choosing "2D map" reveals the
           pattern·map sub-tabs and runs the map program. -->
      <select value={$layout.kind} data-role="layout-kind" on:change={setLayoutKind}>
        <option value="strip">strip</option>
        <option value="grid">grid</option>
        <option value="map">2D map</option>
      </select>
      {#if $layout.kind === "strip"}
        <input
          class="num"
          data-role="layout-px"
          type="number"
          min="1"
          max="4096"
          value={$layout.pixels}
          disabled={!$isPlayground}
          title={$isPlayground ? "pixel count" : "fixed by the device's hardware"}
          on:change={(e) => setLayoutNum("pixels", e)}
        />
        <span class="dim">px</span>
      {:else if $layout.kind === "grid"}
        <input
          class="num"
          data-role="layout-w"
          type="number"
          min="1"
          max="256"
          value={$layout.w}
          on:change={(e) => setLayoutNum("w", e)}
        />
        <span class="dim">×</span>
        <input
          class="num"
          data-role="layout-h"
          type="number"
          min="1"
          max="256"
          value={$layout.h}
          on:change={(e) => setLayoutNum("h", e)}
        />
        {#if $device}
          <button
            data-role="grid-install"
            title="tell the device it is a {$layout.w}×{$layout.h} grid so its patterns render in 2D (no coordinates uploaded, nothing allocated on the device)"
            on:click={installDeviceGrid}
          >
            install grid on device
          </button>
          {#if $deviceMap.installed}
            <span class="dim mono" data-role="map-installed">{$deviceMap.count}px {$deviceMap.dims}D on device</span>
          {/if}
        {/if}
      {:else}
        <span class="dim mono" data-role="map-badge">{$layout.coords.length} px mapped</span>
      {/if}
      {#if subTab === "map"}
        <button data-role="map-run" title="run the map program and install it" on:click={() => mapRef.run()}>
          run map
        </button>
        {#if $device && $layout.kind === "map"}
          <button
            data-role="map-install"
            title="upload this map to the device so its patterns render in 2D/3D"
            on:click={installDeviceMap}
          >
            install on device
          </button>
          {#if $deviceMap.installed}
            <button
              data-role="map-clear"
              title="remove the map from the device"
              on:click={onClearDeviceMap}
            >
              clear device map
            </button>
            <span class="dim mono" data-role="map-installed">
              {$deviceMap.count}px {$deviceMap.dims}D on device
            </span>
          {/if}
        {/if}
        {#if $notes.map}<span class="mapper-error" data-role="map-error">{$notes.map}</span>{/if}
      {/if}
      <span class="sep"></span>
      <select value={targetFps} on:change={onFpsChange}>
        <option value={0}>max fps</option>
        <option value={60}>60 fps</option>
        <option value={30}>30 fps</option>
        <option value={15}>15 fps</option>
        <option value={5}>5 fps</option>
      </select>
      <button data-role="pause" on:click={togglePause} title={running ? "pause" : "resume"}>
        {running ? "pause" : "play"}
      </button>
      {#if subTab === "map"}
        <button
          class="debug-toggle"
          class:active={mapDebugMode}
          data-role="map-debug"
          title="toggle the map debugger"
          on:click={() => mapRef.toggleDebug()}
        >
          debug
        </button>
      {:else}
        <!-- the preview runs on the local engine (even on a device), so the
             step-debugger works everywhere -->
        <button
          class="debug-toggle"
          class:active={micOn}
          data-role="mic-toggle"
          title="feed microphone audio to sensor patterns (frequencyData, energyAverage, maxFrequency)"
          on:click={toggleMic}
        >
          sound
        </button>
        {#if $notes.mic}<span class="mapper-error" data-role="mic-error">{$notes.mic}</span>{/if}
        <button
          class="debug-toggle"
          class:active={debugMode}
          data-role="debug"
          title="toggle debugger"
          on:click={toggleDebug}
        >
          debug
        </button>
      {/if}
    </div>
  </section>
  <section class="right">
    <!-- Conditions that persist until whatever caused them goes away: the
         wasm failed to load, the device is unreachable. Pushed by whoever
         knows (the shell, the push path) rather than derived here. -->
    {#each $banners as b (b.id)}
      <div class="banner" class:error={b.level === "error"} class:warn={b.level === "warn"} data-role={b.role}>
        {b.text}
      </div>
    {/each}
    <!-- Capacity (Gitea #15). Severity follows CERTAINTY, not size: the
         device's own rejection is a fact and reads as an error; our local
         model is advice and reads as a warning. Both are non-blocking —
         the pattern keeps previewing locally either way. -->
    {#if deviceRejectedForSize}
      <div class="banner error" data-role="capacity-rejected">
        the device rejected this pattern: {deviceRejectedForSize}
      </div>
    {:else if capacity}
      <div
        class="banner warn"
        class:capacity-over={capacity.level === "over"}
        data-role="capacity-warning"
        data-level={capacity.level}
        title={capacity.detail}
      >
        {capacity.level === "over" ? "⚠" : "△"}
        {capacity.text}
      </div>
    {/if}
    {#if importError}
      <div class="banner error" data-role="import-error">
        {importError}
        <button class="dismiss" on:click={() => (importError = "")}>×</button>
      </div>
    {/if}
    {#if compileError && subTab === "pattern"}
      <button class="banner error as-button" on:click={jumpToError}>
        line {compileError.line}:{compileError.col} — {compileError.message}
      </button>
    {/if}
    {#if mapCompileError && subTab === "map"}
      <button
        class="banner error as-button"
        data-role="map-compile-error"
        on:click={() => mapRef.jumpToError()}
      >
        map line {mapCompileError.line}:{mapCompileError.col} — {mapCompileError.message}
      </button>
    {/if}
    {#if $runtimeError && !compileError && subTab === "pattern"}
      <div class="banner warn">
        runtime: {$runtimeError.message}
        <button class="dismiss" on:click={() => runtimeError.set(null)}>×</button>
      </div>
    {/if}

    {#if subTab === "map" && mapDebugMode}
      <Debugger
        snapshot={mapDbg}
        runningHint="set a gutter breakpoint, then Run map to step through it"
        on:step={(e) => mapRef.step(e.detail)}
        on:break={() => mapRef.requestBreak()}
      />
    {:else if debugMode && subTab === "pattern"}
      <Debugger snapshot={dbg} on:step={(e) => step(e.detail)} on:break={requestBreak} />
    {/if}

    <div class="preview-wrap">
      <Preview bind:this={preview} layout={$layout} on:inject={onInject} />
    </div>

    <h2>Controls</h2>
    <Controls {controls} bind:values={$controlValues} {readouts} hints={$hints} on:set={onControlSet} />
    {#if controls.length === 0}
      <p class="dim hint">
        export <code>function sliderName(v)</code> to add controls — bound them with
        <code>//# min=0 max=5 step=0.5 default=2</code>
      </p>
    {/if}

    {#if pins.length > 0 || analogPins.length > 0}
      <h2>Pins</h2>
      <PinPanel
        {pins}
        {analogPins}
        levels={pinLevels}
        idleHigh={pinIdleHigh}
        bind:latched={pinLatched}
        bind:analogValues
        on:drive={onPinDrive}
        on:analog={onAnalogDrive}
      />
      <p class="dim hint">
        {#if pins.length > 0}<code>press</code> drives the pin while held; <code>hold</code> keeps
          it driven after you let go. Releasing both returns the pin to its
          <code>pinMode</code> idle level.{/if}{#if analogPins.length > 0}{" "}
          An analog slider is what <code>analogRead</code>/<code>touchRead</code> read on that pin,
          0..1 — it stays where you leave it.{/if}{#if $device}{" "}
          Drives the local preview only: on the device these pins are real GPIO, read from and
          written to the pads every frame.{/if}
      </p>
    {/if}

    {#if $layout.kind === "map"}
      <h2>Map</h2>
      <p class="dim hint">
        A {$pixelTotal}-point map is installed. Edit it in the
        <button class="link" data-role="goto-map" on:click={() => (subTab = "map")}>map</button>
        sub-tab — it's a debuggable Luxel program (<code>plot(x, y)</code> per pixel).{" "}
        {#if $device}It only arranges this preview — it isn't uploaded to the device.{/if} Choose
        a different layout to turn mapping off.
      </p>
    {/if}

    <h2>Vars</h2>
    <VarWatcher {vars} />
    {#if Object.keys(vars).length === 0}
      <p class="dim hint">export <code>var name</code> to watch values here</p>
    {/if}
  </section>
</main>

<style>
  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .editor-view[hidden] {
    display: none;
  }

  .editor-view {
    position: relative;
    display: grid;
    grid-template-columns: minmax(360px, 1fr) minmax(320px, 420px);
    flex: 1;
    min-height: 0;
  }

  .pattern-loading {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-dim);
    background: var(--bg);
  }

  .dim {
    color: var(--text-dim);
  }

  .num {
    width: 64px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .left {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .note {
    font-size: 12px;
  }

  .editor-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }

  .editor-toolbar .primary {
    border-color: var(--accent);
    color: var(--accent);
  }

  .editor-toolbar .note {
    margin-left: auto;
  }

  .overflow {
    position: relative;
    display: inline-flex;
  }

  .more {
    padding: 2px 8px;
    line-height: 1;
  }

  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 140px;
    padding: 4px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-panel);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  }

  .menu button {
    background: transparent;
    border: none;
    border-radius: 4px;
    text-align: left;
    padding: 6px 8px;
    font-size: 12px;
    cursor: pointer;
    color: var(--text);
  }

  .menu button:hover {
    background: var(--bg-inset);
  }

  .subtabs {
    display: flex;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }

  .subtab {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 5px 10px;
    color: var(--text-dim);
    font-size: 12px;
    cursor: pointer;
  }

  .subtab:hover {
    color: var(--text);
  }

  .subtab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .editor-host {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .editor-slot {
    height: 100%;
  }

  .editor-slot[hidden] {
    display: none;
  }

  .playback {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-top: 1px solid var(--border);
    background: var(--bg-panel);
    flex-wrap: wrap;
  }

  .playback .sep {
    width: 1px;
    align-self: stretch;
    margin: 2px 4px;
    background: var(--border);
  }

  .link {
    background: transparent;
    border: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
    font: inherit;
  }

  .right {
    padding: 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--bg-panel);
  }

  h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-dim);
    margin: 10px 0 2px;
  }

  .banner {
    padding: 8px 10px;
    border-radius: 6px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
    text-align: left;
  }

  .banner.error {
    background: color-mix(in srgb, var(--error) 18%, transparent);
    border: 1px solid var(--error);
    color: #f2b8b8;
  }

  .banner.warn {
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    border: 1px solid var(--warn);
    color: #ecd9a8;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* "will not fit" vs "getting close" — same amber family (both are
     predictions, not facts; the device's own rejection is the red one),
     separated by weight rather than hue. */
  .banner.warn.capacity-over {
    background: color-mix(in srgb, var(--warn) 24%, transparent);
    font-weight: 600;
  }

  .as-button {
    cursor: pointer;
    width: 100%;
  }

  .dismiss {
    margin-left: auto;
    border: none;
    background: transparent;
    padding: 0 4px;
  }

  .hint {
    font-size: 12px;
    margin: 2px 0;
  }

  .debug-toggle.active {
    border-color: var(--accent);
    color: var(--accent);
  }

  .file-input {
    display: none;
  }

  .mapper-error {
    color: var(--error);
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .preview-wrap {
    position: relative;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: conn-spin 0.7s linear infinite;
  }

  @keyframes conn-spin {
    to {
      transform: rotate(1turn);
    }
  }
</style>
