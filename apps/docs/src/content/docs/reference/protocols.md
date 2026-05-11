---
title: Protocols
description: REST, GraphQL, WebSocket — what's supported, what's deferred.
---

Argos started as a REST client and grew GraphQL and WebSocket variants
alongside. All three live in the same workspace, share environments and
auth, and round-trip through the YAML file format.

| Protocol     | UI editor | CLI `argos run` | File format       |
| ------------ | :-------: | :-------------: | :---------------: |
| REST         | ✓         | ✓               | `type: rest`      |
| GraphQL      | ✓         | ✓ (as POST)     | `type: graphql`   |
| WebSocket    | ✓         | skipped         | `type: websocket` |
| gRPC         | planned (P2) | planned        | planned           |
| SSE          | planned (P2) | planned        | planned           |
| MQTT         | planned (P2) | planned        | planned           |

## REST

The default. Full method coverage (`GET`, `POST`, `PUT`, `PATCH`,
`DELETE`, `HEAD`, `OPTIONS`), query / header tables with enabled
toggles, body modes (JSON, raw text, form-urlencoded — multipart is
deferred), and per-request auth.

Auth materialises into headers / query params at send time, so a copy-
as-curl preview matches what would actually go on the wire.

## GraphQL

POSTed as `{ query, variables, operationName? }` against the URL with
`Content-Type: application/json`. Both the desktop UI and the CLI use the
same path — `argos run` translates GraphQL requests into REST POSTs
inline, so scripts, reporters, and env resolution work without any
GraphQL-specific plumbing.

What's there:

- Query editor with line numbers / bracket matching.
- Variables JSON editor; invalid JSON is saved as a string and surfaced
  with an inline error message rather than silently dropped.
- Optional `operationName` for documents with multiple named operations.
- Headers, auth, and scripts work the same as REST.

What's coming in P2:

- Introspection-driven autocomplete + schema cache per environment.
- Response type hints from the schema.
- Convert REST → GraphQL migration helper.

## WebSocket

Persistent connection. Connect / disconnect drive the lifecycle; once
connected, messages flow in both directions and are logged in a
timestamped, ↑ / ↓-tagged timeline. Auto-Pong handles keep-alives so
proxied connections survive.

What's there:

- `ws://` and `wss://` URLs.
- `Sec-WebSocket-Protocol` subprotocols (e.g.
  `graphql-transport-ws`).
- Connection-time headers + auth.
- Outgoing message templates persisted in the request file (`messages`
  list).
- Closing the tab closes the connection.

What's not there yet:

- Reconnect / retry logic.
- Binary frames in the editor (received binary frames show up in the
  log as `<binary frame: N bytes>`).
- Driving WebSocket scenarios from `argos run` — long-lived sockets
  need session-aware tooling outside the scope of the sequential
  runner.

## Choosing a protocol when creating a request

Right-click the workspace tree or any folder → **New REST request** /
**New GraphQL request** / **New WebSocket connection**. The file is
created with the right `type:` discriminator and the editor for that
protocol opens in a tab.

## Switching protocols on an existing request

Not yet supported in v1. The advisor is: delete the request and
recreate with the new protocol — keeps the on-disk YAML clean and
avoids stale fields. A migration helper (T5.3.3) lands when there's a
clear pattern of REST ↔ GraphQL conversions.
