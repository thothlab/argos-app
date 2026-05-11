// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  // Update when the docs are deployed under a sub-path (e.g.
  // /docs on argos.app). Cloudflare Pages / Vercel apex serves at
  // root, in which case `site` alone is enough.
  site: 'https://argos.app/docs',
  integrations: [
    starlight({
      title: 'Argos',
      description:
        'Fast, git-native API client. REST / GraphQL / WebSocket, scripting, CLI runner, OpenAPI / Postman / Insomnia / Bruno import.',
      logo: { src: './public/logo-mark.svg', replacesTitle: false },
      social: {
        github: 'https://github.com/argos-app/argos',
      },
      editLink: {
        baseUrl: 'https://github.com/argos-app/argos/edit/main/apps/docs/',
      },
      lastUpdated: true,
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 3 },
      customCss: ['./src/styles/argos.css'],
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
          ],
        },
      ],
    }),
  ],
});
