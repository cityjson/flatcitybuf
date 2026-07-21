import { defineConfig } from 'vite'

export default defineConfig({
  build: {
    lib: {
      entry: { index: 'src/index.ts', 'io/node': 'src/io/node.ts' },
      formats: ['es'],
    },
    rollupOptions: { external: ['flatbuffers', /^node:/] },
    target: 'es2022',
  },
  test: { include: ['test/**/*.test.ts'] },
})
