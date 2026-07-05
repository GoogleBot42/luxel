<script lang="ts">
  import { javascript } from "@codemirror/lang-javascript";
  import { RangeSet, StateEffect, StateField } from "@codemirror/state";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { Decoration, GutterMarker, gutter, type DecorationSet } from "@codemirror/view";
  import { EditorView, basicSetup } from "codemirror";
  import { createEventDispatcher, onDestroy, onMount } from "svelte";

  export let value: string;

  const dispatch = createEventDispatcher<{ change: string; breakpoints: number[] }>();
  let host: HTMLDivElement;
  let view: EditorView | undefined;
  // Track the prop stream: only a *change* in the incoming `value` may
  // rewrite the document. Comparing value against the doc is not safe —
  // Svelte flushes this component before the parent echoes our edits back,
  // and the stale prop would revert every keystroke (bug found in e2e).
  let lastReceived = value;

  // ---- breakpoint gutter ----

  const toggleBp = StateEffect.define<number>(); // line-start pos
  class BpMarker extends GutterMarker {
    override toDOM(): Node {
      const el = document.createElement("span");
      el.className = "cm-bp-dot";
      el.textContent = "●";
      return el;
    }
  }
  const bpMarker = new BpMarker();
  const bpField = StateField.define<RangeSet<GutterMarker>>({
    create: () => RangeSet.empty,
    update(set, tr) {
      set = set.map(tr.changes);
      for (const e of tr.effects) {
        if (e.is(toggleBp)) {
          let existing = false;
          set.between(e.value, e.value, () => {
            existing = true;
          });
          set = existing
            ? set.update({ filter: (from) => from !== e.value })
            : set.update({ add: [bpMarker.range(e.value)] });
        }
      }
      return set;
    },
  });
  const bpGutter = gutter({
    class: "cm-bp-gutter",
    markers: (v) => v.state.field(bpField),
    initialSpacer: () => bpMarker,
    domEventHandlers: {
      mousedown(v, block) {
        v.dispatch({ effects: toggleBp.of(block.from) });
        return true;
      },
    },
  });

  function breakpointLines(v: EditorView): number[] {
    const lines: number[] = [];
    v.state.field(bpField).between(0, v.state.doc.length, (from) => {
      lines.push(v.state.doc.lineAt(from).number);
    });
    return lines;
  }

  // ---- current-debug-line highlight ----

  const setDebugLine = StateEffect.define<number | null>();
  const debugLineDeco = Decoration.line({ class: "cm-debug-line" });
  const debugLineField = StateField.define<DecorationSet>({
    create: () => Decoration.none,
    update(deco, tr) {
      deco = deco.map(tr.changes);
      for (const e of tr.effects) {
        if (e.is(setDebugLine)) {
          if (e.value === null || e.value < 1 || e.value > tr.state.doc.lines) {
            deco = Decoration.none;
          } else {
            const line = tr.state.doc.line(e.value);
            deco = Decoration.set([debugLineDeco.range(line.from)]);
          }
        }
      }
      return deco;
    },
    provide: (f) => EditorView.decorations.from(f),
  });

  onMount(() => {
    view = new EditorView({
      doc: value,
      parent: host,
      extensions: [
        bpGutter,
        bpField,
        debugLineField,
        basicSetup,
        javascript(),
        oneDark,
        EditorView.theme({
          "&": { height: "100%", fontSize: "13px" },
          ".cm-scroller": { fontFamily: "ui-monospace, Menlo, Consolas, monospace" },
          ".cm-bp-gutter": { width: "14px", cursor: "pointer" },
          ".cm-bp-dot": { color: "#e05555" },
          ".cm-debug-line": { backgroundColor: "rgba(232, 163, 61, 0.18)" },
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            dispatch("change", u.state.doc.toString());
          }
          if (
            u.docChanged ||
            u.transactions.some((tr) => tr.effects.some((e) => e.is(toggleBp)))
          ) {
            dispatch("breakpoints", breakpointLines(u.view));
          }
        }),
      ],
    });
  });

  onDestroy(() => view?.destroy());

  // external value swaps (example selection) replace the document
  $: if (view && value !== lastReceived) {
    lastReceived = value;
    if (value !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
    }
  }

  export function jumpTo(line: number, col: number): void {
    if (!view) return;
    const l = view.state.doc.line(Math.min(Math.max(line, 1), view.state.doc.lines));
    const pos = Math.min(l.from + Math.max(col - 1, 0), l.to);
    view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
    view.focus();
  }

  /** Highlight (and reveal) the paused line; null clears. */
  export function setCurrentLine(line: number | null): void {
    if (!view) return;
    const effects: StateEffect<unknown>[] = [setDebugLine.of(line)];
    view.dispatch({ effects });
    if (line !== null && line >= 1 && line <= view.state.doc.lines) {
      const pos = view.state.doc.line(line).from;
      view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: "center" }) });
    }
  }
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    height: 100%;
    overflow: hidden;
    border-right: 1px solid var(--border);
  }
</style>
