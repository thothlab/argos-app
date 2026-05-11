---
title: bru.* API reference
description: Exhaustive list of bru namespace helpers available in pre-request and tests scripts.
---

Both pre-request and tests scripts run in the same sandbox; the only
difference is what's available on `bru.res` (response is unset during
pre-request). Everything below is sync — `bru.test` callbacks may be
async if you really need it, but Argos doesn't wait beyond the script's
top-level evaluation.

## Logging

```js
bru.log(...args)        // captures a log line for the UI's script console
bru.info(...args)       // severity = info
bru.warn(...args)       // severity = warn
```

Multiple arguments are joined with spaces; non-string args are
`JSON.stringify`-ed.

## Failing the script

```js
bru.fail(message)
```

Throws a labelled error. **Inside a `bru.test(...)` callback** it counts
as a failure for that test only. **At the top level** of a pre-request
or tests script it aborts the script run; the request still ran (it
already did, in the tests case) but the run record carries the error.

## Environment

```js
bru.env.get(name)       // → string | undefined
bru.env.set(name, val)  // staged write — survives until end of run
bru.env.unset(name)     // staged clear — same lifetime as set
bru.env.has(name)       // → boolean
```

Writes are *staged* — the active environment file on disk isn't touched.
The host UI surfaces a banner ("3 env values were set during this run —
persist?") that lets the user commit the changes.

## The request snapshot

`bru.req` is read/write during pre-request, read-only during tests.

```js
bru.req.method                       // 'GET' | 'POST' | …
bru.req.url                          // string with {{vars}} unresolved
bru.req.setUrl(newUrl)
bru.req.setMethod('POST')
bru.req.getHeader(name)              // → string | undefined
bru.req.setHeader(name, value)
bru.req.removeHeader(name)
bru.req.body                         // { type: 'json'|'text'|'form', ... }
bru.req.setJsonBody(obj)
bru.req.setTextBody(content, ct?)
bru.req.setFormBody({ k: v, ... })
```

Setting a body changes the type automatically — `setJsonBody` switches
to `type: 'json'` even if the existing body was text.

## The response (tests only)

```js
bru.res.status                       // number
bru.res.headers                      // { [name]: value } — lower-cased keys
bru.res.getHeader(name)              // case-insensitive
bru.res.body                         // raw string
bru.res.json()                       // parsed body; throws on invalid JSON
bru.res.text                         // alias for body
```

## Assertions

`bru.expect(value)` returns a tiny matcher:

```js
bru.expect(value).toBe(expected)        // strict ===
bru.expect(value).toEqual(expected)     // deep equality (JSON-ish)
bru.expect(value).toBeTruthy()
bru.expect(value).toBeFalsy()
bru.expect(value).toContain(sub)        // string includes / array includes
bru.expect(value).toMatch(regex)        // regex test
```

Failure throws an `Error` whose `.message` is the diff between actual
and expected. Use inside `bru.test(...)` for proper failure attribution:

```js
bru.test('user id is an integer', () => {
  const id = bru.res.json().id;
  bru.expect(typeof id).toBe('number');
  bru.expect(Number.isInteger(id)).toBeTruthy();
});
```

## Test registration

```js
bru.test(name, fn)
```

`name` is what shows up in the response pane and in CLI reporters. `fn`
runs immediately — Argos doesn't queue or defer tests across requests.
Multiple tests per script are fine; each runs in its own try/catch so
one failure doesn't shadow later tests.

## What's deliberately missing

- **Network I/O.** No `fetch`, no `XMLHttpRequest`, no
  `bru.sendRequest`. Chain through env values + the runner.
- **File I/O.** No `readFile` / `writeFile`. Pass data via
  `--iteration-data` instead.
- **`require` / `import`.** The sandbox is single-script. Snippets live
  in the UI snippet library — copy / paste rather than import.
- **Long-running timers.** Sandbox aborts after a hard timeout (~2s).

If you reach for one of these and hit a wall, the [chained-request
runner (E14)](https://github.com/argos-app/argos/blob/main/docs/09_implementation_plan.md)
is the planned escape hatch.
