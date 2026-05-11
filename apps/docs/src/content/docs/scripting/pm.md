---
title: pm.* compatibility
description: Postman script API supported by Argos's sandbox — what works, what's a no-op, what's missing.
---

Argos ships a `pm.*` shim that maps Postman's scripting API onto the
native `bru.*` surface. The goal: an existing Postman pre-request /
tests script pastes in unchanged for the common cases. The shim is
**pure JS** — no extra Rust surface, so it stays close to Postman's own
semantics.

## What works

### Environment / variables

```js
pm.environment.get(name)
pm.environment.set(name, value)
pm.environment.unset(name)
pm.environment.has(name)

pm.collectionVariables.get(name)
pm.collectionVariables.set(name, value)
pm.collectionVariables.unset(name)

pm.globals.get(name)
pm.globals.set(name, value)
pm.globals.unset(name)

pm.variables.get(name)
pm.variables.set(name, value)
```

`collectionVariables` and `globals` live as in-memory maps for the run —
they don't persist to disk. `pm.variables` reads from environment first,
collection vars second, globals third (matching Postman's resolution
order).

### Request

```js
pm.request.method                       // 'GET' | 'POST' | …
pm.request.url.toString()
pm.request.headers.get(name)
pm.request.headers.upsert({ key, value })
pm.request.headers.remove(name)
pm.request.body.raw                     // text body
pm.request.body.update({ mode, raw })   // delegates to bru.req.setTextBody / setJsonBody
```

### Response (tests only)

```js
pm.response.code                        // ← bru.res.status
pm.response.status                      // alias
pm.response.json()
pm.response.text()
pm.response.headers.get(name)
pm.response.responseTime                // ms, when available
```

### Tests

```js
pm.test(name, fn)                       // ← bru.test
```

### Assertions (Chai-like)

```js
pm.expect(value).to.equal(expected)
pm.expect(value).to.eql(expected)       // deep equality
pm.expect(value).to.be.a('number')      // 'string' | 'object' | 'array' | …
pm.expect(value).to.be.true             // value === true
pm.expect(value).to.be.false
pm.expect(value).to.be.null
pm.expect(value).to.be.undefined
pm.expect(value).to.be.truthy
pm.expect(value).to.be.falsy
pm.expect(value).to.include(sub)
pm.expect(value).to.match(regex)
pm.expect(value).to.have.lengthOf(n)
pm.expect(value).to.have.property(name)
```

The shim is intentionally subset-only — chains like
`pm.expect(x).to.be.an.instanceof(Foo)` are not implemented. If you need
something missing, prefer the equivalent `bru.expect(...)` shape.

## What's a no-op (with a warning)

- **`pm.sendRequest(...)`** — network from scripts is intentionally
  absent. Calls log a warning to `bru.warn` and return immediately.
- **`pm.iterationData.*`** — Argos surfaces iteration data through the
  *environment* (data row values are bound as env overrides), so
  Postman's separate "iteration data" namespace is replaced by
  `pm.environment.get(...)`. The shim does map `pm.iterationData.get` to
  `pm.environment.get` for compatibility, but
  `pm.iterationData.iterator` / `.toObject()` are not available.
- **`pm.cookies`** — cookie jar manipulation isn't surfaced to scripts.

## What's missing (no shim)

- `pm.sendRequest`'s success path — see above.
- `pm.execution.*` — Postman's execution-control API.
- `pm.vault` — secret store.
- `postman.setNextRequest(...)` — flow control. Use `--bail` plus the
  runner's ordering instead.
- Workflow control statements like `postman.setNextRequest` only make
  sense in a runner with cursor semantics; Argos's sequential runner
  doesn't support skipping ahead.

If your existing Postman tests rely on any of these, the migration path
is:

1. Run the import (drag-drop → confirm).
2. Open the failing request, switch its **Scripts** tab to a `bru.*`
   rewrite of the unsupported bit.
3. Leave the rest of the `pm.*` calls — they keep working.

## Strict mode

The shim is permissive by default — unknown `pm.*` accesses return
`undefined` and log a warning so you can find them. If you want hard
failures during a migration, set the env var
`ARGOS_PM_STRICT=1` and the shim throws on first unsupported call.
