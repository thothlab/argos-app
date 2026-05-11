---
title: CI integration
description: Drop-in GitHub Actions and GitLab CI workflows for argos run.
---

`argos run` exits non-zero on any failing request or test, so the CI step
naturally gates the pipeline. Reporters surface failures inline in the
CI provider's test panel.

## GitHub Actions

```yaml
name: argos

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  run:
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v4

      - name: Install argos
        run: |
          curl -fsSL \
            "https://github.com/thothlab/argos-app/releases/latest/download/argos-x86_64-linux.tar.gz" \
            | tar -xz
          chmod +x ./argos

      - name: Run collection
        id: argos
        run: |
          ./argos run ./argos-workspace/collections \
            --env ci \
            --reporter junit=argos-report.xml \
            --reporter json=argos-report.json
        continue-on-error: true

      - name: Publish JUnit report
        if: always()
        uses: dorny/test-reporter@v1
        with:
          name: argos
          path: argos-report.xml
          reporter: java-junit
          fail-on-error: true

      - name: Upload raw reports
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: argos-reports
          path: |
            argos-report.xml
            argos-report.json

      - name: Fail the job if any request / test failed
        if: steps.argos.outcome == 'failure'
        run: exit 1
```

The `dorny/test-reporter` step turns each `<failure>` in the JUnit XML
into an inline annotation on the Checks tab — failing requests link to
the diff line that introduced them.

## GitLab CI

```yaml
stages: [test]

argos:
  stage: test
  image: ubuntu:24.04
  before_script:
    - apt-get update -qq && apt-get install -yqq --no-install-recommends curl ca-certificates
    - curl -fsSL "https://github.com/thothlab/argos-app/releases/latest/download/argos-x86_64-linux.tar.gz" | tar -xz
    - chmod +x ./argos
  script:
    - ./argos run ./argos-workspace/collections
        --env ci
        --reporter junit=argos-report.xml
        --reporter json=argos-report.json
        --reporter html=argos-report.html
  artifacts:
    when: always
    paths:
      - argos-report.html
      - argos-report.json
    reports:
      junit: argos-report.xml
```

GitLab's built-in `artifacts:reports:junit` makes a failed test the
source of truth for pass/fail — no `continue-on-error` gymnastics needed.

## Secrets

Environment files committed to the repo should never contain secrets.
Use `--env <NAME>` to select a CI environment and pass real values
through the CI provider's secret variables:

```yaml
env:
  ARGOS_TOKEN: ${{ secrets.ARGOS_TOKEN }}
```

Reference them via standard `{{NAME}}` placeholders in headers or URLs —
Argos's resolver folds CI-injected variables into the env map for the
run.

## Other tips

- **Timeouts:** the binary has no global wall clock; set one on the CI
  job (`timeout-minutes: 10` on GitHub, `timeout: 10 minutes` on GitLab)
  so a hung request doesn't burn worker minutes.
- **`--bail`:** add when you want the first failure to stop the run.
  Useful for smoke checks where downstream requests can't recover from a
  broken auth step.
- **`--iteration-data <file>`:** data-driven runs are JUnit-aware —
  each iteration becomes a nested `<testsuite>`, surfaced in CI panels
  as a separate section.

Ready-to-copy workflow files also live in [`examples/ci/`](https://github.com/thothlab/argos-app/tree/main/examples/ci) in the repo.
