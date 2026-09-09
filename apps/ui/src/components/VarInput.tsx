/**
 * Single-line input with `{{variable}}` autocomplete.
 *
 * Typing `{{` opens a popup listing the active environment's variables
 * plus the dynamic built-ins (`$randomUuid`, `$timestamp`, …) that the
 * Rust resolver generates fresh on every send. Enter / Tab / click
 * accepts, Esc closes, ↑ ↓ move.
 *
 * **Why the popup lives next to the input and not inside it** — the
 * `<input>` element must never be torn down while the user types (that is
 * the focus-loss trap `KeyValueTable` documents at the top of its file).
 * So the input is rendered unconditionally and only the sibling `<ul>`
 * is wrapped in `<Show>`; opening or closing the popup never touches the
 * input's DOM node.
 *
 * Keys the popup does not consume are forwarded to `onKeyDown`, so the
 * URL bar keeps its "Enter sends the request" behaviour.
 */

import { createEffect, createMemo, createSignal, Index, Show } from 'solid-js';

import { activeEnvVars } from '../stores/active-env';
import {
  applySuggestion,
  DYNAMIC_VARS,
  envSuggestions,
  filterSuggestions,
  findOpenPlaceholder,
  type OpenPlaceholder,
  type VarSuggestion,
} from '../lib/dynamic-vars';

export type VarInputProps = {
  value: string;
  onInput: (value: string) => void;
  placeholder?: string;
  /** Classes for the `<input>` itself. */
  class?: string;
  /** Classes for the positioning wrapper — the element the parent lays out. */
  wrapperClass?: string;
  disabled?: boolean;
  onKeyDown?: (e: KeyboardEvent) => void;
};

/** Cap the popup so a large environment can't cover the whole editor. */
const MAX_ITEMS = 8;

export default function VarInput(props: VarInputProps) {
  let el!: HTMLInputElement;
  const [open, setOpen] = createSignal<OpenPlaceholder | null>(null);
  const [selected, setSelected] = createSignal(0);
  /**
   * Caret to restore after an accepted suggestion. The new text travels
   * out through `onInput` and comes back as `props.value`; when Solid
   * writes that back into the element it resets the caret to the end, so
   * the caret is placed *after* the round-trip, not before it.
   */
  let pendingCaret: { text: string; caret: number } | null = null;

  function flushCaret(): void {
    const p = pendingCaret;
    if (!p || el.value !== p.text) return;
    pendingCaret = null;
    el.setSelectionRange(p.caret, p.caret);
    el.focus();
  }

  createEffect(() => {
    // Track the prop so this runs on the store round-trip.
    void props.value;
    void Promise.resolve().then(flushCaret);
  });

  const items = createMemo<VarSuggestion[]>(() => {
    const o = open();
    if (!o) return [];
    const all = [...envSuggestions(activeEnvVars()), ...DYNAMIC_VARS];
    return filterSuggestions(all, o.query).slice(0, MAX_ITEMS);
  });

  /** Recompute the open placeholder from the live caret position. */
  function sync(): void {
    const caret = el.selectionStart ?? el.value.length;
    setOpen(findOpenPlaceholder(el.value, caret));
    setSelected(0);
  }

  function accept(name: string): void {
    const o = open();
    if (!o) return;
    const caret = el.selectionStart ?? el.value.length;
    const next = applySuggestion(el.value, caret, o, name);
    setOpen(null);
    el.value = next.text;
    pendingCaret = next;
    props.onInput(next.text);
    // If the value never round-trips (parent ignored it, or it was already
    // equal so no effect fires) this places the caret anyway; `flushCaret`
    // is idempotent.
    void Promise.resolve().then(flushCaret);
  }

  function onKeyDown(e: KeyboardEvent): void {
    const list = items();
    if (open() && list.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelected((i) => (i + 1) % list.length);
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelected((i) => (i - 1 + list.length) % list.length);
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        accept(list[selected()]!.name);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        setOpen(null);
        return;
      }
    }
    props.onKeyDown?.(e);
  }

  return (
    <div class={props.wrapperClass ?? 'relative'}>
      <input
        ref={el}
        type="text"
        spellcheck={false}
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        class={
          props.class ??
          'w-full bg-transparent px-3 py-2 outline-none placeholder:text-fg-secondary'
        }
        placeholder={props.placeholder}
        value={props.value}
        disabled={props.disabled}
        onInput={(e) => {
          props.onInput(e.currentTarget.value);
          sync();
        }}
        onClick={sync}
        onKeyDown={onKeyDown}
        onKeyUp={(e) => {
          // Arrow keys / Home / End move the caret out of (or into) a
          // placeholder without firing `input`.
          if (e.key.startsWith('Arrow') || e.key === 'Home' || e.key === 'End') sync();
        }}
        onBlur={() => setOpen(null)}
      />

      <Show when={open() && items().length > 0}>
        <ul
          // Keep a click on padding or the scrollbar from blurring the
          // input, which would close the popup mid-drag.
          onMouseDown={(e) => e.preventDefault()}
          class="absolute left-0 top-full z-50 mt-0.5 max-h-64 w-[22rem] max-w-[90vw] overflow-auto rounded border border-border bg-bg-card py-1 shadow-lg"
        >
          <Index each={items()}>
            {(item, i) => (
              <li>
                <button
                  type="button"
                  class="flex w-full items-baseline gap-2 px-2 py-1 text-left"
                  classList={{ 'bg-bg-secondary': selected() === i }}
                  // `mousedown` would blur the input and close the popup
                  // before `click` ever lands.
                  onMouseDown={(e) => {
                    e.preventDefault();
                    accept(item().name);
                  }}
                  onMouseEnter={() => setSelected(i)}
                >
                  <span
                    class="shrink-0 font-mono text-[12px]"
                    classList={{
                      'text-primary': item().kind === 'dynamic',
                      'text-fg-primary': item().kind === 'env',
                    }}
                  >
                    {`{{${item().name}}}`}
                  </span>
                  <span class="truncate text-[11px] text-fg-secondary">{item().description}</span>
                </button>
              </li>
            )}
          </Index>
        </ul>
      </Show>
    </div>
  );
}
