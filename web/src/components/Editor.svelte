<script lang="ts">
  import {
    autocompletion,
    completeFromList,
    type Completion,
    type CompletionContext,
    type CompletionResult,
  } from "@codemirror/autocomplete";
  import { javascript } from "@codemirror/lang-javascript";
  import { BUILTINS, GLOBALS } from "../lib/builtins";
  import { setDiagnostics } from "@codemirror/lint";
  import { RangeSet, StateEffect, StateField } from "@codemirror/state";
  import { oneDark } from "@codemirror/theme-one-dark";
  import {
    Decoration,
    GutterMarker,
    gutter,
    hoverTooltip,
    type DecorationSet,
  } from "@codemirror/view";
  import { EditorView, basicSetup } from "codemirror";
  import { createEventDispatcher, onDestroy, onMount } from "svelte";

  export let value: string;
  /** Resolve an identifier to a display value for hover inspection. */
  export let hoverValue: ((name: string) => string | null) | undefined = undefined;

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
  const clearBps = StateEffect.define<null>();
  /// replace the whole set (line-start positions) without firing the
  /// breakpoints event — used to echo VM-resolved lines back into the gutter
  const replaceBps = StateEffect.define<number[]>();
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
        if (e.is(clearBps)) {
          set = RangeSet.empty;
        }
        if (e.is(replaceBps)) {
          set = RangeSet.of(e.value.map((pos) => bpMarker.range(pos)));
        }
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

  // ---- hover value inspection ----

  const hoverExt = hoverTooltip(
    (v, pos) => {
      const word = v.state.wordAt(pos);
      if (!word) return null;
      const name = v.state.doc.sliceString(word.from, word.to);
      if (!/^[A-Za-z_$][\w$]*$/.test(name)) return null;
      // builtin/global? show its signature + doc (the autocomplete data)
      const builtin = BUILTINS.find((b) => b.name === name) ?? GLOBALS.find((g) => g.name === name);
      if (builtin) {
        return {
          pos: word.from,
          end: word.to,
          create: () => {
            const dom = document.createElement("div");
            dom.className = "cm-hover-value cm-hover-doc";
            const sig = document.createElement("div");
            sig.className = "cm-hover-sig";
            sig.textContent = builtin.sig;
            const doc = document.createElement("div");
            doc.textContent = builtin.doc;
            dom.append(sig, doc);
            return { dom };
          },
        };
      }
      const val = hoverValue?.(name);
      if (val === null || val === undefined) return null;
      return {
        pos: word.from,
        end: word.to,
        create: () => {
          const dom = document.createElement("div");
          dom.className = "cm-hover-value";
          dom.textContent = `${name} = ${val}`;
          return { dom };
        },
      };
    },
    { hoverTime: 200 },
  );

  // ---- autocomplete ----

  const KEYWORDS = ["var", "let", "const", "function", "export", "return", "if", "else", "for", "while", "true", "false", "break", "continue"];
  const staticCompletions: Completion[] = [
    ...BUILTINS.map((b) => ({
      label: b.name,
      type: "function",
      detail: b.sig.replace(b.name, ""),
      info: b.doc,
      boost: 1,
    })),
    ...GLOBALS.map((g) => ({ label: g.name, type: "constant", info: g.doc })),
    ...KEYWORDS.map((k) => ({ label: k, type: "keyword", boost: -1 })),
  ];
  const builtinSource = completeFromList(staticCompletions);

  /** Identifiers already present in the pattern (user globals/locals). */
  function docWordSource(ctx: CompletionContext): CompletionResult | null {
    const word = ctx.matchBefore(/[A-Za-z_$][\w$]*/);
    if (!word || (word.from === word.to && !ctx.explicit)) return null;
    const seen = new Set<string>();
    const text = ctx.state.doc.toString();
    for (const m of text.matchAll(/[A-Za-z_$][\w$]*/g)) {
      const w = m[0];
      if (w.length > 1 && m.index !== word.from) seen.add(w);
    }
    return {
      from: word.from,
      options: [...seen].map((w) => ({ label: w, type: "variable", boost: -2 })),
      validFor: /^[\w$]*$/,
    };
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
        hoverExt,
        basicSetup,
        // override basicSetup's default completion (JS-scope based) with
        // luxel builtins + document identifiers
        autocompletion({ override: [builtinSource, docWordSource] }),
        javascript(),
        oneDark,
        EditorView.theme({
          "&": { height: "100%", fontSize: "13px" },
          ".cm-scroller": { fontFamily: "ui-monospace, Menlo, Consolas, monospace" },
          ".cm-bp-gutter": { width: "14px", cursor: "pointer" },
          ".cm-bp-dot": { color: "#e05555" },
          ".cm-debug-line": { backgroundColor: "rgba(232, 163, 61, 0.18)" },
          ".cm-hover-value": {
            padding: "3px 8px",
            fontFamily: "ui-monospace, Menlo, Consolas, monospace",
            fontSize: "12px",
          },
          ".cm-hover-doc": { maxWidth: "360px" },
          ".cm-hover-sig": { color: "#e8a33d", marginBottom: "2px" },
          ".cm-lintRange-error": {
            backgroundImage: "none",
            textDecoration: "underline wavy #e05555 1px",
            textUnderlineOffset: "3px",
          },
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            dispatch("change", u.state.doc.toString());
          }
          if (
            u.docChanged ||
            u.transactions.some((tr) =>
              tr.effects.some((e) => e.is(toggleBp) || e.is(clearBps)),
            )
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
      // a wholesale external swap (example load) also drops breakpoints —
      // they belong to the old program
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
        effects: clearBps.of(null),
      });
    }
  }

  export function jumpTo(line: number, col: number): void {
    if (!view) return;
    const l = view.state.doc.line(Math.min(Math.max(line, 1), view.state.doc.lines));
    const pos = Math.min(l.from + Math.max(col - 1, 0), l.to);
    view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
    view.focus();
  }

  /** Show a compile-error squiggle over [from, to) (char offsets); null clears. */
  export function setErrorRange(range: { from: number; to: number; message: string } | null): void {
    if (!view) return;
    const len = view.state.doc.length;
    const diags =
      range === null
        ? []
        : [
            {
              from: Math.min(range.from, len),
              to: Math.min(Math.max(range.to, range.from + 1), len),
              severity: "error" as const,
              message: range.message,
            },
          ];
    view.dispatch(setDiagnostics(view.state, diags));
  }

  /** Highlight (and reveal) the paused line; null clears. */
  /** Move the gutter dots to the VM-resolved breakpoint lines (a click on a
   *  comment/blank line resolves to the next executable one). Does not fire
   *  the breakpoints event. */
  export function setBreakpointLines(lines: number[]): void {
    // microtask: this is reached from inside the updateListener that fired
    // the breakpoints event, and CM6 forbids dispatch-during-update
    queueMicrotask(() => {
      if (!view) return;
      const doc = view.state.doc;
      const positions = [...new Set(lines)]
        .filter((l) => l >= 1 && l <= doc.lines)
        .map((l) => doc.line(l).from)
        .sort((a, b) => a - b);
      view.dispatch({ effects: replaceBps.of(positions) });
    });
  }

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
