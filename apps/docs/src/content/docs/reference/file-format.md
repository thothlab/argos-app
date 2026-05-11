---
title: File format
description: Workspace, request, folder, environment YAML on disk.
---

Everything in a workspace is a YAML file. The desktop app and the CLI
read the same tree; nothing lives in a database, nothing lives in
`~/.config`.

## Layout

```
my-workspace/
├── argos.yaml                    # workspace manifest
├── collections/                  # request tree
│   ├── _folder.argos.yaml        # folder meta (headers / auth)
│   ├── list-users.argos.yaml     # one request
│   └── admin/
│       ├── _folder.argos.yaml
│       └── promote.argos.yaml
└── environments/
    ├── dev.env.argos.yaml
    └── prod.env.argos.yaml
```

File suffix tells the parser what it is — `.argos.yaml` = request,
`.env.argos.yaml` = environment, `argos.yaml` = the workspace manifest.
Folders are described by an optional `_folder.argos.yaml` inside the
folder; in its absence the directory is just a name.

## Workspace manifest

```yaml
kind: workspace
version: 1
name: my-api
description: Production checks for the public API.
config:
  collections_dir: collections
  environments_dir: environments
  runs_dir: runs
  default_environment: dev
```

`version` exists for forward-compatibility; today only `1` is valid.
`config` paths are relative to the manifest's directory — moving the
workspace folder around doesn't break anything.

## Request

```yaml
kind: request
name: Create user                  # display name; doesn't have to match filename
description: Creates a new user.   # optional
type: rest                         # rest | graphql | websocket

# REST-specific fields (when type: rest)
method: POST
url: "{{baseUrl}}/users"
query:
  - { name: dry_run, value: "true", enabled: false }
headers:
  - { name: Accept, value: application/json, enabled: true }
auth:
  type: bearer
  token: "{{token}}"
body:
  kind: json
  value:
    name: Alice
    role: admin

# Optional scripts (any variant)
scripts:
  pre_request: |
    bru.req.setHeader('X-Request-Id', crypto.randomUUID());
  tests: |
    bru.test('status 201', () => bru.expect(bru.res.status).toBe(201));

# Optional reference to an OpenAPI fragment for schema-aware validation
# schema_ref: openapi/users.yaml#/paths/~1users/post
```

### GraphQL variant

```yaml
kind: request
name: List posts
type: graphql
url: "{{baseUrl}}/graphql"
query: |
  query ListPosts($limit: Int) {
    posts(limit: $limit) { id title }
  }
variables:
  limit: 10
operation_name: ListPosts
headers:
  - { name: X-Apollo, value: "true", enabled: true }
auth:
  type: bearer
  token: "{{token}}"
```

### WebSocket variant

```yaml
kind: request
name: Chat socket
type: websocket
url: wss://chat.example.com/socket
subprotocols:
  - graphql-transport-ws
headers:
  - { name: Authorization, value: "Bearer {{token}}", enabled: true }
messages:
  - { name: Ping, body: '{"type":"ping"}' }
```

## Folder

```yaml
# collections/admin/_folder.argos.yaml
kind: folder
name: Admin
description: Authenticated operations.
headers:
  - { name: X-Admin-Token, value: "{{adminToken}}", enabled: true }
auth:
  type: bearer
  token: "{{adminToken}}"
```

Folder `headers` apply to every request inside (request-level headers
override by name). Folder `auth` is used when a request opts into
inheritance (`auth: { type: inherit }` or no `auth` at all).

## Environment

```yaml
kind: environment
name: prod
variables:
  - { name: baseUrl, value: "https://api.example.com", enabled: true }
  - { name: clientId, value: my-client-id, enabled: true }
secrets:
  - { name: token, value: "{{TOKEN_FROM_CI}}", enabled: true }
```

`variables` are plain values committed to git. `secrets` are listed in
the same file but with the expectation that the value is either:

- A reference to an injected CI variable (`{{TOKEN_FROM_CI}}`), or
- Encrypted with the sops/age workflow (planned, E12).

## Encoding rules

- UTF-8.
- LF line endings — Argos rewrites CRLF to LF on save.
- Two-space indent.
- Keys preserve insertion order on round-trip (we use a structure-
  preserving YAML serializer).
- Comments survive round-trips at the top level; comments inside
  collection-style fields may be lost.

## Editing by hand

Open any `.argos.yaml` in your editor of choice — `vim`, VS Code,
JetBrains IDEs. The Argos desktop app watches the workspace folder and
reloads automatically when files change on disk. Saves from the UI go
through atomic temp-file rename to avoid half-written files.
