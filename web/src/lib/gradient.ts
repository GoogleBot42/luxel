// The colour-ramp editor's MODEL — the parts of
// `components/GradientEditor.svelte` that are rules rather than widgetry.
//
// The #787 bug inventory found that the three places the editor could lose the
// user's work are all decisions with no canvas, no pointer and no device in
// them: whether a removal is allowed, what a trailing commit carries, and what
// an emptied number field means. They live here so `web/tests/gradient.test.mjs`
// can hold them without a DOM, and so the step-2 redesign inherits the rules
// rather than the widget.
//
// Nothing here knows about Svelte or about a stop's colour: the list functions
// are generic over the element type, which is `GradientStop` in the component.

/**
 * Remove stop `i` — or REFUSE (`null`) when the list is already at its floor.
 *
 * Gitea #787 §3: below `minStops` the editor used to fall through to
 * `clearAll()`, so pressing Delete on one of a scene ramp's two stops dropped
 * the layer's whole `Ramp` record. `Edit…` then reseeds black→white, which
 * makes the ramp the user had unrecoverable — no confirmation, no undo. A
 * removal now removes a stop or does nothing; destroying the ramp is `clear`,
 * a separate, named action.
 *
 * The device palette mount passes `minStops = 0` and so keeps its behaviour:
 * its last stop is legitimately removable and an empty palette is a valid
 * state (`DELETE /api/output/palette`).
 *
 * `picked` comes back because the selection has to follow the shortened list.
 */
export function removeStopAt<T>(
  list: readonly T[],
  i: number,
  minStops: number,
): { stops: T[]; picked: number } | null {
  if (!Number.isInteger(i) || i < 0 || i >= list.length) return null;
  if (list.length <= minStops) return null;
  const stops = list.filter((_, n) => n !== i);
  return { stops, picked: Math.max(0, Math.min(i, stops.length - 1)) };
}

/** A pending trailing commit. `pending()` is for tests and assertions. */
export interface TrailingCommit {
  /** (Re)start the timer from now. */
  arm(): void;
  /** Drop a pending commit — a later edit supersedes it. */
  cancel(): void;
  /** Is a commit still in flight? */
  pending(): boolean;
}

/**
 * The trailing commit behind a colour storm (Gitea #787 §2).
 *
 * `ColorPicker` emits on every pointermove across its saturation field, so a
 * colour edit has to commit once the hand STOPS — Settings would otherwise
 * POST the whole palette per mouse move. The defect was what the pending
 * commit carried: the list captured when it was armed. A drag or a Delete
 * inside the 250 ms window was therefore undone 250 ms later — the drag lost,
 * the deleted stop back.
 *
 * Two rules fix it and both are properties of this latch:
 *   1. it commits whatever `current()` says at the moment it FIRES, and
 *   2. any other edit `cancel()`s it, so the later gesture wins.
 */
export function trailingCommit<T>(
  delayMs: number,
  current: () => T,
  commit: (value: T) => void,
): TrailingCommit {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancel = (): void => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  };
  return {
    arm(): void {
      cancel();
      timer = setTimeout(() => {
        timer = undefined;
        commit(current());
      }, delayMs);
    },
    cancel,
    pending: (): boolean => timer !== undefined,
  };
}

/**
 * What a number field the user CLEARED means: nothing (Gitea #787 §4).
 *
 * `Number("")` is 0, and it was unguarded — clearing `position` slammed the
 * stop to 0, and clearing `amount` set the blend to 0, i.e. silently turned
 * the palette off with no message. `null` is "no change", and the caller puts
 * the value in effect back in the box so the field never shows a value the
 * model refused.
 */
export function fieldNumber(raw: string): number | null {
  const t = raw.trim();
  if (t === "") return null;
  const v = Number(t);
  return Number.isFinite(v) ? v : null;
}
