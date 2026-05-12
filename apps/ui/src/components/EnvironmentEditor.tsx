/**
 * Environment editor — modal listing all envs in the workspace with an
 * editable variables / secrets table per env.
 *
 * Save is explicit (a Save button per env). Creating / deleting envs talks
 * to `environment_create` / `environment_delete`. Workspace state is
 * refreshed via the file watcher after each write — see
 * `crates/desktop/src-tauri/src/main.rs` for how that bubbles back as a
 * `workspace_changed` event.
 */

import { Dialog } from '@kobalte/core/dialog';
import { Plus, Trash2, X } from 'lucide-solid';
import { createEffect, createSignal, For, Show, untrack } from 'solid-js';
import { createStore, produce } from 'solid-js/store';

import {
  environmentCreate,
  environmentDelete,
  environmentSave,
  workspaceReload,
} from '../lib/api';
import { promptText } from '../lib/prompt';
import { setActiveEnvName, activeEnvName } from '../stores/active-env';
import { setWorkspace, workspace } from '../stores/workspace';
import type { EnvVar, Environment, EnvironmentEntry } from '../types/workspace';

type Draft = {
  path: string;
  name: string;
  variables: EnvVar[];
  secrets: EnvVar[];
  dirty: boolean;
};

function entryToDraft(e: EnvironmentEntry): Draft {
  return {
    path: e.path,
    name: e.name,
    variables: e.variables.map((v) => ({ ...v })),
    secrets: e.secrets.map((v) => ({ ...v })),
    dirty: false,
  };
}

