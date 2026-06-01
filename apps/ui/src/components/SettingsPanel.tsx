/**
 * Settings panel — modal with Appearance / Editor / Keybindings / Advanced tabs.
 * Opens via ⌘, or the Settings entry in the TopBar workspace menu.
 *
 * Storage lives in [[settings.ts]]. Keybinding edits flow through the
 * action registry in [[actions.ts]] — overrides are stored by `ActionId`,
 * defaults are derived from each `defineAction` call.
 */

import { createSignal, For, Show, createEffect, onCleanup, type JSX } from 'solid-js';
import { RotateCcw, X } from 'lucide-solid';

import { closeSettings, settingsActiveTab, settingsOpen, setSettingsTab, type SettingsTab } from '../stores/settings-panel';
import { openCrashLog } from '../stores/crash-log-panel';
import { checkForUpdatesNow, installPendingUpdate, pendingUpdate } from '../lib/updater';
import {
  AI_PROVIDERS,
  DEFAULT_SETTINGS,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  RELEASE_CHANNELS,
  TAB_SIZES,
  mergeWithDefaults,
  replaceAllSettings,
  resetAllKeybindings,
  setAiApiKey,
  setAiBaseUrl,
  setAiModel,
  setAiProvider,
  setEditorFontSize,
  setEditorLineWrapping,
  setEditorTabSize,
  setEditorTheme,
  setKeybinding,
  setReleaseChannel,
  setTheme,
  settings,
  type AiProvider,
  type EditorThemeMode,
  type ReleaseChannel,
  type Settings,
  type Theme,
} from '../stores/settings';
import { actionConflicting, effectiveCombo, listActions, type ActionDef, type ActionId } from '../lib/actions';
import { comboToString, eventToCombo, label } from '../lib/hotkeys';
import { settingsReset } from '../lib/api';
import { notify, notifyError } from '../lib/toast';

const TABS: Array<{ id: SettingsTab; label: string }> = [
  { id: 'appearance', label: 'Appearance' },
  { id: 'editor', label: 'Editor' },
  { id: 'keybindings', label: 'Keybindings' },
  { id: 'ai', label: 'AI' },
  { id: 'advanced', label: 'Advanced' },
];

export default function SettingsPanel() {
  // Local keydown to close on Escape (action router doesn't route Escape).
  function onKey(e: KeyboardEvent) {
    if (!settingsOpen()) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      closeSettings();
    }
  }
  createEffect(() => {
    if (settingsOpen()) {
      window.addEventListener('keydown', onKey);
    }
  });
  onCleanup(() => window.removeEventListener('keydown', onKey));

  return (
    <Show when={settingsOpen()}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-bg-primary/70"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onClick={(e) => {
          if (e.target === e.currentTarget) closeSettings();
        }}
      >
        <div class="flex h-[560px] w-[760px] flex-col overflow-hidden rounded-xl border border-border bg-bg-card shadow-xl">
          <header class="flex items-center justify-between border-b border-border px-4 py-3">
            <h2 id="settings-title" class="text-[14px] font-semibold">
              Settings
            </h2>
            <button
              type="button"
              class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
              onClick={() => closeSettings()}
              aria-label="Close settings"
            >
              <X size={14} />
            </button>
          </header>

          <div class="flex min-h-0 flex-1">
            <nav class="flex w-44 shrink-0 flex-col gap-0.5 border-r border-border bg-bg-secondary/40 p-2">
              <For each={TABS}>
                {(t) => (
                  <button
                    type="button"
                    class="rounded px-3 py-1.5 text-left text-[13px] hover:bg-bg-secondary"
                    classList={{
                      'bg-bg-secondary font-medium text-fg-primary':
                        settingsActiveTab() === t.id,
                      'text-fg-secondary': settingsActiveTab() !== t.id,
                    }}
                    onClick={() => setSettingsTab(t.id)}
                  >
                    {t.label}
                  </button>
                )}
              </For>
            </nav>

            <section class="flex-1 overflow-y-auto p-6 scrollbar-thin">
              <Show when={settingsActiveTab() === 'appearance'}>
                <AppearanceTab />
              </Show>
              <Show when={settingsActiveTab() === 'editor'}>
                <EditorTab />
              </Show>
              <Show when={settingsActiveTab() === 'keybindings'}>
                <KeybindingsTab />
              </Show>
              <Show when={settingsActiveTab() === 'ai'}>
                <AiTab />
              </Show>
              <Show when={settingsActiveTab() === 'advanced'}>
                <AdvancedTab />
              </Show>
            </section>
          </div>
        </div>
      </div>
    </Show>
  );
}

