---
title: argos run
description: Execute a collection or single request from the command line.
---

```sh
argos run <PATH> [OPTIONS]
```

Walks the workspace tree (or the subtree under `<PATH>`), runs every request
sequentially, prints a Mocha-like summary, and exits non-zero if any request
fails at the transport layer or any test inside a request fails.

## Path resolution

`<PATH>` can be:

- The workspace root or its `collections/` subdir → run everything.
- A folder under `collections/` → run that subtree.
- A single `.argos.yaml` request file → run just that request.

If `<PATH>` is outside any workspace, pass `--workspace <ROOT>` (or set
`ARGOS_WORKSPACE`) so the engine can find the matching `argos.yaml` manifest
and load environments.

## Options

| Flag                            | Effect                                                                 |
| ------------------------------- | ---------------------------------------------------------------------- |
| `--env <NAME>`                  | Activate an environment from `environments/`. `{{var}}` placeholders resolve against it. |
| `--bail`                        | Stop on the first failing request. Without `--bail` the runner finishes the whole tree. |
| `--iteration-data <FILE>`       | Data-driven runs. CSV or JSON; one full pass through `<PATH>` per row. |
| `--reporter <FORMAT>[=<PATH>]`  | Emit a structured report. Repeatable. See [Reporters](/cli/reporters/). |
| `--workspace <ROOT>`            | Workspace root override. Defaults to walking up from `<PATH>` looking for `argos.yaml`. |

## Examples

Run everything in the workspace:

```sh
argos run ./collections
```

Run a single folder against the `prod` environment, bail on the first
failure:

```sh
argos run ./collections/checkout --env prod --bail
```

Data-driven: re-run the whole tree once per CSV row, with row values bound
as env overrides:

```sh
argos run ./collections \
  --env ci \
  --iteration-data ./fixtures/users.csv
```

```csv
user_email,user_id
alice@example.com,42
bob@example.com,43
```

Inside any request, reference `{{user_email}}` / `{{user_id}}` and they'll
resolve to the current iteration's values (after the environment, which
takes lower precedence).

## What gets executed

- **REST** requests run through the HTTP engine (`reqwest` under the hood).
- **GraphQL** requests are translated to `POST <url>` with a JSON envelope
  `{ query, variables, operationName }` and run through the same path —
  scripts, reporters, env resolution all work.
- **WebSocket** requests are skipped with a stderr notice; long-lived
  sockets need session-aware tooling outside the scope of `argos run`.

## Exit codes

| Code | Meaning                                                            |
| ---- | ------------------------------------------------------------------ |
| `0`  | Every request succeeded and every test passed.                     |
| `1`  | One or more requests failed (transport, pre-request, or test).     |
| `2`  | Argument / configuration error (unknown environment, malformed iteration data, etc.). |

## Concurrency

Sequential by design. Argos is built for *correctness* over throughput at the
single-collection level; parallel execution lives in the planned **performance
mode** epic (E15) behind a separate `argos perf` subcommand.
