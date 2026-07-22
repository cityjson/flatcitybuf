import { defineConfig } from 'vite'

// A plain app build (not a library): index.html is the entry point, and the
// only runtime dependency is the reader package itself
// (`@cityjson/flatcitybuf`, wired via a `file:../../src/ts` dependency in
// package.json so `npm install` picks up its `dist/` build). Nothing here
// needs to differ from Vite's defaults beyond `base: './'`, so the built
// `dist/index.html` can be opened from any path, not just the server root.
export default defineConfig({
  base: './',
})
