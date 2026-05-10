/**
 * Thin Solid wrapper around CodeMirror 6.
 *
 * The `value` prop is one-way: external changes overwrite the editor's
 * contents. Edits inside the editor flow back via `onChange`. Round-trip
 * loops are guarded by tracking the last value we pushed in.
 */

import { javascript } from '@codemirror/lang-javascript';
import { EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView, keymap, lineNumbers } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, indentOnInput, syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { createEffect, onCleanup, onMount } from 'solid-js';

export type CodeEditorRef = {
  /** Insert `text` at the current selection, replacing it. Returns the new value. */
  insertAtCursor: (text: string) => string;
  /** Focus the editor. */
  focus: () => void;
};

export default function CodeEditor(props: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  minHeight?: string;
  ref?: (api: CodeEditorRef) => void;
}) {
  let host!: HTMLDivElement;
  let view: EditorView | null = null;
  // Latest value we pushed into the editor — used to skip the
  // upstream-echo when our own onChange triggers a re-render that
  // re-passes the same `value` back in.
  let lastEmittedValue = props.value;

  onMount(() => {
    const state = EditorState.create({
      doc: props.value,
      extensions: [
        lineNumbers(),
        history(),
        indentOnInput(),
        bracketMatching(),
        closeBrackets(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        javascript(),
        oneDark,
        keymap.of([
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...historyKeymap,
          indentWithTab,
        ]),
        EditorView.lineWrapping,
        EditorView.updateListener.of((u) => {
          if (!u.docChanged) return;
          const next = u.state.doc.toString();
          if (next === lastEmittedValue) return;
          lastEmittedValue = next;
          props.onChange(next);
        }),
      ],
    });
    view = new EditorView({ state, parent: host });

    if (props.ref) {
      props.ref({
        insertAtCursor: (text: string) => {
          if (!view) return lastEmittedValue;
          const sel = view.state.selection.main;
          view.dispatch({
            changes: { from: sel.from, to: sel.to, insert: text },
            selection: { anchor: sel.from + text.length },
          });
          view.focus();
          return view.state.doc.toString();
        },
        focus: () => view?.focus(),
      });
    }
  });

  // External changes (e.g. tab switch reloading a different draft).
  createEffect(() => {
    const v = props.value;
    if (!view) return;
    if (v === lastEmittedValue) return;
    lastEmittedValue = v;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: v },
    });
  });

  onCleanup(() => {
    view?.destroy();
    view = null;
  });

  return (
    <div
      ref={(el) => (host = el)}
      class="code-editor flex-1 overflow-auto rounded border border-border scrollbar-thin"
      style={{ 'min-height': props.minHeight ?? '160px' }}
      data-placeholder={props.placeholder ?? ''}
    />
  );
}
