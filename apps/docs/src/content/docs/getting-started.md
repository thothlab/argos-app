---
title: Getting started
description: Install Argos, open or create a workspace, and run your first request.
---

Argos ships as a desktop app and a single-binary CLI. Both read the same
workspace folder, so you can hack in the UI and re-run the same requests
in CI without exporting anything.

## Install the desktop app

Pre-built bundles are attached to every [GitHub release](https://github.com/thothlab/argos-app/releases).

- **macOS:** `Argos_<version>_aarch64.dmg` (Apple Silicon). Intel builds
  ship on demand via the `release-darwin-x86_64` workflow.
- **Windows:** `Argos_<version>_x64-setup.exe` (or the `.msi`).
- **Linux:** `.deb` / `.rpm` / `.AppImage` for x86_64. Arch users can
  install [`argos-bin`](https://aur.archlinux.org/packages/argos-bin)
  from the AUR.

The desktop app bundles the CLI binary so the two can't drift. On first launch
the welcome screen offers **Open workspace folder** or **Create new workspace**.

### Linux — verify the download

Every `.deb`, `.rpm`, and `.AppImage` ships with a detached GPG
signature next to it (`.asc`). Import the release key once and verify
before installing:

```sh
# Import the Argos release-signing key.
curl -fsSL https://argos.thothlab.tech/argos-gpg.pub | gpg --import

# Download the artifact + its signature.
V=0.1.3
curl -fsSLO "https://github.com/thothlab/argos-app/releases/download/v${V}/Argos_${V}_amd64.AppImage"
curl -fsSLO "https://github.com/thothlab/argos-app/releases/download/v${V}/Argos_${V}_amd64.AppImage.asc"

# Verify — expect "Good signature from Argos Releases <releases@thothlab.tech>".
gpg --verify "Argos_${V}_amd64.AppImage.asc" "Argos_${V}_amd64.AppImage"
```

The same `.asc` pattern works for `.deb` and `.rpm`. We use detached
GPG signatures across all three formats rather than the distro-native
mechanisms (`debsig-verify` / `rpm --import`) so one workflow covers
every Linux user.

The `.sig` files Tauri produces alongside the artifacts are minisign
signatures consumed by the in-app updater — separate chain, ignore
them for download verification.

## Install the CLI on a CI runner

The CLI ships with every desktop bundle; on Linux runners you can
extract it from the `.deb` directly:

```sh
V=0.1.3
curl -fsSLO "https://github.com/thothlab/argos-app/releases/download/v${V}/Argos_${V}_amd64.deb"
dpkg-deb -x "Argos_${V}_amd64.deb" out
./out/usr/bin/argos --help
```

See [CI integration](/docs/cli/ci/) for ready-made GitHub Actions and GitLab CI
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

- [Import an existing collection](/docs/importing/) — Postman, Insomnia, OpenAPI.
- [Environments](/docs/reference/environments/) — `{{baseUrl}}` and secret values.
- [Scripting overview](/docs/scripting/) — `bru.*` test helpers and Postman parity
  via `pm.*`.
