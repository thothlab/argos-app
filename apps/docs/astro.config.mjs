// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  // The deployed home is argos.thothlab.tech with docs mounted under
  // /docs. `base` prefixes every generated asset / link so CSS, JS,
  // sitemap and internal navigation resolve correctly once served from
  // the sub-path. `site` is used for canonical / sitemap absolute URLs.
  site: 'https://argos.thothlab.tech',
  base: '/docs',
  integrations: [
    starlight({
      title: 'Argos',
      description:
        'Fast, git-native API client. REST / GraphQL / WebSocket, scripting, CLI runner, OpenAPI / Postman / Insomnia / Bruno import.',
      logo: { src: './public/logo-mark.svg', replacesTitle: false },
      social: {
        github: 'https://github.com/thothlab/argos-app',
      },
      editLink: {
        baseUrl: 'https://github.com/thothlab/argos-app/edit/main/apps/docs/',
      },
      lastUpdated: true,
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 3 },
      customCss: ['./src/styles/argos.css'],
      components: {
        ThemeSelect: './src/components/ThemeToggle.astro',
      },
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'What is Argos', link: '/' },
            { label: 'Getting started', link: '/getting-started/' },
            { label: 'Importing collections', link: '/importing/' },
          ],
        },
        {
          label: 'CLI',
          items: [
            { label: 'argos run', link: '/cli/run/' },
            { label: 'argos list / validate', link: '/cli/inspect/' },
            { label: 'Reporters', link: '/cli/reporters/' },
            { label: 'CI integration', link: '/cli/ci/' },
          ],
        },
        {
          label: 'Scripting',
          items: [
            { label: 'Overview', link: '/scripting/' },
            { label: 'bru.* API', link: '/scripting/bru/' },
            { label: 'pm.* compatibility', link: '/scripting/pm/' },
            { label: 'Snippets', link: '/scripting/snippets/' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'File format', link: '/reference/file-format/' },
            { label: 'Environments', link: '/reference/environments/' },
            { label: 'Protocols', link: '/reference/protocols/' },
            { label: 'Codegen targets', link: '/reference/codegen/' },
            { label: 'Desktop app', link: '/reference/app/' },
          ],
        },
      ],
    }),
  ],
});
