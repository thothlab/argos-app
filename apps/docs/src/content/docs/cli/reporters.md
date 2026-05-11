---
title: Reporters
description: Structured output from argos run — JSON, JUnit, HTML.
---

Console output (a Mocha-like per-request log) is always on. **Reporters**
are structured outputs for tooling: `--reporter <FORMAT>[=<PATH>]`,
repeatable.

```sh
argos run ./collections \
  --reporter junit=argos-report.xml \
  --reporter json=argos-report.json \
  --reporter html=argos-report.html
```

If `<PATH>` is omitted (`--reporter json`), the payload is appended to
stdout *after* the console summary — handy for piping through `jq`.

## JSON

Stable schema identified by `"schema": "argos.run.v1"`. Iterations and
their per-request outcomes are nested, not flattened, so a data-driven
run remains parseable:

```json
{
  "schema": "argos.run.v1",
  "workspace": "my-api",
  "summary": {
    "iterations": 3,
    "requests_total": 12,
    "requests_failed": 1,
    "tests_total": 9,
    "tests_failed": 1,
    "duration_ms": 184
  },
  "iterations": [
    {
      "index": 0,
      "requests": [
        {
          "name": "List users",
          "method": "GET",
          "url": "https://api/users",
          "status": 200,
          "duration_ms": 23,
          "ok": true,
          "error": null,
          "tests": [
            { "name": "status 200", "passed": true, "message": "" }
          ]
        }
      ]
    }
  ]
}
```

Field stability: never remove or rename fields without bumping the
`schema` discriminator. Adding fields is fine — consumers should ignore
unknown keys.

## JUnit XML

Targets the [testmoapp / Surefire dialect][surefire] that GitHub Actions
and GitLab CI consume out of the box. Each iteration becomes a
`<testsuite>`; requests with no scripts attached still emit a synthetic
`<testcase name="request">` so the run is visible in the CI test panel.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="argos run" tests="12" failures="1" errors="0" time="0.184">
  <testsuite name="iteration 1" tests="4" failures="1" errors="0" time="0.062">
    <testcase classname="GET https://api/users" name="status 200" time="0.023" />
    <testcase classname="POST https://api/users" name="201 on create" time="0.018">
      <failure message="expected 201, got 500">expected 201, got 500</failure>
    </testcase>
  </testsuite>
</testsuites>
```

Transport failures (DNS, TLS, timeout) become `<error>` children instead
of `<failure>` — that's the convention CI test panels use to distinguish
"test failed" from "test didn't run".

[surefire]: https://github.com/testmoapp/junitxml

## HTML

Single self-contained file, inline CSS, **no JavaScript**. Opens
offline, no CSP problems, scrolls cleanly on phones. Failing tests
expand by default via `<details>` so the failure message is one click
away.

Use as a CI artifact — upload it, link to it from a PR comment.

## Picking a reporter

- **JUnit** for CI test panels (GitHub Checks, GitLab pipeline tests).
- **JSON** for further processing (`jq`, custom dashboards, slack
  webhooks).
- **HTML** for human eyeballs — failed-build forensics, sharing a run
  with a teammate.
- **Console** is always on; nothing to enable.

All three iterate-aware: when `--iteration-data` is in play, each
iteration is a separate suite/section in the output.
