/**
 * Method picker — coloured dropdown for `GET / POST / PUT / PATCH / DELETE / ...`.
 *
 * Uses Kobalte's Select primitive for keyboard-accessible interactions
 * (arrow-key navigation, Esc to close, focus trap on open).
 */

import { Select } from '@kobalte/core/select';
import { ChevronDown } from 'lucide-solid';

import type { HttpMethod } from '../types/http';
import { HTTP_METHODS } from '../types/http';

const METHOD_VAR: Record<HttpMethod, string> = {
  GET: 'var(--method-get)',
  POST: 'var(--method-post)',
  PUT: 'var(--method-put)',
  PATCH: 'var(--method-patch)',
  DELETE: 'var(--method-delete)',
  HEAD: 'var(--fg-secondary)',
  OPTIONS: 'var(--fg-secondary)',
};

export type MethodPickerProps = {
  value: HttpMethod;
  onChange: (m: HttpMethod) => void;
};

export default function MethodPicker(props: MethodPickerProps) {
  return (
    <Select<HttpMethod>
      value={props.value}
      onChange={(v) => v && props.onChange(v)}
      options={HTTP_METHODS as unknown as HttpMethod[]}
      itemComponent={(itemProps) => (
        <Select.Item
          item={itemProps.item}
          class="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-[13px] font-mono font-bold hover:bg-bg-secondary data-[selected]:bg-bg-secondary"
        >
          <span style={{ color: METHOD_VAR[itemProps.item.rawValue] }}>
            {itemProps.item.rawValue}
          </span>
        </Select.Item>
      )}
    >
      <Select.Trigger
        aria-label="HTTP method"
        class="flex h-9 items-center gap-1 rounded-l-full border-y border-l border-border bg-bg-card pl-3 pr-2 font-mono text-[13px] font-bold hover:bg-bg-secondary"
        style={{ color: METHOD_VAR[props.value] }}
      >
        <Select.Value<HttpMethod>>{(s) => s.selectedOption()}</Select.Value>
        <Select.Icon>
          <ChevronDown size={12} class="text-fg-secondary" />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Content class="z-50 overflow-hidden rounded-md border border-border bg-bg-card shadow-lg">
          <Select.Listbox class="max-h-72 overflow-auto py-1" />
        </Select.Content>
      </Select.Portal>
    </Select>
  );
}
