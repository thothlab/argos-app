---
title: Scripting overview
description: Pre-request hooks, tests, and the bru.* / pm.* APIs.
---

Each request has two optional JavaScript hooks:

- **Pre-request** — runs before the request is sent. Can mutate the
  request body, headers, URL, and set environment variables for later
  requests in the run.
- **Tests** — runs after the response arrives. Registers test cases
  whose pass / fail status surfaces in the response pane and in CLI
  reporters.

Both run in a sandboxed QuickJS interpreter — no network, no file system,
no `eval`. The `bru.*` and `pm.*` namespaces are the only way out.

## A minimal test

```js
bru.test('status 200', () => {
  bru.expect(bru.res.status).toBe(200);
});
```

`bru.test` registers a case under the supplied name. Anything that throws
inside the callback fails it; the failure message is the thrown error's
`.message`.

## Mutating the request before send

```js
// Pre-request — stamp a request id and refresh the token.
bru.req.setHeader('X-Request-Id', crypto.randomUUID());
const token = await refreshToken(); // ← not available; see below
bru.env.set('token', token);
```

`bru.req.setHeader` / `setJsonBody` / `setUrl` mutate the request that
will go on the wire. `bru.env.set` updates the environment for the rest
of the run (per-run, not persisted to disk).

**Network access from scripts is intentionally absent.** If you need to
refresh a token, generate it outside Argos and pass it through the CI
env, or wait for the planned chained-request runner.

## Postman compatibility

A `pm.*` shim is installed automatically so existing Postman snippets
paste in unchanged:

```js
pm.test('user id is a number', () => {
  pm.expect(pm.response.json().id).to.be.a('number');
});
```

See [pm.* compatibility](/docs/scripting/pm/) for the full mapping. Anything
not listed is unimplemented — Argos warns to the console rather than
silently no-oping.

## Where scripts live on disk

Inside the request file (YAML), under the `scripts:` key:

```yaml
kind: request
name: List users
type: rest
method: GET
url: "{{baseUrl}}/users"
scripts:
  pre_request: |
    bru.req.setHeader('X-Request-Id', crypto.randomUUID());
  tests: |
    bru.test('status 200', () => {
      bru.expect(bru.res.status).toBe(200);
    });
```

Empty hooks are omitted from the YAML — `pre_request: null` doesn't
appear at all on save.

## Where to next

- [bru.* API reference](/docs/scripting/bru/) — exhaustive list.
- [pm.* compatibility](/docs/scripting/pm/) — Postman mapping.
- [Snippets](/docs/scripting/snippets/) — common patterns ready to paste.
