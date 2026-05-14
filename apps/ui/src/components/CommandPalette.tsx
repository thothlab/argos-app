/**
 * Command palette — fuzzy jump-to-request across the workspace tree.
 *
 * Opens via ⌘K or the top-bar search button. Filters by a name+path token
 * match; arrow keys navigate, Enter opens or focuses a tab, Escape closes.
 */

import { createEffect, createMemo, createSignal, For, Show } from 'solid-js';
import { Search } from 'lucide-solid';

import { closePalette, paletteOpen } from '../stores/command-palette';
import { openOrFocusTabForRequest } from '../stores/tabs';
import { workspace } from '../stores/workspace';
import type { HttpMethod } from '../types/http';
import type { ProtocolTag, RequestDraft, TreeNode } from '../types/workspace';

type Item = {
  path: string;
  name: string;
  folderTrail: string[];
  protocol: ProtocolTag;
  method: HttpMethod | null;
  draft: RequestDraft;
};

const METHOD_VAR: Record<HttpMethod, string> = {
  GET: 'var(--method-get)',
  POST: 'var(--method-post)',
  PUT: 'var(--method-put)',
  PATCH: 'var(--method-patch)',
  DELETE: 'var(--method-delete)',
  HEAD: 'var(--fg-secondary)',
  OPTIONS: 'var(--fg-secondary)',
};

const PROTO_LABEL: Record<ProtocolTag, string> = {
  rest: 'REST',
  graphql: 'GQL',
  websocket: 'WS',
};

function collect(node: TreeNode, trail: string[], out: Item[]): void {
  if (node.kind === 'request') {
    out.push({
      path: node.path,
      name: node.draft.name,
      folderTrail: trail,
      protocol: node.draft.type,
      method: node.draft.type === 'rest' ? node.draft.method : null,
      draft: node.draft,
    });
    return;
  }
  const nextTrail = node.name ? [...trail, node.name] : trail;
  for (const child of node.children) collect(child, nextTrail, out);
}

function allItems(): Item[] {
  const ws = workspace();
  if (!ws) return [];
  const out: Item[] = [];
  // Skip the root folder name — it's the workspace itself.
  if (ws.tree.kind === 'folder') {
    for (const child of ws.tree.children) collect(child, [], out);
  } else {
    collect(ws.tree, [], out);
  }
  return out;
}

function score(item: Item, tokens: string[]): number {
  if (tokens.length === 0) return 1;
  const name = item.name.toLowerCase();
  const trail = item.folderTrail.join('/').toLowerCase();
  const hay = `${name} ${trail}`;
  let s = 0;
  for (const t of tokens) {
    if (!hay.includes(t)) return 0;
    if (name.startsWith(t)) s += 100;
    else if (name.includes(t)) s += 50;
    else s += 10;
  }
  return s;
}

export default function CommandPalette() {
  const [query, setQuery] = createSignal('');
  const [cursor, setCursor] = createSignal(0);
  let listEl: HTMLUListElement | undefined;

  // Reset state each time the palette opens.
  createEffect(() => {
    if (paletteOpen()) {
      setQuery('');
      setCursor(0);
    }
  });

  const results = createMemo<Item[]>(() => {
    if (!paletteOpen()) return [];
    const q = query().trim().toLowerCase();
    const tokens = q.length === 0 ? [] : q.split(/\s+/);
    const items = allItems();
    if (tokens.length === 0) {
      // No query → show everything in document order, capped.
      return items.slice(0, 200);
    }
    const scored = items
      .map((it) => ({ it, s: score(it, tokens) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 200)
      .map((x) => x.it);
    return scored;
  });

  // Keep the cursor inside the result set on every change.
  createEffect(() => {
    const max = Math.max(0, results().length - 1);
    if (cursor() > max) setCursor(max);
  });

  // Scroll the highlighted row into view when the cursor moves.
  createEffect(() => {
    const i = cursor();
    if (!paletteOpen() || !listEl) return;
    const row = listEl.children.item(i) as HTMLElement | null;
    row?.scrollIntoView({ block: 'nearest' });
  });

  function activate(item: Item) {
    openOrFocusTabForRequest(item.path, item.draft);
    closePalette();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closePalette();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const max = Math.max(0, results().length - 1);
      setCursor((i) => (i >= max ? 0 : i + 1));
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      const max = Math.max(0, results().length - 1);
      setCursor((i) => (i <= 0 ? max : i - 1));
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const sel = results()[cursor()];
      if (sel) activate(sel);
      return;
    }
  }

  return (
    <Show when={paletteOpen()}>
      <div
        class="pointer-events-auto fixed inset-0 z-[100] flex items-start justify-center bg-bg-primary/70 pt-[12vh]"
        role="dialog"
        aria-modal="true"
        data-kb-top-layer
        onClick={(e) => {
          if (e.target === e.currentTarget) closePalette();
        }}
      >
        <div class="flex w-[560px] max-w-[90vw] flex-col overflow-hidden rounded-xl border border-border bg-bg-card shadow-xl">
          <div class="flex items-center gap-2 border-b border-border px-3">
            <Search size={14} class="text-fg-secondary" />
            <input
              ref={(el) => {
                window.requestAnimationFrame(() => el?.focus());
              }}
              type="text"
              spellcheck={false}
              autocomplete="off"
              class="h-11 flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-secondary"
              placeholder="Search or jump to…"
              value={query()}
              onInput={(e) => {
                setQuery(e.currentTarget.value);
                setCursor(0);
              }}
              onKeyDown={onKeyDown}
            />
          </div>

          <Show
            when={results().length > 0}
            fallback={
              <div class="px-4 py-6 text-center font-mono text-[12px] text-fg-secondary">
                <Show when={!workspace()} fallback={<>No matches.</>}>
                  Open a workspace to search requests.
                </Show>
              </div>
            }
          >
            <ul ref={listEl} class="max-h-[50vh] overflow-y-auto py-1">
              <For each={results()}>
                {(item, i) => {
                  const active = () => i() === cursor();
                  return (
                    <li>
                      <button
                        type="button"
                        class="flex w-full items-center gap-3 px-3 py-1.5 text-left text-[13px]"
                        classList={{
                          'bg-bg-secondary text-fg-primary': active(),
                          'text-fg-primary hover:bg-bg-secondary': !active(),
                        }}
                        onMouseEnter={() => setCursor(i())}
                        onClick={() => activate(item)}
                      >
                        <span
                          class="w-12 shrink-0 font-mono text-[10px] font-bold"
                          style={{
                            color: item.method
                              ? METHOD_VAR[item.method]
                              : 'var(--fg-secondary)',
                          }}
                        >
                          {item.method ?? PROTO_LABEL[item.protocol]}
                        </span>
                        <span class="truncate">{item.name}</span>
                        <Show when={item.folderTrail.length > 0}>
                          <span class="ml-auto truncate pl-3 font-mono text-[11px] text-fg-secondary">
                            {item.folderTrail.join(' / ')}
                          </span>
                        </Show>
                      </button>
                    </li>
                  );
                }}
              </For>
            </ul>
          </Show>

          <div class="flex items-center gap-3 border-t border-border px-3 py-1.5 font-mono text-[10px] text-fg-secondary">
            <span>
              <kbd class="rounded bg-bg-secondary px-1 py-0.5">↑↓</kbd> navigate
            </span>
            <span>
              <kbd class="rounded bg-bg-secondary px-1 py-0.5">↵</kbd> open
            </span>
            <span>
              <kbd class="rounded bg-bg-secondary px-1 py-0.5">esc</kbd> close
            </span>
          </div>
        </div>
      </div>
    </Show>
  );
}
