/**
 * Auth tab — type picker plus a small form per auth kind.
 *
 * Covers the four basic auth schemes (Inherit / Bearer / Basic / API key).
 * OAuth 2 (client-credentials, authorization-code-pkce, device-code) ships
 * as a follow-up — those need a token exchange flow on the backend rather
 * than a static header injection.
 *
 * Auth values support `{{var}}` substitution like everything else; the
 * resolver runs server-side before send.
 */

import { For, Match, Show, Switch } from 'solid-js';

import { activeTabId } from '../stores/tabs';
import { getRequest, setAuth, type ApiKeyLocation, type DraftAuth } from '../stores/request';

const TYPES: Array<{ kind: DraftAuth['kind']; label: string; hint: string }> = [
  { kind: 'none', label: 'No auth', hint: 'No Authorization header sent.' },
  { kind: 'inherit', label: 'Inherit', hint: 'Use whatever the parent folder defines.' },
  { kind: 'bearer', label: 'Bearer token', hint: 'Authorization: Bearer <token>.' },
  { kind: 'basic', label: 'Basic auth', hint: 'Authorization: Basic <base64(user:pass)>.' },
  { kind: 'api_key', label: 'API key', hint: 'Custom header / query param / cookie.' },
];

export default function AuthTab() {
  const tabId = () => activeTabId();
  const draft = () => {
    const id = tabId();
    return id ? getRequest(id) : null;
  };
  const auth = (): DraftAuth => draft()?.auth ?? { kind: 'none' };

  function update(next: DraftAuth) {
    const id = tabId();
    if (id) setAuth(id, next);
  }

  function changeKind(kind: DraftAuth['kind']) {
    switch (kind) {
      case 'none':
      case 'inherit':
        update({ kind });
        break;
      case 'bearer':
        update({ kind: 'bearer', token: '' });
        break;
      case 'basic':
        update({ kind: 'basic', username: '', password: '' });
        break;
      case 'api_key':
        update({ kind: 'api_key', name: '', value: '', location: 'header' });
        break;
    }
  }

  const currentHint = () => TYPES.find((t) => t.kind === auth().kind)?.hint ?? '';

  return (
    <div class="flex flex-col gap-3 p-4">
      <div class="flex flex-wrap items-center gap-3">
        <span class="text-fg-secondary">Type:</span>
        <For each={TYPES}>
          {(t) => (
            <label class="flex cursor-pointer items-center gap-1.5 text-[13px]">
              <input
                type="radio"
                name="auth-type"
                class="accent-primary"
                checked={auth().kind === t.kind}
                onChange={() => changeKind(t.kind)}
              />
              <span>{t.label}</span>
            </label>
          )}
        </For>
      </div>

      <p class="text-[12px] text-fg-secondary">{currentHint()}</p>

      <Switch>
        <Match when={auth().kind === 'bearer'}>
          {(_) => {
            const a = () => auth() as Extract<DraftAuth, { kind: 'bearer' }>;
            return (
              <Field
                label="Token"
                value={a().token}
                placeholder="{{token}} or a literal value"
                onInput={(v) => update({ kind: 'bearer', token: v })}
                monospace
              />
            );
          }}
        </Match>

        <Match when={auth().kind === 'basic'}>
          {(_) => {
            const a = () => auth() as Extract<DraftAuth, { kind: 'basic' }>;
            return (
              <>
                <Field
                  label="Username"
                  value={a().username}
                  onInput={(v) => update({ kind: 'basic', username: v, password: a().password })}
                />
                <Field
                  label="Password"
                  value={a().password}
                  onInput={(v) => update({ kind: 'basic', username: a().username, password: v })}
                  type="password"
                />
              </>
            );
          }}
        </Match>

        <Match when={auth().kind === 'api_key'}>
          {(_) => {
            const a = () => auth() as Extract<DraftAuth, { kind: 'api_key' }>;
            return (
              <>
                <div class="flex items-center gap-2 text-[13px]">
                  <span class="w-24 shrink-0 text-fg-secondary">Add to:</span>
                  <For each={['header', 'query', 'cookie'] as ApiKeyLocation[]}>
                    {(loc) => (
                      <label class="flex cursor-pointer items-center gap-1.5">
                        <input
                          type="radio"
                          name="api-key-location"
                          class="accent-primary"
                          checked={a().location === loc}
                          onChange={() =>
                            update({ kind: 'api_key', name: a().name, value: a().value, location: loc })
                          }
                        />
                        <span>{loc}</span>
                      </label>
                    )}
                  </For>
                </div>
                <Field
                  label="Name"
                  value={a().name}
                  placeholder={
                    a().location === 'header' ? 'X-Api-Key' : a().location === 'query' ? 'api_key' : 'session'
                  }
                  onInput={(v) =>
                    update({ kind: 'api_key', name: v, value: a().value, location: a().location })
                  }
                  monospace
                />
                <Field
                  label="Value"
                  value={a().value}
                  placeholder="{{apiKey}} or a literal value"
                  onInput={(v) =>
                    update({ kind: 'api_key', name: a().name, value: v, location: a().location })
                  }
                  monospace
                />
              </>
            );
          }}
        </Match>
      </Switch>

      <Show when={auth().kind === 'inherit'}>
        <p class="rounded border border-border bg-bg-secondary px-3 py-2 text-[12px] text-fg-secondary">
          Folder-level auth resolution lands in a follow-up — for now `inherit` falls back to no auth.
        </p>
      </Show>
    </div>
  );
}

function Field(props: {
  label: string;
  value: string;
  placeholder?: string;
  type?: string;
  monospace?: boolean;
  onInput: (v: string) => void;
}) {
  return (
    <label class="flex items-center gap-2 text-[13px]">
      <span class="w-24 shrink-0 text-fg-secondary">{props.label}</span>
      <input
        type={props.type ?? 'text'}
        spellcheck={false}
        autocomplete="off"
        class="h-8 flex-1 rounded border border-border bg-bg-card px-2 outline-none focus:border-primary"
        classList={{ 'font-mono': !!props.monospace }}
        value={props.value}
        placeholder={props.placeholder}
        onInput={(e) => props.onInput(e.currentTarget.value)}
      />
    </label>
  );
}