// ---- tabs -----------------------------------------------------------------

function SectionHeading(props: { children: string; hint?: string }) {
  return (
    <div class="mb-3">
      <h3 class="text-[13px] font-semibold text-fg-primary">{props.children}</h3>
      <Show when={props.hint}>
        <p class="mt-0.5 text-[11px] text-fg-secondary">{props.hint}</p>
      </Show>
    </div>
  );
}

function Row(props: { label: string; hint?: string; children: JSX.Element }) {
  return (
    <div class="flex items-start justify-between gap-6 py-2.5">
      <div class="min-w-0 flex-1">
        <div class="text-[12px] font-medium text-fg-primary">{props.label}</div>
        <Show when={props.hint}>
          <div class="mt-0.5 text-[11px] text-fg-secondary">{props.hint}</div>
        </Show>
      </div>
      <div class="shrink-0">{props.children}</div>
    </div>
  );
}

function AppearanceTab() {
  return (
    <>
      <SectionHeading>Theme</SectionHeading>
      <Row label="Application theme" hint="System follows your OS appearance setting.">
        <Segmented<Theme>
          value={settings().appearance.theme}
          options={[
            { value: 'light', label: 'Light' },
            { value: 'dark', label: 'Dark' },
            { value: 'system', label: 'System' },
          ]}
          onChange={setTheme}
        />
      </Row>
    </>
  );
}

function EditorTab() {
  return (
    <>
      <SectionHeading>Editor preferences</SectionHeading>
      <Row label="Font size">
        <div class="flex items-center gap-3">
          <input
            type="range"
            min={FONT_SIZE_MIN}
            max={FONT_SIZE_MAX}
            step="1"
            value={settings().editor.fontSize}
            onInput={(e) => setEditorFontSize(Number(e.currentTarget.value))}
            class="accent-[var(--color-primary)]"
          />
          <span class="w-10 text-right font-mono text-[12px] text-fg-primary">
            {settings().editor.fontSize}px
          </span>
        </div>
      </Row>
      <Row label="Tab size">
        <Segmented<number>
          value={settings().editor.tabSize}
          options={TAB_SIZES.map((n) => ({ value: n, label: String(n) }))}
          onChange={(n) => setEditorTabSize(n as 2 | 4 | 8)}
        />
      </Row>
      <Row label="Line wrapping" hint="Off shows horizontal scrollbars on long lines.">
        <Toggle
          checked={settings().editor.lineWrapping}
          onChange={setEditorLineWrapping}
        />
      </Row>
      <Row label="Editor theme" hint="“Follow app” keeps the editor in sync with the application theme.">
        <Segmented<EditorThemeMode>
          value={settings().editor.theme}
          options={[
            { value: 'follow-app', label: 'Follow app' },
            { value: 'one-dark', label: 'One Dark' },
          ]}
          onChange={setEditorTheme}
        />
      </Row>
    </>
  );
}

function KeybindingsTab() {
  const actions = () => listActions();
  return (
    <>
      <SectionHeading hint="Click a binding to record a new combo. Press Esc to cancel.">
        Global keyboard shortcuts
      </SectionHeading>
      <div class="flex flex-col">
        <For each={actions()}>{(a) => <KeybindingRow action={a} />}</For>
      </div>
      <div class="mt-4 border-t border-border pt-3">
        <button
          type="button"
          class="flex items-center gap-1.5 rounded border border-border px-2.5 py-1.5 text-[12px] hover:bg-bg-secondary"
          onClick={() => resetAllKeybindings()}
        >
          <RotateCcw size={12} />
          Reset all to defaults
        </button>
      </div>
    </>
  );
}

