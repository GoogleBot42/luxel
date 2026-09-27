<script lang="ts">
  // `Import image…` — the trigger, the file input, and the fit dialog behind
  // them (Gitea #784). ONE component, mounted by both entry points (the
  // Sprites tab and the sprite editor) so the two cannot drift, and by the
  // console and the playground alike: the pipeline is entirely client-side, so
  // there is nothing device-shaped about it.
  //
  // Drag-and-drop is the SAME path: the page wires its own
  // `dragover`/`drop` (it owns the panel and the drop highlight) and hands the
  // file here through [`offerFiles`], which is what the file input calls too.
  //
  // The result never goes near the store. It is STAGED (`stageSprite`) and the
  // editor adopts it as an unsaved document, so an import can be touched up
  // before Save — #784: "land in the editor (not straight into the store)".
  //
  // `dataRole` must DIFFER per mount (`sprite-import` on the tab,
  // `sprite-editor-import` in the editor): both screens are in the DOM at once,
  // so a shared role makes the hidden one's file input the first match and a
  // harness uploads into a screen nobody can see. The DIALOG's roles are
  // shared on purpose — only one instance can hold a `source` at a time.
  import { createEventDispatcher } from "svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import { decodeImageFile, IMPORT_ACCEPT, NOT_AN_IMAGE, type DecodedImage } from "../../lib/imageDecode";
  import type { Sprite } from "../../lib/sprite";
  import { note } from "../../stores/notify";

  /** The button's words. The editor's `⋯` menu passes its own. */
  export let label = "Import image…";
  export let cls = "btn";
  export let dataRole = "sprite-import";
  /** Render the trigger. `false` = this instance is only the machinery (the
   *  hidden input, the dialog, the drop path) and something else calls
   *  [`pick`] — which is what an overflow-menu item does, because it has to
   *  close the menu on the same click. */
  export let trigger = true;
  /** The fixture the sprite is for — the dialog's default target size. */
  export let panel: { w: number; h: number } | undefined = undefined;
  export let maxBytes = 16 * 1024;

  const dispatch = createEventDispatcher<{ import: Sprite }>();

  let input: HTMLInputElement | undefined;
  let source: DecodedImage | null = null;
  let busy = false;

  /** Open the file chooser. */
  export function pick(): void {
    note("sprite", "");
    input?.click();
  }

  /**
   * Decode `files[0]` and open the fit dialog. The ONE entry point: the file
   * input and every drop target go through it, so a refusal reads the same
   * way whichever way the file arrived.
   */
  export async function offerFiles(files: FileList | readonly File[] | null): Promise<void> {
    const file = files === null ? undefined : files[0];
    if (!file) return;
    note("sprite", "");
    busy = true;
    try {
      source = await decodeImageFile(file, file.name);
    } catch (e) {
      source = null;
      const why = e instanceof Error && e.message !== "" ? e.message : NOT_AN_IMAGE;
      // The refusal goes on the `sprite` note channel, which both screens that
      // mount this component show. An inline error here would have to live
      // inside the page bar it is a flex item of.
      note("sprite", `sprite: ${file.name}: ${why}`, 12000);
    } finally {
      busy = false;
    }
  }

  function onChange(e: Event): void {
    const el = e.currentTarget as HTMLInputElement;
    void offerFiles(el.files).then(() => {
      // so picking the SAME file twice in a row fires `change` again
      el.value = "";
    });
  }

  function onImport(s: Sprite): void {
    source = null;
    dispatch("import", s);
  }
</script>

{#if trigger}
  <button
    class={cls}
    data-role={dataRole}
    disabled={busy}
    data-reason={busy ? "reading the image" : null}
    on:click={pick}>{busy ? "Reading…" : label}</button
  >
{/if}

<!-- hidden, and driven only through the button: a bare file input is the one
     control this app cannot style (and puppeteer uploads to it directly). -->
<input
  type="file"
  accept={IMPORT_ACCEPT}
  hidden
  bind:this={input}
  data-role={`${dataRole}-file`}
  on:change={onChange}
/>

<ImportDialog
  {source}
  {panel}
  {maxBytes}
  on:import={(e) => onImport(e.detail)}
  on:close={() => (source = null)}
/>
