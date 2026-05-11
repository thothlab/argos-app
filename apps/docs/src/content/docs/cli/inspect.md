---
title: argos list / validate
description: Inspect a workspace without running anything.
---

Two read-only sibling commands to `argos run`. Useful in CI for smoke checks
("does the workspace still parse?") and in editors when wiring up a new
collection.

## `argos list`

Prints the workspace tree — folders, requests, methods, URLs. Output is
plain text; pipe through `grep` / `awk` if you need to script around it.

```sh
argos list [PATH]
```

`PATH` defaults to the current directory; if omitted, Argos walks up to find
the nearest `argos.yaml`. Pass `--workspace <ROOT>` to skip the walk.

Example:

```
Workspace: my-api
Root:      /Users/dev/my-api
Envs:      dev, prod, ci

📁 collections
  - GET    {{baseUrl}}/users           [List users]
  - POST   {{baseUrl}}/users           [Create user]
  📁 admin
    - POST   {{baseUrl}}/promote       [Promote]
    - GQL    {{baseUrl}}/graphql       [List posts]
    - WS     wss://realtime.x/socket   [Activity feed]
```

Protocol pill on the left tells you what each entry is — `GET/POST/…`
for REST, `GQL` for GraphQL, `WS` for WebSocket.

## `argos validate`

Opens the workspace and reports whether every YAML file parses cleanly.
Returns exit code `0` for "ok", `1` for "at least one file failed".

```sh
argos validate [PATH]
```

Output on success:

```
✓ my-api valid — 4 folder(s), 12 request(s), 3 env(s)
```

Output on failure surfaces the offending file plus the parser error, then
exits non-zero. Add this to CI before `argos run` to catch malformed YAML
early — pyramid of doom debugging averted.

## `argos version` (and the default subcommand)

Calling `argos` with no subcommand prints the embedded `argos-core` version
plus a hint:

```
argos 0.1.0
Run `argos --help` to see commands.
```

The `argos-core` version is the source of truth — desktop, CLI, and WASM
bindings all share it, so a "0.1.0" CLI talks to a "0.1.0" UI without drift.

## Common workflow

```sh
argos validate && argos run ./collections --reporter junit=report.xml
```

If `validate` fails, `run` never executes. Keeps CI runs cheap.
