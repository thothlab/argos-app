/**
 * Thin Solid wrapper around CodeMirror 6.
 *
 * The `value` prop is one-way: external changes overwrite the editor's
 * contents. Edits inside the editor flow back via `onChange`. Round-trip
 * loops are guarded by tracking the last value we pushed in.
 *
 * Theme / font size / tab size / line wrapping are driven by the settings
 * store ([[settings.ts]]) — kept reactive via CodeMirror compartments so
 * a settings change reconfigures live editors without remounting.
 */

import { javascript } from '@codemirror/lang-javascript';
import { Compartment, EditorState } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView, keymap, lineNumbers } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import {
  bracketMatching,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
  defaultHighlightStyle,
} from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { createEffect, onCleanup, onMount } from 'solid-js';

import { settings } from '../stores/settings';
import { effectiveTheme } from '../stores/theme';

export type CodeEditorRef = {
  /** Insert `text` at the current selection, replacing it. Returns the new value. */
  insertAtCursor: (text: string) => string;
  /** Focus the editor. */
  focus: () => void;
};

function fontSizeExt(px: number) {
  return EditorView.theme({
    '&': { fontSize: `${px}px` },
    '.cm-content': { fontSize: `${px}px` },
    '.cm-gutters': { fontSize: `${px}px` },
  });
}

function tabSizeExt(n: number) {
  return [EditorState.tabSize.of(n), indentUnit.of(' '.repeat(n))];
}

function wrapExt(on: boolean) {
  return on ? EditorView.lineWrapping : [];
}

function themeExt(mode: 'follow-app' | 'one-dark') {
  if (mode === 'one-dark') return oneDark;
  // follow-app — light/dark mirrors the application theme.
  return effectiveTheme() === 'dark' ? oneDark : [];
}

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

  // Compartments let us swap a single facet (font / tabs / wrap / theme)
  // without rebuilding the entire EditorState.
  const fontC = new Compartment();
  const tabC = new Compartment();
  const wrapC = new Compartment();
  const themeC = new Compartment();

  onMount(() => {
    const s = settings();
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
        themeC.of(themeExt(s.editor.theme)),
        fontC.of(fontSizeExt(s.editor.fontSize)),
        tabC.of(tabSizeExt(s.editor.tabSize)),
        wrapC.of(wrapExt(s.editor.lineWrapping)),
        keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
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

  // Reactive editor preferences. Each effect tracks one slice so unrelated
  // settings don't trigger a re-dispatch.
  createEffect(() => {
    const size = settings().editor.fontSize;
    if (!view) return;
    view.dispatch({ effects: fontC.reconfigure(fontSizeExt(size)) });
  });
  createEffect(() => {
    const n = settings().editor.tabSize;
    if (!view) return;
    view.dispatch({ effects: tabC.reconfigure(tabSizeExt(n)) });
  });
  createEffect(() => {
    const on = settings().editor.lineWrapping;
    if (!view) return;
    view.dispatch({ effects: wrapC.reconfigure(wrapExt(on)) });
  });
  createEffect(() => {
    const mode = settings().editor.theme;
    // Also track the resolved app theme so `follow-app` reacts to ⌘⇧T.
    void effectiveTheme();
    if (!view) return;
    view.dispatch({ effects: themeC.reconfigure(themeExt(mode)) });
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
