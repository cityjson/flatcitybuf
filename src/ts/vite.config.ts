import { configDefaults, defineConfig } from 'vitest/config'

export default defineConfig({
  build: {
    lib: {
      entry: { index: 'src/index.ts', 'io/node': 'src/io/node.ts' },
      formats: ['es'],
    },
    rollupOptions: { external: ['flatbuffers', /^node:/] },
    target: 'es2022',
  },
  // The default run is Node-only. `test/browser/**` would otherwise match
  // `test/**/*.test.ts` and be run under Node -- where its browser globals and
  // `inject`ed range server do not exist -- so it is excluded here. The browser
  // tests run only via their own config: `npx vitest run --config
  // vitest.browser.config.ts` (aka `npm run test:browser`).
  test: {
    include: ['test/**/*.test.ts'],
    exclude: [...configDefaults.exclude, 'test/browser/**'],
  },
})
