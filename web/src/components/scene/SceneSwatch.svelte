<script lang="ts">
  // The inspector's colour field (mockup S7/S7e `.swatchbtn`): a 14px chip
  // and the colour's NAME, in a 26px-tall bordered pill.
  //
  // The chip IS `components/ColorPicker.svelte` — THE colour control in this
  // app (Jeremy, 2026-09-19: no raw HSV numbers, and not the browser picker
  // either) — resized to the mock's 14px. Reusing it rather than drawing a
  // second popover is what keeps a scene colour and a pattern control colour
  // the same thing to edit.
  import { createEventDispatcher } from "svelte";
  import ColorPicker from "../ColorPicker.svelte";
  import { hexToRgb, rgbToHex } from "../../lib/color";

  /** `rrggbb`, no `#` — the wire's spelling. */
  export let value = "ffffff";
  export let label = "colour";
  export let dataRole = "scene-color";

  const dispatch = createEventDispatcher<{ input: string }>();

  /** The pill, so a click anywhere in it can reach the swatch inside it
   *  (Gitea #736 item 7). Jeremy: "the entire color button (which includes
   *  the name) isn't clickable to change the color, just the color preview
   *  square itself" — the name read as part of the control because it IS
   *  drawn as part of it, and only the 14px chip did anything. */
  let pill: HTMLElement | undefined;

  /**
   * Forward a click on the pill to the picker's own opener.
   *
   * Two things make this the shape it is. The opener must stay ONE button —
   * a second `<button>` nested inside the pill is not legal HTML and the
   * picker's swatch is already the keyboard path — so the pill dispatches a
   * real click on it instead of duplicating the popover's state. And the
   * pill's own handler is `|stopPropagation`: `Popover` closes on any window
   * click that is not inside itself or inside its `anchor`, so without it the
   * label's click would open the popover and the SAME event would go on to
   * dismiss it (.claude/rules/web.md — this exact trap, in this exact file's
   * neighbour).
   */
  function openPicker(e: MouseEvent): void {
    // the swatch handles its own click; re-dispatching would toggle twice
    if ((e.target as HTMLElement | null)?.closest("button.swatch")) return;
    pill?.querySelector<HTMLButtonElement>("button.swatch")?.click();
  }

  $: rgb = hexToRgb(`#${value}`) ?? [1, 1, 1];

  function onInput(e: CustomEvent<number[]>): void {
    const [r, g, b] = e.detail;
    dispatch("input", rgbToHex([r ?? 0, g ?? 0, b ?? 0]).slice(1));
  }

  /** The nearest of the palette's own names — the mock labels a swatch
   *  `white` / `amber`, not `#e8a33d`, because the name is what you read at
   *  12px. Falls back to the hex when nothing is close. */
  const NAMED: [string, number, number, number][] = [
    ["black", 0, 0, 0],
    ["white", 255, 255, 255],
    ["grey", 128, 128, 128],
    ["red", 224, 85, 85],
    ["amber", 232, 163, 61],
    ["yellow", 247, 224, 138],
    ["green", 95, 191, 122],
    ["teal", 15, 111, 106],
    ["blue", 74, 138, 255],
    ["purple", 199, 146, 234],
    ["pink", 194, 58, 107],
  ];

  function colorName(hex: string): string {
    const v = Number.parseInt(hex, 16);
    if (!Number.isFinite(v)) return hex;
    const r = (v >> 16) & 0xff;
    const g = (v >> 8) & 0xff;
    const b = v & 0xff;
    let best = "";
    let bestD = Infinity;
    for (const [name, nr, ng, nb] of NAMED) {
      const d = (r - nr) ** 2 + (g - ng) ** 2 + (b - nb) ** 2;
      if (d < bestD) {
        bestD = d;
        best = name;
      }
    }
    // ~28 per channel; anything further away is better said in hex
    return bestD <= 2400 ? best : `#${hex}`;
  }
</script>

<!-- The whole pill is the control. `role="presentation"` and no key handler
     of its own on purpose: this is a larger hit area for a pointer, and the
     thing it forwards to — the picker's swatch button — is what carries the
     role, the label, `aria-expanded` and the keyboard. -->
<span
  class="swatchbtn"
  bind:this={pill}
  data-role={dataRole}
  data-value={value}
  title={`${label} — #${value}`}
  role="presentation"
  on:click|stopPropagation={openPicker}
>
  <ColorPicker kind="rgb" value={rgb} {label} on:input={onInput} />
  <span class="cname">{colorName(value)}</span>
</span>

<style>
  /* The pill is a button now in everything but the tag name, so it says so:
     a pointer cursor over the whole of it, and the same hover lift the other
     bordered controls have. */
  .swatchbtn {
    cursor: pointer;
  }

  /* the app's one "this is pressable" hover, `button:hover` in app.css */
  .swatchbtn:hover {
    border-color: var(--accent);
  }

  /* the name, no longer a bare text node — it needs to be addressable so the
     click forwarder can tell it from the swatch */
  .cname {
    pointer-events: none;
  }

  /* the mock's `.swatchbtn i` — the picker's own 26x22 swatch, resized */
  .swatchbtn :global(button.swatch) {
    width: 14px;
    height: 14px;
    border-radius: 3px;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }

  /* a finger needs 24px (§5.7, `mockdiff --sweep`); the CHIP is the mock’s
     14px everywhere it is drawn, which is every width above a phone */
  @media (max-width: 600px) {
    .swatchbtn :global(button.swatch) {
      width: 24px;
      height: 24px;
    }
  }
</style>