export default function EnvironmentEditor(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [drafts, setDrafts] = createStore<Draft[]>([]);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  // Re-sync drafts every time the modal opens (or the workspace reloads
  // while it is open) — but never overwrite a dirty draft, otherwise the
  // file watcher would clobber the user's in-progress edits.
  createEffect(() => {
    const ws = workspace();
    if (!props.open || !ws) return;

    untrack(() => {
      setDrafts(
        produce((arr) => {
          const seen = new Set<string>();
          for (const e of ws.environments) {
            seen.add(e.path);
            const idx = arr.findIndex((d) => d.path === e.path);
            if (idx < 0) {
              arr.push(entryToDraft(e));
            } else if (!arr[idx]!.dirty) {
              arr[idx] = entryToDraft(e);
            }
          }
          for (let i = arr.length - 1; i >= 0; i -= 1) {
            if (!seen.has(arr[i]!.path)) arr.splice(i, 1);
          }
        }),
      );
      if (selected() === null && drafts.length > 0) {
        setSelected(drafts[0]!.path);
      }
    });
  });

  function patchDraft(path: string, fn: (d: Draft) => void) {
    setDrafts(
      (d) => d.path === path,
      produce((d) => {
        fn(d);
        d.dirty = true;
      }),
    );
  }

  async function createEnv() {
    const ws = workspace();
    if (!ws) return;
    const name = await promptText({
      title: 'New environment',
      placeholder: 'staging',
      defaultValue: 'staging',
      submitLabel: 'Create',
    });
    if (!name) return;
    setBusy(true);
    setErr(null);
    try {
      const envDir = `${ws.root}/${ws.manifest.config.environments_dir}`;
      const newPath = await environmentCreate(envDir, name);
      const next = await workspaceReload(ws.root);
      setWorkspace(next);
      setSelected(newPath);
    } catch (e) {
      setErr(String((e as Error).message ?? e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteEnv(path: string, name: string) {
    if (!confirm(`Delete environment "${name}"?`)) return;
    setBusy(true);
    setErr(null);
    try {
      await environmentDelete(path);
      const ws = workspace();
      if (ws) {
        const next = await workspaceReload(ws.root);
        setWorkspace(next);
      }
      if (activeEnvName() === name) setActiveEnvName(null);
      if (selected() === path) setSelected(drafts[0]?.path ?? null);
    } catch (e) {
      setErr(String((e as Error).message ?? e));
    } finally {
      setBusy(false);
    }
  }

  async function saveEnv(path: string) {
    const draft = drafts.find((d) => d.path === path);
    if (!draft) return;
    setBusy(true);
    setErr(null);
    try {
      const env: Environment = {
        kind: 'environment',
        name: draft.name,
        variables: draft.variables.map((v) => ({ ...v })),
        secrets: draft.secrets.map((v) => ({ ...v })),
      };
      await environmentSave(path, env);
      setDrafts((d) => d.path === path, 'dirty', false);
      const ws = workspace();
      if (ws) {
        const next = await workspaceReload(ws.root);
        setWorkspace(next);
      }
    } catch (e) {
      setErr(String((e as Error).message ?? e));
    } finally {
      setBusy(false);
    }
  }

  const current = () => drafts.find((d) => d.path === selected()) ?? null;

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay class="fixed inset-0 z-40 bg-black/50" />
        <div class="fixed inset-0 z-50 flex items-center justify-center p-6">
          <Dialog.Content class="flex h-[640px] max-h-[85vh] w-[920px] max-w-[95vw] flex-col overflow-hidden rounded-lg border border-border bg-bg-card shadow-xl">
            <header class="flex h-11 shrink-0 items-center gap-3 border-b border-border px-4">
              <Dialog.Title class="text-[14px] font-medium text-fg-primary">
                Environments
              </Dialog.Title>
              <span class="text-[11px] text-fg-secondary">
                {workspace()?.manifest.name}
              </span>
              <div class="flex-1" />
              <Dialog.CloseButton class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary">
                <X size={14} />
              </Dialog.CloseButton>
            </header>

            <div class="flex flex-1 overflow-hidden">
              <aside class="flex w-56 shrink-0 flex-col border-r border-border">
                <button
                  type="button"
                  class="flex h-9 items-center gap-2 border-b border-border px-3 text-[13px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary disabled:opacity-50"
                  disabled={busy()}
                  onClick={createEnv}
                >
                  <Plus size={14} />
                  <span>New environment</span>
                </button>
                <div class="flex-1 overflow-auto scrollbar-thin">
                  <For each={drafts} fallback={<EmptyHint />}>
                    {(d) => (
                      <button
                        type="button"
                        class="group flex w-full items-center gap-2 border-l-2 px-3 py-2 text-left text-[13px]"
                        classList={{
                          'border-primary bg-bg-secondary text-fg-primary':
                            selected() === d.path,
                          'border-transparent text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary':
                            selected() !== d.path,
                        }}
                        onClick={() => setSelected(d.path)}
                      >
                        <span class="flex-1 truncate font-mono">{d.name}</span>
                        <Show when={d.dirty}>
                          <span
                            class="h-1.5 w-1.5 rounded-full bg-primary"
                            aria-label="unsaved changes"
                          />
                        </Show>
                        <span
                          role="button"
                          tabindex={0}
                          class="invisible rounded p-0.5 text-fg-secondary hover:text-danger group-hover:visible"
                          onClick={(e) => {
                            e.stopPropagation();
                            void deleteEnv(d.path, d.name);
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter' || e.key === ' ') {
                              e.stopPropagation();
                              void deleteEnv(d.path, d.name);
                            }
                          }}
                        >
                          <Trash2 size={12} />
                        </span>
                      </button>
                    )}
                  </For>
                </div>
              </aside>

              <section class="flex flex-1 flex-col overflow-hidden">
                <Show when={current()} fallback={<NoSelection />}>
                  {(d) => (
                    <>
                      <div class="flex h-10 shrink-0 items-center gap-3 border-b border-border px-4">
                        <span class="text-[12px] text-fg-secondary">Name:</span>
                        <span class="font-mono text-[13px] text-fg-primary">
                          {d().name}
                        </span>
                        <span class="font-mono text-[10px] text-fg-secondary">
                          {d().path}
                        </span>
                        <div class="flex-1" />
                        <button
                          type="button"
                          class="rounded bg-primary px-3 py-1 text-[12px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
                          disabled={!d().dirty || busy()}
                          onClick={() => void saveEnv(d().path)}
                        >
                          {d().dirty ? 'Save' : 'Saved'}
                        </button>
                      </div>

                      <div class="flex-1 overflow-auto scrollbar-thin px-4 py-3">
                        <VarsTable
                          title="Variables"
                          rows={d().variables}
                          onChange={(rows) =>
                            patchDraft(d().path, (dd) => {
                              dd.variables = rows;
                            })
                          }
                          masked={false}
                        />
                        <div class="h-4" />
                        <VarsTable
                          title="Secrets"
                          hint="Stored alongside the env file. Secret values win on key collision when resolving {{vars}}."
                          rows={d().secrets}
                          onChange={(rows) =>
                            patchDraft(d().path, (dd) => {
                              dd.secrets = rows;
                            })
                          }
                          masked
                        />
                      </div>
                    </>
                  )}
                </Show>
              </section>
            </div>

            <Show when={err()}>
              <footer class="shrink-0 border-t border-border bg-bg-secondary px-4 py-2 text-[12px] text-danger">
                {err()}
              </footer>
            </Show>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog>
  );
}

function EmptyHint() {
  return (
    <div class="px-3 py-4 text-[12px] text-fg-secondary">
      No environments yet. Use “New environment” above to create one.
    </div>
  );
}

function NoSelection() {
  return (
    <div class="flex flex-1 items-center justify-center text-[13px] text-fg-secondary">
      Select an environment on the left.
    </div>
  );
}

function VarsTable(props: {
  title: string;
  hint?: string;
  rows: EnvVar[];
  masked: boolean;
  onChange: (rows: EnvVar[]) => void;
}) {
  function update(idx: number, patch: Partial<EnvVar>) {
    const next = props.rows.map((r, i) => (i === idx ? { ...r, ...patch } : r));
    props.onChange(next);
  }
  function remove(idx: number) {
    props.onChange(props.rows.filter((_, i) => i !== idx));
  }
  function add() {
    props.onChange([...props.rows, { name: '', value: '', enabled: true }]);
  }

  return (
    <div>
      <div class="mb-1.5 flex items-baseline gap-2">
        <h3 class="text-[13px] font-medium text-fg-primary">{props.title}</h3>
        <Show when={props.hint}>
          <span class="text-[11px] text-fg-secondary">{props.hint}</span>
        </Show>
      </div>
      <div class="rounded border border-border">
        <div class="grid grid-cols-[28px_minmax(160px,1fr)_minmax(160px,2fr)_28px] border-b border-border bg-bg-secondary text-[11px] text-fg-secondary">
          <div class="px-2 py-1.5" />
          <div class="px-2 py-1.5">Name</div>
          <div class="px-2 py-1.5">Value</div>
          <div class="px-2 py-1.5" />
        </div>
        <For
          each={props.rows}
          fallback={
            <div class="px-3 py-3 text-[12px] text-fg-secondary">
              No entries.
            </div>
          }
        >
          {(row, idx) => (
            <div class="grid grid-cols-[28px_minmax(160px,1fr)_minmax(160px,2fr)_28px] items-center border-b border-border last:border-b-0">
              <div class="flex justify-center">
                <input
                  type="checkbox"
                  class="accent-primary"
                  checked={row.enabled}
                  onChange={(e) => update(idx(), { enabled: e.currentTarget.checked })}
                />
              </div>
              <input
                type="text"
                spellcheck={false}
                autocomplete="off"
                class="h-8 bg-transparent px-2 font-mono text-[12px] outline-none focus:bg-bg-secondary"
                value={row.name}
                onInput={(e) => update(idx(), { name: e.currentTarget.value })}
              />
              <input
                type={props.masked ? 'password' : 'text'}
                spellcheck={false}
                autocomplete="off"
                class="h-8 bg-transparent px-2 font-mono text-[12px] outline-none focus:bg-bg-secondary"
                value={row.value}
                onInput={(e) => update(idx(), { value: e.currentTarget.value })}
              />
              <button
                type="button"
                class="rounded p-1 text-fg-secondary hover:text-danger"
                onClick={() => remove(idx())}
                title="Remove row"
              >
                <Trash2 size={12} />
              </button>
            </div>
          )}
        </For>
        <button
          type="button"
          class="flex w-full items-center gap-1.5 px-3 py-1.5 text-[12px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
          onClick={add}
        >
          <Plus size={12} />
          <span>Add row</span>
        </button>
      </div>
    </div>
  );
}
