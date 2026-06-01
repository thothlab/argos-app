---
title: Importing collections
description: Bring Postman, Insomnia, Bruno, OpenAPI / Swagger, or curl into an Argos workspace.
---

Argos imports every common collection format directly into the workspace as
native YAML — no proprietary database, no lock-in.

| Source              | Format                  | Notes                                                              |
| ------------------- | ----------------------- | ------------------------------------------------------------------ |
| **Postman**         | v2.1 collection JSON    | `pm.*` script shim runs imported scripts unchanged. v2.0 unsupported. |
| **Insomnia**        | v4 export JSON          | Workspaces, requests, env vars; gRPC entries skipped.              |
| **Bruno**           | `.bru` collection dir   | Folders mirrored 1:1; `meta`/`auth` blocks preserved.              |
| **OpenAPI 3.0/3.1** | JSON or YAML spec       | One request per `paths.{path}.{method}`; folders by `tags[0]`.     |
| **Swagger 2.0**     | JSON or YAML spec       | Converted to 3.0 shape in-memory: `host`+`basePath`+`schemes` → `servers`, body / formData params → `requestBody`, `definitions` → `components.schemas`. |
| **cURL**            | `curl …` shell command  | Paste from devtools / docs; multi-line `\` continuations accepted. |
| **Log file (AI)**   | Pasted log text         | Bring your own key (Anthropic / OpenAI / Ollama) — the model extracts HTTP requests from any log format. |

## Drag and drop

The fastest path: drag the source file (or, for Bruno, the collection folder)
anywhere onto the Argos window. A drop overlay appears, the format is
detected on the Rust side, and a confirmation modal previews the import
before it writes anything to disk.

Detection rules:

- A directory containing `bruno.json` → **Bruno**.
- A file whose first ~64 KB JSON parses with `info.schema` containing `"v2.1"` → **Postman**.
- `"_type":"export"` or `"__export_format"` → **Insomnia**.
- `"openapi":"3.x"` or `"swagger":"2.0"` (JSON), `openapi: 3.x` or `swagger: "2.0"` (YAML) → **OpenAPI / Swagger**.
- Anything else → an error toast pointing you at the explicit **File → Import**
  menu.

Multi-file drop takes only the first file with a console warning; the wizard
is one-thing-at-a-time in v1.

## File → Import menu

For URLs, clipboard cURL, or when the auto-detection trips up:

- **File → Import → From cURL command** — paste a single command, hit
  **Import**.
- **File → Import → From Postman v2.1 (JSON)…**
- **File → Import → From Insomnia v4 (JSON)…**
- **File → Import → From Bruno collection (folder)…**
- **File → Import → From OpenAPI / Swagger (JSON / YAML)…** — handles both OpenAPI 3.x and Swagger 2.0.

Each opens a system file picker and runs the importer. Counts of requests +
folders + variables land in a toast on success.

## What survives an import

Argos's request model is a strict superset of the source formats it imports
from, so the round-trip is lossless for the supported variants:

| Field              | Postman | Insomnia | Bruno | OpenAPI |
| ------------------ | :-----: | :------: | :---: | :-----: |
| Method + URL       | ✓       | ✓        | ✓     | ✓       |
| Headers (enabled)  | ✓       | ✓        | ✓     | ✓       |
| Query params       | ✓       | ✓        | ✓     | ✓       |
| JSON / text body   | ✓       | ✓        | ✓     | ✓       |
| Form-urlencoded    | ✓       | ✓        | ✓     | ✓       |
| Multipart files    | ✗ (placeholder) | ✗ | ✗ | ✗     |
| Auth: bearer/basic/apikey | ✓ | ✓        | ✓     | ✓       |
| Auth: oauth2 / oidc | partial | partial | partial | ✗ (note in description) |
| Pre-request script | ✓       | ✓        | ✓     | n/a     |
| Tests script       | ✓       | ✓        | ✓     | n/a     |
| Folder inheritance | ✓       | ✓        | ✓     | ✓ (by tag) |
| Collection vars    | ✓ (→ env file) | ✓ | ✓ | ✓ (`{{baseUrl}}`) |

Multipart file uploads aren't supported by the Argos request type yet —
imports surface them as a placeholder field so a human can wire the file in
manually.

## OpenAPI specifics

When importing an OpenAPI 3.x document:

- `servers[0].url` becomes a `{{baseUrl}}` variable in a fresh environment.
- Path templates `{name}` are rewritten to Argos's `{{name}}` so the env
  resolver picks them up.
- Parameters are seeded from `example` → `schema.example` → `schema.default` →
  first `enum` entry, in that order.
- Request bodies prefer `application/json` examples; without one, a one-level
  schema stub is generated. Cyclic `$ref` graphs are bounded at depth 4 so a
  recursive `Node` schema can't blow the stack.
- `security` + `components.securitySchemes` map to Argos auth types: `http`
  bearer → Bearer; `http` basic → Basic; `apiKey` → ApiKey (header / query /
  cookie). OAuth2 and OIDC are deferred to a future revision.

### Swagger 2.0 specifics

Swagger 2.0 documents (`swagger: "2.0"`) are accepted by the same
importer — the parser rewrites the document to a 3.0 shape in memory
before the rest of the logic runs. What's rewritten:

| Swagger 2.0                                       | OpenAPI 3.0 equivalent                                             |
| ------------------------------------------------- | ------------------------------------------------------------------ |
| `host`, `basePath`, `schemes[0]`                  | `servers[0].url = "{scheme}://{host}{basePath}"`                   |
| `definitions`                                     | `components.schemas`                                               |
| Root `parameters` / `responses`                   | `components.parameters` / `components.responses`                   |
| `securityDefinitions`                             | `components.securitySchemes` (with `type: basic` → `type: http, scheme: basic`) |
| Per-operation `parameters[in: body]`              | `requestBody.content["application/json"]` (or `consumes[0]`)       |
| Per-operation `parameters[in: formData]`          | `requestBody.content["application/x-www-form-urlencoded"]` schema  |
| `$ref: "#/definitions/X"`                         | `$ref: "#/components/schemas/X"` (similar for parameters / responses / securityDefinitions) |
| `consumes` / `produces` (op-level or root)        | media types on the matching `requestBody` / `responses.*` content  |

OAuth2 flow shape differs between versions; we emit a stub
`components.securitySchemes.<name>.flows.<mapped-flow>` so the field
exists, but the OpenAPI auth walker treats unknown OAuth2 detail as a
description note either way.

File uploads in `parameters: [{in: formData, type: file}]` become
`{type: string, format: binary}` in the schema — generators can at
least produce a placeholder field; full multipart support is on the
roadmap.

## Log file (AI) specifics

Logs come in too many shapes — Android logcat with OkHttp's
`HttpLoggingInterceptor`, Charles session text, nginx access lines,
Spring's Logbook output, ad-hoc `console.log({ url, headers, body })`
dumps. Instead of shipping a parser per format, Argos lets you point
an LLM at the paste.

### Setup

**Settings → AI → Provider** picks where the request goes:

- `Anthropic` — Claude API directly. Default model `claude-haiku-4-5`.
- `OpenAI` — OpenAI itself or any URL with an OpenAI-style
  `/chat/completions` endpoint (Groq, Together, Fireworks, a self-hosted
  gateway, …). Pick any model name your endpoint exposes.
- `OpenRouter` — aggregator that exposes most major models under one key.
  Use the `provider/model` syntax: `anthropic/claude-haiku-4-5`,
  `openai/gpt-4o-mini`, `meta-llama/llama-3.3-70b-instruct:free`, etc.
- `Ollama` — local Ollama server, default `http://127.0.0.1:11434`,
  no API key needed.

