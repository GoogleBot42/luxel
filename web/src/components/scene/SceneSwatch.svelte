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

<span class="swatchbtn" data-role={dataRole} data-value={value}>
  <ColorPicker kind="rgb" value={rgb} {label} on:input={onInput} />
  {colorName(value)}
</span>

<style>
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
