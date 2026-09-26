<script lang="ts">
  // THE frame strip (Gitea #741). Jeremy: "this text makes no sense or it is
  // just broken. I set frames to 2 and 3 and saw nothing change: 'animation
  // frames — a frame strip appears at 2+'". It was broken: the number field
  // clamped 1..64, `onCell` hard-coded frame 0, `fps` was never editable, and
  // NO FRAME STRIP EXISTED ANYWHERE. This is the strip.
  //
  // Frames are managed HERE and nowhere else — the inspector's Frames row is a
  // read-only count now, because a number field that resizes an animation is
  // exactly the control that did nothing.
  import { createEventDispatcher } from "svelte";
  import SpriteThumb from "./SpriteThumb.svelte";
  import { SPRITE_MAX_FRAMES, type Sprite } from "../../lib/sprite";

  export let sprite: Sprite;
  export let frame = 0;
  /** Playing the animation at the sprite's own fps (space toggles it). */
  export let playing = false;
  export let onion = false;

  const dispatch = createEventDispatcher<{
    select: number;
    add: number;
    duplicate: number;
    remove: number;
    move: { from: number; to: number };
    playing: boolean;
    onion: boolean;
  }>();

  $: frames = Array.from({ length: sprite.frames }, (_, i) => i);
  $: full = sprite.frames >= SPRITE_MAX_FRAMES;
  $: still = sprite.fps === 0 || sprite.frames < 2;

  const frameCapReason = `a sprite holds at most ${SPRITE_MAX_FRAMES} frames`;
  const lastFrameReason = "a sprite always has at least one frame";
  const firstFrameReason = "this is already the first frame";
  const lastPositionReason = "this is already the last frame";
  const stillReason = "set FPS above 0 and add a second frame to animate";
</script>

<div class="strip" data-role="sprite-frames">
  <div class="shead">
    <div class="slabel">Frames</div>
    <div class="rdim" data-role="sprite-frame-count">
      {frame + 1} of {sprite.frames}
    </div>
    <div class="grp">
      <!-- Play is ABSENT of meaning on a still: a one-frame or 0-fps sprite has
           nothing to cycle, so the button says why instead of lying. It is the
           ONE disabled control here and it carries its reason
           (.claude/rules/web.md). -->
      <button
        class="btn sm"
        class:on={playing}
        data-role="sprite-play"
        disabled={still}
        data-reason={still ? stillReason : null}
        title={still ? stillReason : "play at this sprite's fps (space)"}
        aria-pressed={playing}
        on:click={() => dispatch("playing", !playing)}>{playing ? "Pause" : "Play"}</button
      >
      <button
        class="btn sm"
        class:on={onion}
        data-role="sprite-onion"
        aria-pressed={onion}
        title="ghost the previous frame under this one"
        on:click={() => dispatch("onion", !onion)}>Onion</button
      >
    </div>
  </div>

  <div class="frames">
    {#each frames as f (f)}
      <div class="fr" class:cur={f === frame} data-role="sprite-frame" data-frame={f}>
        <button
          class="fbtn"
          data-role="sprite-frame-pick"
          aria-current={f === frame}
          title={`frame ${f + 1}`}
          on:click={() => dispatch("select", f)}
        >
          <SpriteThumb {sprite} size={56} frame={f} playing={false} dataRole="sprite-frame-thumb" />
        </button>
        <div class="fnum">{f + 1}</div>
      </div>
    {/each}

    <button
      class="addfr"
      data-role="sprite-frame-add"
      title="add a copy of this frame after it"
      aria-label="add a frame"
      disabled={full}
      data-reason={full ? frameCapReason : null}
      on:click={() => dispatch("add", frame)}>+</button
    >
  </div>

  <!-- The ops act on the CURRENT frame, in one row under the strip rather
       than as four 13px glyphs crammed under a 58px thumbnail — #741 is a
       complaint about "tiny confusing buttons" and four of those under every
       frame would be the same mistake twice. Words, not symbols, for the same
       reason. Every disabled state carries its REASON on `data-reason`:
       `disabledSweep` fails a control that is disabled without one, and a
       limit you cannot see is a bug report (.claude/rules/web.md). -->
  <div class="fops" data-role="sprite-frame-ops">
    <button
      class="btn sm"
      data-role="sprite-frame-left"
      title={frame === 0 ? firstFrameReason : "move this frame one earlier"}
      disabled={frame === 0}
      data-reason={frame === 0 ? firstFrameReason : null}
      on:click={() => dispatch("move", { from: frame, to: frame - 1 })}>← Move</button
    >
    <button
      class="btn sm"
      data-role="sprite-frame-duplicate"
      title={full ? frameCapReason : "insert a copy of this frame after it"}
      disabled={full}
      data-reason={full ? frameCapReason : null}
      on:click={() => dispatch("duplicate", frame)}>Duplicate</button
    >
    <button
      class="btn sm fdel"
      data-role="sprite-frame-delete"
      title={sprite.frames <= 1 ? lastFrameReason : "delete this frame"}
      disabled={sprite.frames <= 1}
      data-reason={sprite.frames <= 1 ? lastFrameReason : null}
      on:click={() => dispatch("remove", frame)}>Delete</button
    >
    <button
      class="btn sm"
      data-role="sprite-frame-right"
      title={frame === sprite.frames - 1 ? lastPositionReason : "move this frame one later"}
      disabled={frame === sprite.frames - 1}
      data-reason={frame === sprite.frames - 1 ? lastPositionReason : null}
      on:click={() => dispatch("move", { from: frame, to: frame + 1 })}>Move →</button
    >
  </div>
</div>

<style>
  .strip {
    margin-top: 12px;
  }

  .shead {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .shead .grp {
    margin-left: auto;
    display: inline-flex;
    gap: 4px;
  }

  /* wide content scrolls inside its OWN container, never the page
     (.claude/rules/web.md) — thirty frames is a long strip */
  .frames {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    overflow-x: auto;
    padding-bottom: 4px;
  }

  .fr {
    flex: none;
    width: 58px;
    text-align: center;
  }

  .fbtn {
    display: block;
    width: 58px;
    padding: 1px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-inset);
    line-height: 0;
  }

  .fr.cur .fbtn {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .fnum {
    margin-top: 3px;
    font: 10.5px/1 var(--mono);
    color: var(--text-dim);
  }

  .fops {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  /* `fdel`, not the scene inspector's `del`: this screen wears the `scenes`
     class for its design system, and `.scenes .btn.del` is that inspector's
     BORDERLESS bottom-of-the-column "Delete layer" verb (S7). Inheriting it
     here made one of four peer buttons look like a link. */
  .fops .btn.fdel:not([disabled]) {
    color: var(--error);
  }

  .fops .btn.fdel:hover:not([disabled]) {
    border-color: var(--error);
  }

  .addfr {
    flex: none;
    width: 58px;
    height: 58px;
    border: 1px dashed #3d4450;
    border-radius: 4px;
    background: transparent;
    color: var(--text-dim);
    font-size: 20px;
    line-height: 1;
  }

  .addfr:hover:not([disabled]) {
    border-color: var(--accent);
    color: var(--accent);
  }

  /* a finger needs 24px below the phone breakpoint (§5.7) */
  @media (max-width: 600px) {
    .fops .btn {
      min-height: 26px;
    }
  }
</style>
