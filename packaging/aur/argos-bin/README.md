# `argos-bin` — AUR package

Source of truth for the [AUR `argos-bin`](https://aur.archlinux.org/packages/argos-bin)
package. CI renders `PKGBUILD.template` into a real `PKGBUILD` and
pushes it to `ssh://aur@aur.archlinux.org/argos-bin.git` on every
`v*` release.

## Layout

- `PKGBUILD.template` — `@PKGVER@` + `@SHA256@` substituted at release
  time. The rendered file is what AUR users see.
- `.SRCINFO` — generated from the rendered PKGBUILD by the deploy
  action. Not stored here.

## What the package installs

- `/opt/argos/argos.AppImage` — the AppImage as-is. Runs via FUSE,
  using its own bundled GTK / WebKit copies, so the build is
  independent of the system library minor versions.
- `/usr/bin/argos` — `sh` wrapper that `exec`s the AppImage. CLI users
  type `argos run …`, the desktop launcher uses the same path.
- `/usr/share/applications/argos.desktop` — desktop entry pulled from
  the bundle, with `Exec=` repointed at `/usr/bin/argos`.
- `/usr/share/icons/hicolor/128x128/apps/argos-desktop.png` — icon
  referenced by the desktop file.

## Local test

```sh
cd packaging/aur/argos-bin
sed -e "s/@PKGVER@/0.1.3/g" \
    -e "s/@SHA256@/$(curl -sL https://github.com/thothlab/argos-app/releases/download/v0.1.3/Argos_0.1.3_amd64.AppImage | sha256sum | cut -d' ' -f1)/" \
    PKGBUILD.template > PKGBUILD
makepkg -si
```

## Upgrading by hand (emergency)

If CI is broken and you need to ship an AUR update manually:

```sh
git clone ssh://aur@aur.archlinux.org/argos-bin.git
cd argos-bin
# Render PKGBUILD with the new version + sha256, then:
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "0.1.4"
git push
```
