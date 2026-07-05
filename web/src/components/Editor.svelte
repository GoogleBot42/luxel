<script lang="ts">
  import { javascript } from "@codemirror/lang-javascript";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { EditorView, basicSetup } from "codemirror";
  import { createEventDispatcher, onDestroy, onMount } from "svelte";

  export let value: string;

  const dispatch = createEventDispatcher<{ change: string }>();
  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let internal = false;

  onMount(() => {
    view = new EditorView({
      doc: value,
      parent: host,
      extensions: [
        basicSetup,
        javascript(),
        oneDark,
        EditorView.theme({
          "&": { height: "100%", fontSize: "13px" },
          ".cm-scroller": { fontFamily: "ui-monospace, Menlo, Consolas, monospace" },
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            internal = true;
            dispatch("change", u.state.doc.toString());
            internal = false;
          }
        }),
      ],
    });
  });

  onDestroy(() => view?.destroy());

  // external value swaps (example selection) replace the document
  $: if (view && !internal && value !== view.state.doc.toString()) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
  }

  export function jumpTo(line: number, col: number): void {
    if (!view) return;
    const l = view.state.doc.line(Math.min(Math.max(line, 1), view.state.doc.lines));
    const pos = Math.min(l.from + Math.max(col - 1, 0), l.to);
    view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
    view.focus();
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