function KeybindingRow(props: { action: ActionDef }) {
  const [capturing, setCapturing] = createSignal(false);
  const [conflict, setConflict] = createSignal<ActionDef | null>(null);
  const [pendingCombo, setPendingCombo] = createSignal<ReturnType<typeof eventToCombo>>(null);

  const id = props.action.id as ActionId;

  function onKeyDown(e: KeyboardEvent) {
    if (!capturing()) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      cancel();
      return;
    }
    const combo = eventToCombo(e);
    if (!combo) return; // modifier-only — wait for the real key
    const c = actionConflicting(combo, id);
    if (c) {
      setPendingCombo(combo);
      setConflict(c);
      return;
    }
    setKeybinding(id, comboToString(combo));
    cancel();
  }

  function cancel() {
    setCapturing(false);
    setConflict(null);
    setPendingCombo(null);
  }

  function applyOverride() {
    const combo = pendingCombo();
    const c = conflict();
    if (!combo || !c) return;
    // Free the conflicting binding (explicitly disable) before assigning.
    setKeybinding(c.id as ActionId, null);
    setKeybinding(id, comboToString(combo));
    cancel();
  }

  createEffect(() => {
    if (capturing()) {
      window.addEventListener('keydown', onKeyDown, true);
    } else {
      window.removeEventListener('keydown', onKeyDown, true);
    }
  });
  onCleanup(() => window.removeEventListener('keydown', onKeyDown, true));

  const combo = () => effectiveCombo(id);
  const isOverridden = () => settings().keybindings[id] !== undefined;

  return (
    <div class="flex items-center justify-between gap-3 border-b border-border/60 py-2 last:border-b-0">
      <div class="min-w-0 flex-1 text-[12px] text-fg-primary">{props.action.label}</div>
      <div class="flex items-center gap-2">
        <Show when={isOverridden() && !capturing()}>
          <button
            type="button"
            class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
            title="Reset to default"
            onClick={() => setKeybinding(id, undefined)}
          >
            <RotateCcw size={12} />
          </button>
        </Show>
        <button
          type="button"
          class="rounded border px-2.5 py-1 font-mono text-[11px]"
          classList={{
            'border-primary bg-primary/10 text-fg-primary': capturing(),
            'border-border bg-bg-secondary hover:bg-border text-fg-primary':
              !capturing() && combo() !== null,
            'border-dashed border-border text-fg-secondary': combo() === null,
          }}
          onClick={() => setCapturing((v) => !v)}
        >
          <Show
            when={capturing()}
            fallback={combo() ? label(combo()!) : 'disabled'}
          >
            <span class="text-fg-secondary">press a key…</span>
          </Show>
        </button>
      </div>
      <Show when={conflict()}>
        <div class="basis-full pt-1 text-[11px] text-fg-error">
          <span>
            Conflicts with <strong>{conflict()!.label}</strong>.{' '}
          </span>
          <button
            type="button"
            class="underline hover:no-underline"
            onClick={applyOverride}
          >
            Override anyway
          </button>
          <span> · </span>
          <button type="button" class="underline hover:no-underline" onClick={cancel}>
            Cancel
          </button>
        </div>
      </Show>
    </div>
  );
}

