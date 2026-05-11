---
title: Codegen targets
description: Languages the "Copy as code" dropdown emits for.
---

The Copy button next to the Send button on every request opens a
dropdown of code-generation targets. The snippet is the request as it
would go on the wire — env variables resolved, auth headers
materialised, query merged into the URL.

## Available targets

| Target              | What you get                                                            |
| ------------------- | ----------------------------------------------------------------------- |
| **cURL**            | Multi-line `curl` with `\` continuations. Single-quoted args, safe under bash / zsh / dash. |
| **JS — browser**    | `await fetch(...)` snippet with a comment hint to paste into devtools.   |
| **JS — Node**       | Same body as browser fetch, with a comment marking Node 18+ as the target. No `node-fetch` import. |
| **Python — requests** | `import requests`, top-level script. Uses `data=` for forms and `json=` for JSON. |
| **Go — net/http**   | Complete `package main` — runs as `go run snippet.go` after `go mod init`. |
| **Rust — reqwest**  | `reqwest::blocking::Client` script; one `cargo add reqwest --features blocking,json` away from compiling. |

## What gets preserved

- **Method, URL, query, headers, body** — exact bytes the engine would
  send.
- **Auth** — already materialised onto the request before the
  generator runs, so a Bearer token becomes an `Authorization` header
  in the snippet without a separate auth helper.
- **Content-Type** — the generator only adds the default
  `Content-Type` for the body type when the user hasn't set one
  explicitly. Custom content-types (e.g. `application/vnd.api+json`)
  survive untouched.

## What's not in the snippet

- **Scripts.** Pre-request / tests are Argos-only concepts; they're
  not exported.
- **Environment lookup.** Variables resolve to their current values
  before generation — the snippet is "this run", not "the workspace".
- **Cookies.** Argos doesn't surface a cookie jar to scripts; the
  generator doesn't manage one either.
- **Binary bodies.** `Raw` bodies emit a `// TODO: binary body` comment
  instead of trying to inline the bytes. Use `--data-binary @path` in
  curl for those, or hand-edit the snippet.

## Picking a target

- **cURL** — universal. Best for sharing in chat, in tickets, in
  Stack Overflow.
- **JS — browser** — quickest path to reproduce in devtools. Pair
  with the network panel for debugging.
- **JS — Node / Python — requests** — most concise for scripting
  follow-up actions.
- **Go / Rust** — embed the request into a service. Each is a complete
  `main` you can drop into a fresh module.

## Java?

Skipped on purpose. Postman has a robust Java codegen — Argos's ICP is
web / Go / Python and Java users will likely keep their Postman seat.
If you'd use it, file an issue and we'll prioritise.
