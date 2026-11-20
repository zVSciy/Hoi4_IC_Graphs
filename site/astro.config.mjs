import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';

// Project page: https://zvsciy.github.io/Hoi4_IC_Graphs/
export default defineConfig({
  site: 'https://zvsciy.github.io',
  base: '/Hoi4_IC_Graphs',
  // Pages link to each other relatively (../code/data/), which needs trailing slashes.
  trailingSlash: 'always',
  integrations: [mdx()],
  markdown: {
    shikiConfig: { theme: 'github-dark' },
  },
});