function AiTab() {
  const provider = () => settings().ai.provider;
  const providerLabel = (p: AiProvider): string => {
    switch (p) {
      case 'none':
        return 'None';
      case 'anthropic':
        return 'Anthropic';
      case 'openai-compatible':
        return 'OpenAI';
      case 'openrouter':
        return 'OpenRouter';
      case 'ollama':
        return 'Ollama';
    }
  };
  return (
    <>
      <SectionHeading hint="Bring your own key. Argos never proxies — requests go straight from your machine to the provider you pick. The key is stored plaintext in settings.json.">
        AI provider
      </SectionHeading>
      <Row label="Provider" hint="Choose `none` to disable every AI-powered feature.">
        <Segmented<AiProvider>
          value={provider()}
          options={AI_PROVIDERS.map((p) => ({ value: p, label: providerLabel(p) }))}
          onChange={setAiProvider}
        />
      </Row>

      <Show when={provider() !== 'none'}>
        <Show when={provider() !== 'ollama'}>
          <Row label="API key" hint="Stored in settings.json (no OS keychain in v1).">
            <input
              type="password"
              spellcheck={false}
              autocomplete="off"
              class="w-[280px] rounded border border-border bg-bg-secondary px-2 py-1 font-mono text-[12px]"
              placeholder={
                provider() === 'anthropic'
                  ? 'sk-ant-…'
                  : provider() === 'openrouter'
                    ? 'sk-or-v1-…'
                    : 'sk-… or provider-specific'
              }
              value={settings().ai.apiKey}
              onInput={(e) => setAiApiKey(e.currentTarget.value)}
            />
          </Row>
        </Show>
        <Row
          label="Base URL"
          hint={
            provider() === 'openai-compatible'
              ? 'OpenAI default, or override for Groq / Together / Fireworks / a local proxy.'
              : provider() === 'openrouter'
                ? 'OpenRouter aggregator endpoint — exposes most major models behind one key.'
                : provider() === 'ollama'
                  ? 'Default points at a local Ollama server.'
                  : 'Override only if you proxy Anthropic through a custom gateway.'
          }
        >
          <input
            type="text"
            spellcheck={false}
            autocomplete="off"
            class="w-[280px] rounded border border-border bg-bg-secondary px-2 py-1 font-mono text-[12px]"
            value={settings().ai.baseUrl}
            onInput={(e) => setAiBaseUrl(e.currentTarget.value)}
          />
        </Row>
        <Row
          label="Model"
          hint={
            provider() === 'anthropic'
              ? 'e.g. claude-haiku-4-5, claude-sonnet-4-6'
              : provider() === 'openrouter'
                ? 'provider/model — e.g. anthropic/claude-haiku-4-5, openai/gpt-4o-mini, meta-llama/llama-3.3-70b-instruct'
                : provider() === 'ollama'
                  ? 'e.g. llama3.1:8b, qwen2.5:7b'
                  : 'e.g. gpt-4o-mini, llama-3.3-70b-instruct'
          }
        >
          <input
            type="text"
            spellcheck={false}
            autocomplete="off"
            class="w-[280px] rounded border border-border bg-bg-secondary px-2 py-1 font-mono text-[12px]"
            value={settings().ai.model}
            onInput={(e) => setAiModel(e.currentTarget.value)}
          />
        </Row>
      </Show>

      <div class="mt-4 border-t border-border pt-4" />
      <SectionHeading hint="Argos features that opt in to the configured provider.">
        Used for
      </SectionHeading>
      <Row label="Log file import" hint="Paste a logcat / Charles / nginx / proprietary log; the model returns extractable HTTP requests.">
        <span class="font-mono text-[11px] text-fg-secondary">
          {provider() === 'none' ? 'disabled' : 'enabled'}
        </span>
      </Row>
    </>
  );
}

