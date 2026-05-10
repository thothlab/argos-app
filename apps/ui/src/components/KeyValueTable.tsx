/**
 * Reusable key-value table for headers, query params, env vars.
 *
 * **Why `<Index>` not `<For>`** — `<For>` keys by referential identity, so
 * any update that produces a new row object (which is every immutable patch
 * in our store) causes Solid to recreate the row's DOM and the input loses
 * focus mid-typing. `<Index>` keys by position so the DOM is preserved while
 * the data inside the row changes.
 *
 * Behaviour:
 * - One always-empty trailing row at the bottom so adding entries feels
 *   "endless". When users start typing in it, the parent's `onChange` fires
 *   and the row becomes a real entry; a fresh trailer takes its place on the
 *   next render.
 * - Each real row has an `enabled` checkbox + trash icon on hover.
 */

import { Index, Show, type JSX } from 'solid-js';

import { Trash2 } from 'lucide-solid';

export type RowEntry = {
  name: string;
  value: string;
  enabled: boolean;
};

export type KeyValueTableProps = {
  rows: RowEntry[];
  onChange: (rows: RowEntry[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  /** Optional custom rendering for the value column (e.g. for autocomplete). */
  valueColumn?: (row: RowEntry, idx: number, set: (v: string) => void) => JSX.Element;
};

export default function KeyValueTable(props: KeyValueTableProps) {
  // Always render exactly one trailing empty row.
  const rowsWithTrailer = (): RowEntry[] => {
    const list = props.rows.slice();
    const last = list.at(-1);
    if (!last || last.name !== '' || last.value !== '') {
      list.push({ name: '', value: '', enabled: true });
    }
    return list;
  };

  function commit(list: RowEntry[]): void {
    // Drop trailing empty rows beyond the first; the parent stores the
    // "real" entries and the trailer is re-derived on the next render.
    while (list.length > 0 && list.at(-1)!.name === '' && list.at(-1)!.value === '') {
      list.pop();
    }
    props.onChange(list);
  }

  function update(idx: number, patch: Partial<RowEntry>): void {
    const list = rowsWithTrailer();
    list[idx] = { ...list[idx]!, ...patch };
    commit(list);
  }

  function remove(idx: number): void {
    const list = rowsWithTrailer();
    list.splice(idx, 1);
    commit(list);
  }

  return (
    <table class="w-full font-mono text-[13px]">
      <thead>
        <tr class="text-left text-[10px] uppercase tracking-widest text-fg-secondary">
          <th class="w-8" />
          <th class="px-3 py-2 font-medium">{props.keyPlaceholder ?? 'Key'}</th>
          <th class="px-3 py-2 font-medium">{props.valuePlaceholder ?? 'Value'}</th>
          <th class="w-8" />
        </tr>
      </thead>
      <tbody>
        <Index each={rowsWithTrailer()}>
          {(row, i) => {
            const isPlaceholder = () => row().name === '' && row().value === '';
            return (
              <tr
                class="group border-t border-border"
                classList={{
                  'opacity-50': !row().enabled && !isPlaceholder(),
                }}
              >
                <td class="px-3 align-middle">
                  <input
                    type="checkbox"
                    class="h-3.5 w-3.5 cursor-pointer accent-primary disabled:opacity-30"
                    checked={row().enabled}
                    disabled={isPlaceholder()}
                    onChange={(e) => update(i, { enabled: e.currentTarget.checked })}
                  />
                </td>
                <td>
                  <input
                    type="text"
                    spellcheck={false}
                    autocomplete="off"
                    class="w-full bg-transparent px-3 py-2 outline-none placeholder:text-fg-secondary"
                    placeholder={props.keyPlaceholder ?? 'Key'}
                    value={row().name}
                    onInput={(e) => update(i, { name: e.currentTarget.value })}
                  />
                </td>
                <td>
                  {props.valueColumn ? (
                    props.valueColumn(row(), i, (v) => update(i, { value: v }))
                  ) : (
                    <input
                      type="text"
                      spellcheck={false}
                      autocomplete="off"
                      class="w-full bg-transparent px-3 py-2 outline-none placeholder:text-fg-secondary"
                      placeholder={props.valuePlaceholder ?? 'Value'}
                      value={row().value}
                      onInput={(e) => update(i, { value: e.currentTarget.value })}
                    />
                  )}
                </td>
                <td class="px-2">
                  <Show when={!isPlaceholder()}>
                    <button
                      type="button"
                      class="rounded p-1 text-fg-secondary opacity-0 hover:bg-bg-secondary hover:text-[var(--color-error-foreground)] group-hover:opacity-100"
                      title="Remove"
                      onClick={() => remove(i)}
                    >
                      <Trash2 size={13} />
                    </button>
                  </Show>
                </td>
              </tr>
            );
          }}
        </Index>
      </tbody>
    </table>
  );
}
