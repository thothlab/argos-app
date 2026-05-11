---
title: Getting started
description: Install Argos, open or create a workspace, and run your first request.
---

Argos ships as a desktop app and a single-binary CLI. Both read the same
workspace folder, so you can hack in the UI and re-run the same requests
in CI without exporting anything.

## Install the desktop app

Pre-built bundles are attached to every [GitHub release](https://github.com/argos-app/argos/releases).

- **macOS:** `Argos-<version>.dmg` (Apple Silicon and Intel).
- **Windows:** `Argos-<version>-setup.exe`.
- **Linux:** `.deb` / `.rpm` for the common distros, AppImage as a fallback.

The desktop app bundles the CLI binary so the two can't drift. On first launch
the welcome screen offers **Open workspace folder** or **Create new workspace**.

## Install the CLI on a CI runner

The CLI is a static binary — drop it on the runner before invoking it:

```sh
curl -fsSL \
  "https://github.com/argos-app/argos/releases/latest/download/argos-x86_64-linux.tar.gz" \
  | tar -xz
chmod +x ./argos
./argos --help
```

See [CI integration](/cli/ci/) for ready-made GitHub Actions and GitLab CI
workflows.

## Create your first request

In the desktop app:

1. **Open workspace folder** → pick an empty folder.
2. Click **+** in the sidebar header → **New REST request**.
3. Enter `https://httpbin.org/get` in the URL bar.
4. Press <kbd>⌘</kbd>+<kbd>Enter</kbd> (or click **Send**).

The response pane shows status, headers, body, and timing. Argos auto-saves
the request to `<workspace>/collections/<slug>.argos.yaml` — open it in your
editor of choice if you want to see the on-disk shape.

## Add a test

In the **Scripts** tab of the request editor:

```js
bru.test('status 200', () => {
  bru.expect(bru.res.status).toBe(200);
});
```

Send the request again. The **Tests** sub-tab in the response pane shows
`✓ status 200`. The same script runs in CI when you use `argos run`.

## Run a collection from the command line

From the workspace root:

```sh
argos run ./collections
```

Argos walks the tree, runs every request in order, prints a Mocha-like
summary, and exits non-zero if anything failed. Add a reporter to capture a
structured artifact for CI:

```sh
argos run ./collections \
  --reporter junit=report.xml \
  --reporter json=report.json
```

## Next steps

- [Import an existing collection](/importing/) — Postman, Insomnia, OpenAPI.
- [Environments](/reference/environments/) — `{{baseUrl}}` and secret values.
- [Scripting overview](/scripting/) — `bru.*` test helpers and Postman parity
  via `pm.*`.
