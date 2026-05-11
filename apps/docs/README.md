# @argos/docs

Astro + [Starlight](https://starlight.astro.build/) documentation site
for Argos.

## Develop

```sh
pnpm --filter @argos/docs dev          # http://localhost:4321
pnpm --filter @argos/docs build        # static export to dist/
pnpm --filter @argos/docs preview      # serve the built dist/
```

## Content layout

- `src/content/docs/` — markdown / MDX pages (URL = path).
- `astro.config.mjs` — Starlight config: sidebar, social links, theme.
- `src/styles/argos.css` — brand-accent overrides on Starlight defaults.

Add a page by dropping a `.md` / `.mdx` file under `src/content/docs/`
and adding it to the `sidebar` array in `astro.config.mjs`. Pagefind
indexes the built HTML automatically — no extra config.

## Deploy

Static hosting — any of the usual suspects works:

- **Cloudflare Pages:** point at this folder, build command
  `pnpm --filter @argos/docs build`, output dir `apps/docs/dist`.
- **Vercel:** same flags via `vercel.json` at repo root, or set in the
  project settings.
- **GitHub Pages:** `actions/upload-pages-artifact` on `apps/docs/dist`
  + the pages action.

The `site:` URL in `astro.config.mjs` is `https://argos.app/docs` —
adjust if the canonical lives elsewhere before the public 1.0 launch.

## Source of truth

Several pages mirror content that lives in this repo's source — the
CLI reference reflects `crates/cli/src/main.rs`, the scripting reference
reflects `crates/scripting/src/lib.rs`, the file format reflects
`crates/core/src/format/`. When those change, update the doc page in the
same commit so the site doesn't drift.
