// What a console does at boot with the working copy the browser was holding
// (Gitea #585). One pure function, because it is a RULE — the same class of
// decision as the playlist transport reconciler (`lib/playlist.ts`), and it is
// worth pinning in `tests/resume.test.mjs` rather than re-deriving it inside a
// boot path nobody can step through.
//
// The rule it encodes is #563's: the editor writes to the device only while
// its document IS the device's running program. At boot that stops being a
// question about clicks and becomes a question about two ids.

/** What `bootDevice` does with the resumed working copy. */
export type BootResume =
  /** Throw the copy away and open what the device is running (live push). */
  | "adopt-running"
  /** Keep the copy — it is an unsaved edit OF the running program, so the
   *  device follows the editor exactly as it did before the reload. */
  | "resume-live"
  /** Keep the copy as a LOCAL PREVIEW document: nothing is sent, and whatever
   *  the device is running keeps running. */
  | "resume-preview";

export interface ResumeInput {
  /** the autosaved copy has genuinely unsaved changes (`WorkingCopy.dirty`) */
  dirty: boolean;
  /** the device pattern the copy is an edit of, "" if none */
  wipPatternId: string;
  /** the pattern the DEVICE is running, "" if it is on an ad-hoc program */
  runningId: string;
}

/**
 * A clean copy defers to the device, as it always has.
 *
 * A DIRTY copy is the user's work and is never thrown away — but it is only
 * pushed when it is an edit of the program the device is already running.
 * Anything else (an edit of a stored pattern that is not playing, a library
 * pick, an import, a share link) resumes in local preview: pushing it would
 * be `playlist::stop()` on the firmware, i.e. a page load that replaces the
 * user's installation before they have touched anything.
 *
 * Two empty ids are NOT a match. Both the ad-hoc program the device is running
 * and an unsaved copy from an earlier session have `""`, so "" === "" would
 * make the commonest resume the one that takes the device over — and the
 * device's current program, which somebody or something chose after that copy
 * was last edited, is the better claim on the LEDs.
 */
export function bootResume({ dirty, wipPatternId, runningId }: ResumeInput): BootResume {
  if (!dirty) return "adopt-running";
  if (wipPatternId !== "" && wipPatternId === runningId) return "resume-live";
  return "resume-preview";
}
