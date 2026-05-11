---
title: Importing collections
description: Bring Postman, Insomnia, Bruno, OpenAPI, or curl into an Argos workspace.
---

Argos imports every common collection format directly into the workspace as
native YAML — no proprietary database, no lock-in.

| Source              | Format                  | Notes                                                              |
| ------------------- | ----------------------- | ------------------------------------------------------------------ |
| **Postman**         | v2.1 collection JSON    | `pm.*` script shim runs imported scripts unchanged. v2.0 unsupported. |
| **Insomnia**        | v4 export JSON          | Workspaces, requests, env vars; gRPC entries skipped.              |
| **Bruno**           | `.bru` collection dir   | Folders mirrored 1:1; `meta`/`auth` blocks preserved.              |
| **OpenAPI 3.0/3.1** | JSON or YAML spec       | One request per `paths.{path}.{method}`; folders by `tags[0]`.     |
| **cURL**            | `curl …` shell command  | Paste from devtools / docs; multi-line `\` continuations accepted. |

## Drag and drop

The fastest path: drag the source file (or, for Bruno, the collection folder)
anywhere onto the Argos window. A drop overlay appears, the format is
detected on the Rust side, and a confirmation modal previews the import
before it writes anything to disk.

Detection rules:

- A directory containing `bruno.json` → **Bruno**.
- A file whose first ~64 KB JSON parses with `info.schema` containing `"v2.1"` → **Postman**.
- `"_type":"export"` or `"__export_format"` → **Insomnia**.
- `"openapi":"3.x"` (JSON) or `openapi: 3.x` (YAML) → **OpenAPI**.
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
- **File → Import → From OpenAPI 3.x (JSON / YAML)…**

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

## Exporting

The reverse direction — **File → Export → Postman v2.1 collection** — writes
a `.postman_collection.json` next to the workspace. Folders, requests,
headers, query, body, auth and scripts round-trip. Non-REST requests (GraphQL,
WebSocket) emit placeholder entries with a description noting the
non-representability.