Paste your provider's API key, optionally override the base URL, pick
a model. The key is stored plaintext in `settings.json` — Argos has no
OS-keychain integration in v1.

### Privacy

Argos **never proxies AI traffic**. The log + your API key go straight
from the desktop binary to the host you configured (`api.anthropic.com`,
`api.openai.com`, `openrouter.ai`, your local Ollama, etc). The
destination domain is shown in the import modal next to the byte count,
so you see where the paste lands at the moment you click Extract — not
in docs you didn't read.

When the OpenRouter provider is used, Argos additionally sends an
`HTTP-Referer: https://argos.thothlab.tech` and `X-Title: Argos`
header — this is OpenRouter's standard attribution mechanism for their
public app leaderboard; it doesn't change pricing or routing.

The opt-in modal cap is 50 KB. Larger logs would (a) exceed many
providers' context windows, (b) cost real tokens, (c) take long enough
that the app feels frozen. Trim or split first; the modal shows live
byte count.

### Flow

1. **File → Import → From log file (AI)…** opens the modal.
2. Paste the log into the textarea. Byte count updates live.
3. Click **Extract requests**. The model returns a list of HTTP
   requests it found in the log.
4. Review the list — uncheck anything you don't want (the model
   sometimes picks up noise like health-check probes).
5. Pick a target. Two modes:
   - **New folder** (default) — the editable folder name defaults to
     `AI import HH:MM`; the new folder is created under
     `<workspace>/collections/` (or the workspace root if there's no
     `collections/` dir). If a folder with that slug already exists,
     a numeric suffix (`-2`, `-3`, …) is appended automatically.
   - **Add to existing folder** — picks any folder currently in the
     workspace tree; request files are appended into it without
     touching existing requests. Disabled when the workspace has no
     folders yet.
6. Click **Import selected**. The chosen requests are written into the
   target.

The extracted shape mirrors the standard Argos request: method, URL,
headers (verbatim — including auth tokens for replay), query, body
(JSON / text / form-urlencoded). Folder grouping, tag extraction, and
multi-step session detection are deferred to future revisions.

### What this is good for

- One-off "I see this request in the log, let me replay it" workflows.
- Logs from frameworks Argos doesn't have a native parser for.
- Heterogeneous logs from multiple sources concatenated together.

### What this isn't

- A replacement for native parsers when one exists. If you have a
  Postman / Bruno / OpenAPI export, those are deterministic — use the
  matching importer.
- Batch processing of 100 MB log files. Cap is 50 KB per extract.
- Free. The user's API key incurs the user's provider bill; budget
  ~$0.001–$0.01 per Extract for Anthropic Haiku / OpenAI Mini sized
  models on typical paste sizes.

## Exporting

The reverse direction — **File → Export → Postman v2.1 collection** — writes
a `.postman_collection.json` next to the workspace. Folders, requests,
headers, query, body, auth and scripts round-trip. Non-REST requests (GraphQL,
WebSocket) emit placeholder entries with a description noting the
non-representability.