function AdvancedTab() {
  async function doExport() {
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const target = await save({
        defaultPath: 'argos-settings.json',
        filters: [{ name: 'Argos settings', extensions: ['json'] }],
      });
      if (!target) return;
      const { writeTextFile } = await import('@tauri-apps/plugin-fs');
      await writeTextFile(target, JSON.stringify(settings(), null, 2));
      notify.success('Settings exported', target);
    } catch (e) {
      notifyError('Export failed', e);
    }
  }

  async function doImport() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const picked = await open({
        multiple: false,
        filters: [{ name: 'Argos settings', extensions: ['json'] }],
      });
      if (!picked || Array.isArray(picked)) return;
      const { readTextFile } = await import('@tauri-apps/plugin-fs');
      const text = await readTextFile(picked);
      const parsed = JSON.parse(text) as unknown;
      const merged = mergeWithDefaults(parsed);
      replaceAllSettings(merged);
      notify.success('Settings imported');
    } catch (e) {
      notifyError('Import failed', e);
    }
  }

  async function doReset() {
    if (!window.confirm('Reset all settings to defaults? This cannot be undone.')) return;
    try {
      await settingsReset();
      replaceAllSettings(JSON.parse(JSON.stringify(DEFAULT_SETTINGS)) as Settings);
      notify.success('Settings reset to defaults');
    } catch (e) {
      notifyError('Reset failed', e);
    }
  }

  const [checking, setChecking] = createSignal(false);
  const [installing, setInstalling] = createSignal(false);

  async function doCheck() {
    setChecking(true);
    try {
      await checkForUpdatesNow();
    } finally {
      setChecking(false);
    }
  }

  async function doInstall() {
    setInstalling(true);
    try {
      await installPendingUpdate();
    } finally {
      // Either we relaunched (this scope is gone) or it failed and the
      // pending update is still here for retry.
      setInstalling(false);
    }
  }

  return (
    <>
      <SectionHeading>Updates</SectionHeading>
      <Row
        label="Release channel"
        hint="Beta + nightly run ahead of stable; expect more rough edges."
      >
        <Segmented<ReleaseChannel>
          value={settings().updates.channel}
          options={RELEASE_CHANNELS.map((c) => ({
            value: c,
            label: c === 'stable' ? 'Stable' : c === 'beta' ? 'Beta' : 'Nightly',
          }))}
          onChange={setReleaseChannel}
        />
      </Row>
      <Row
        label={pendingUpdate() ? `Update available — v${pendingUpdate()!.version}` : 'Up to date'}
        hint={
          pendingUpdate()
            ? 'Download and install the new build. Argos will restart automatically.'
            : 'Argos checks once per launch. You can also check manually.'
        }
      >
        <Show
          when={pendingUpdate()}
          fallback={
            <button
              type="button"
              class="rounded border border-border bg-bg-secondary px-3 py-1.5 text-[12px] hover:bg-border disabled:opacity-50"
              disabled={checking()}
              onClick={() => void doCheck()}
            >
              {checking() ? 'Checking…' : 'Check for updates'}
            </button>
          }
        >
          <button
            type="button"
            class="rounded bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
            disabled={installing()}
            onClick={() => void doInstall()}
          >
            {installing() ? 'Installing…' : 'Install and restart'}
          </button>
        </Show>
      </Row>

      <div class="mt-4 border-t border-border pt-4" />
      <SectionHeading>Backup & restore</SectionHeading>
      <Row label="Export settings" hint="Saves the full settings.json — useful for sharing or backups.">
        <button
          type="button"
          class="rounded border border-border bg-bg-secondary px-3 py-1.5 text-[12px] hover:bg-border"
          onClick={() => void doExport()}
        >
          Export…
        </button>
      </Row>
      <Row label="Import settings" hint="Replaces all current settings with the contents of a JSON file.">
        <button
          type="button"
          class="rounded border border-border bg-bg-secondary px-3 py-1.5 text-[12px] hover:bg-border"
          onClick={() => void doImport()}
        >
          Import…
        </button>
      </Row>
      <Row label="Reset to defaults" hint="Deletes the settings file and restores shipped defaults.">
        <button
          type="button"
          class="rounded border border-border px-3 py-1.5 text-[12px] text-fg-error hover:bg-bg-secondary"
          onClick={() => void doReset()}
        >
          Reset all
        </button>
      </Row>

      <div class="mt-4 border-t border-border pt-4" />
      <SectionHeading>Diagnostics</SectionHeading>
      <Row label="Crash reports" hint="Inspect crash reports Argos has submitted from this install.">
        <button
          type="button"
          class="rounded border border-border bg-bg-secondary px-3 py-1.5 text-[12px] hover:bg-border"
          onClick={() => {
            closeSettings();
            openCrashLog();
          }}
        >
          View log…
        </button>
      </Row>
    </>
  );
}

// ---- primitives -----------------------------------------------------------

function Segmented<T>(props: {
  value: T;
  options: Array<{ value: T; label: string }>;
  onChange: (v: T) => void;
}) {
  return (
    <div class="inline-flex overflow-hidden rounded-md border border-border bg-bg-secondary text-[12px]">
      <For each={props.options}>
        {(opt) => (
          <button
            type="button"
            class="px-3 py-1.5 transition-colors"
            classList={{
              'bg-primary text-primary-foreground': opt.value === props.value,
              'text-fg-secondary hover:bg-border hover:text-fg-primary':
                opt.value !== props.value,
            }}
            onClick={() => props.onChange(opt.value)}
          >
            {opt.label}
          </button>
        )}
      </For>
    </div>
  );
}

function Toggle(props: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.checked}
      class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border-2 border-transparent transition-colors"
      classList={{
        'bg-primary': props.checked,
        'bg-border': !props.checked,
      }}
      onClick={() => props.onChange(!props.checked)}
    >
      <span
        class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-sm ring-0 transition-transform"
        classList={{ 'translate-x-4': props.checked, 'translate-x-0': !props.checked }}
      />
    </button>
  );
}

