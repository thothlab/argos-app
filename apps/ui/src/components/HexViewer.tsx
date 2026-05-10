/**
 * Hex viewer for binary response bodies.
 *
 * Renders rows of 16 bytes each with three columns: offset (hex), bytes
 * (space-separated hex), printable ASCII (with `.` for non-printable).
 *
 * For T1.4 v0.1 we cap at 8 KB to keep DOM small. Larger bodies show a
 * truncation hint — virtualisation ships with the streaming-response work.
 */

import { For, Show } from 'solid-js';

const ROW_BYTES = 16;
const MAX_BYTES = 8 * 1024;

export type HexViewerProps = {
  bytes: number[];
};

export default function HexViewer(props: HexViewerProps) {
  const visible = (): number[] =>
    props.bytes.length > MAX_BYTES ? props.bytes.slice(0, MAX_BYTES) : props.bytes;

  const rows = (): Array<{ offset: number; chunk: number[] }> => {
    const out: Array<{ offset: number; chunk: number[] }> = [];
    const data = visible();
    for (let i = 0; i < data.length; i += ROW_BYTES) {
      out.push({ offset: i, chunk: data.slice(i, i + ROW_BYTES) });
    }
    return out;
  };

  const truncated = () => props.bytes.length > MAX_BYTES;

  return (
    <div class="font-mono text-[12px] leading-relaxed">
      <table class="w-full">
        <tbody>
          <For each={rows()}>
            {(row) => (
              <tr class="border-b border-border/40">
                <td class="select-none whitespace-nowrap px-3 py-1 align-top text-fg-secondary">
                  {row.offset.toString(16).padStart(8, '0')}
                </td>
                <td class="whitespace-nowrap px-3 py-1 align-top text-fg-primary">
                  {row.chunk.map(toHex).join(' ').padEnd(ROW_BYTES * 3 - 1, ' ')}
                </td>
                <td class="whitespace-pre px-3 py-1 align-top text-fg-secondary">
                  {row.chunk.map(toAscii).join('')}
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={truncated()}>
        <p class="border-t border-border px-3 py-2 text-[11px] text-fg-secondary">
          Truncated at {MAX_BYTES} bytes — full body is {props.bytes.length} bytes.
          Streaming hex viewer arrives in a T1.4 follow-up.
        </p>
      </Show>
    </div>
  );
}

function toHex(b: number): string {
  return b.toString(16).padStart(2, '0');
}

function toAscii(b: number): string {
  return b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : '.';
}
