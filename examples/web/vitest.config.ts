import { defineConfig } from 'vitest/config'

// Pure-module tests only (crs, geometry, reader). They run in Node: reader.test
// opens examples/data/delft.fcb from disk via FcbReader.fromBytes. No DOM.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
})
