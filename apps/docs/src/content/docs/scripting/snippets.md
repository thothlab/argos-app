---
title: Snippets
description: Common scripting patterns ready to paste.
---

The desktop app ships a snippet library next to the script editor — the
list below is a stable subset, useful as a copy-paste reference outside
the app.

## Status check

```js
bru.test('status 200', () => {
  bru.expect(bru.res.status).toBe(200);
});

bru.test('status is 2xx', () => {
  bru.expect(bru.res.status).toBeTruthy();
  bru.expect(bru.res.status >= 200 && bru.res.status < 300).toBeTruthy();
});
```

## JSON shape

```js
bru.test('response has id', () => {
  const body = bru.res.json();
  bru.expect(body.id).toBeTruthy();
});

bru.test('list response is an array', () => {
  bru.expect(Array.isArray(bru.res.json())).toBeTruthy();
});
```

## Pluck a token, stash for next request

```js
// Tests of the /login request.
bru.test('login returns a token', () => {
  const { token } = bru.res.json();
  bru.expect(typeof token).toBe('string');
  bru.env.set('token', token);
});
```

Subsequent requests using `{{token}}` (or `Authorization: Bearer
{{token}}`) pick it up.

## Idempotent request id

```js
// Pre-request.
const id = crypto.randomUUID();
bru.req.setHeader('X-Request-Id', id);
bru.env.set('lastRequestId', id);
```

`crypto.randomUUID()` is exposed in the sandbox.

## Stamp a timestamp / nonce

```js
// Pre-request.
bru.req.setHeader('X-Timestamp', Date.now().toString());
bru.req.setHeader('X-Nonce', Math.random().toString(36).slice(2));
```

## Conditional body

```js
// Pre-request — toggle payload shape based on an env var.
const mode = bru.env.get('mode') ?? 'prod';
if (mode === 'shadow') {
  bru.req.setJsonBody({ ...bru.req.body.value, dry_run: true });
}
```

## Postman parity (pm.* shim)

```js
// Pasted directly from a Postman collection.
pm.test('Content-Type is JSON', () => {
  pm.expect(pm.response.headers.get('Content-Type')).to.include('application/json');
});
```

## Logging during development

```js
bru.log('status:', bru.res.status);
bru.log('body:', bru.res.json());
```

Output shows up in the *Script console* sub-tab in the response pane.
Logging from a CLI run prints to stdout next to the request's console
summary line.

## Anti-patterns

- **Don't loop a `fetch` or `pm.sendRequest`.** The sandbox blocks I/O;
  the call no-ops. Chain through env values + multiple requests
  instead.
- **Don't time out long scripts manually.** The sandbox enforces ~2s.
  A `while (true)` halts the run and reports a sandbox-error.
- **Don't rely on cross-request state through `globalThis`.** The
  sandbox runs each script in its own context; only `bru.env` (and the
  `pm.*` shim's in-memory maps) survives across requests in a run.
