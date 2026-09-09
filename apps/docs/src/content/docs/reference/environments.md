---
title: Environments
description: Variable resolution, secrets, and the {{var}} syntax.
---

An environment is a named bag of `name → value` pairs Argos resolves
`{{var}}` placeholders against. Workspaces typically have one
environment per deployment (`dev`, `staging`, `prod`, `ci`); switching
the active environment from the toolbar changes which file's values
the resolver uses.

## Where they live

`<workspace>/environments/<name>.env.argos.yaml`, one file per
environment. The active environment is part of UI state — not a file
edit — so the workspace stays diff-clean as you flip between them.

## Placeholder syntax

```
{{name}}
```

Resolved everywhere strings appear in a request: URL, query, header
names, header values, body content, body field values, auth tokens.
Unresolved placeholders pass through untouched so a typo doesn't fail
silently — you'll see `{{baesUrl}}` in the actual URL the engine sent.

The resolver is **single-pass** — `{{a}}` resolving to `{{b}}` does not
recurse. Nested templating belongs in a pre-request script:

```js
const ref = bru.env.get('a');           // '{{b}}'
const val = bru.env.get(ref.slice(2, -2));
bru.req.setHeader('X-Final', val);
```

## Dynamic variables

Some fields must differ on every send — a `request_id`, a nonce, a
cache-buster. Instead of storing a constant and editing it by hand
before each run, reference a `$`-prefixed **dynamic variable**. The
engine computes the value at send time; nothing is stored in the
request file.

```
X-Request-Id: {{$randomUuid}}
```

| Variable                | Value                                        |
| ----------------------- | -------------------------------------------- |
| `{{$randomUuid}}`       | UUID v4, fresh on every send                  |
| `{{$guid}}`             | alias of `$randomUuid` (Postman compatibility)|
| `{{$randomUUID}}`       | alias of `$randomUuid` (Postman compatibility)|
| `{{$randomInt}}`        | random 32-bit unsigned integer                |
| `{{$randomHex}}`        | random 16-character hex string                |
| `{{$randomAlphaNumeric}}` | random 16-character alphanumeric string     |
| `{{$timestamp}}`        | Unix epoch, milliseconds                      |
| `{{$timestampSeconds}}` | Unix epoch, seconds                           |
| `{{$isoTimestamp}}`     | RFC 3339 / ISO 8601 timestamp at UTC          |

Type `{{` in the URL bar or in any Params / Headers value to get the
list as autocomplete, alongside the active environment's own variables.

### One value per request

Each dynamic variable is generated **once per request** and reused
everywhere it appears in that request. A `{{$randomUuid}}` in a header
and the same placeholder in the body carry the *same* id — which is what
a correlation id needs — and the next send generates a new one. The same
holds for `{{$timestamp}}`: one request cannot straddle two timestamps.

When a single request genuinely needs two independent values of the same
kind, add a `#label` suffix. The label only separates them; it is not
part of the generated value:

```json
{ "outer_id": "{{$randomUuid#outer}}", "inner_id": "{{$randomUuid#inner}}" }
```

An environment variable with the same name wins over the built-in, so a
test environment can pin `$timestamp` to a fixed value to make request
snapshots reproducible.

Random values come from a fast non-cryptographic PRNG. They are fine as
ids and cache-busters; do not use them as secrets or tokens.

## Precedence

Highest to lowest:

1. **Iteration data row** — `--iteration-data` values are folded into
   the env map for the duration of one iteration, on top of everything
   below.
2. **Pre-request `bru.env.set`** — staged for the rest of the run.
3. **Active environment file** — `secrets` first, then `variables`
   (secrets win when both define the same name).
4. **Dynamic built-in** — a `$`-prefixed name from the table above,
   computed once per request.
5. **Nothing** — placeholder passes through untouched.

The `CI` environment + `--iteration-data` pattern is the workhorse
combination for parameterised CI runs: a stable workspace + a CSV that
the runner sweeps.

## Secrets vs variables

`variables` and `secrets` live in the same file but are surfaced
differently:

- The UI hides `secrets` values behind a reveal toggle.
- Exports (Postman, HAR) include `variables` and **skip** `secrets`.
- The schema preserves the distinction so future encrypted-secrets
  support (E12, sops/age) drops in without a format change.

For now, in CI: keep secret values as `{{ENV_FROM_CI}}` references and
inject the actual value via the runner's secrets mechanism. Argos's
resolver picks them up the same way as any other env var.

## Disabled entries

```yaml
variables:
  - { name: baseUrl, value: "https://api.example.com", enabled: true }
  - { name: experimental_baseUrl, value: "https://beta.api", enabled: false }
```

`enabled: false` means "the file has it but the resolver ignores it" —
useful for keeping a hot-swap candidate around without renaming. The
editor renders a checkbox; the runner respects the bit.

## Creating an environment

In the UI: **File → Environment → New** (or the picker dropdown's
"Manage environments" entry). The created file lives at
`environments/<slug>.env.argos.yaml` and is loaded on the next workspace
reload.

From the CLI / editor: just drop a file in `environments/`. The desktop
app's file watcher picks it up; the CLI sees it on next `argos run`.

## Importing variables

Each importer maps collection-level variables into a fresh environment:

| Source         | Goes to                                                         |
| -------------- | --------------------------------------------------------------- |
| **Postman**    | `variable` block → new `environments/<collection-slug>.env.argos.yaml`. |
| **Insomnia**   | `environment` documents → one env file each.                    |
| **Bruno**      | `.env.bru` and per-folder env blocks → one env file per Bruno env. |
| **OpenAPI**    | `servers[0].url` → a `baseUrl` variable.                        |

The toast notification on import surfaces the path it wrote.
