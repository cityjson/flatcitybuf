import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// A plain app build. index.html is the entry point; base './' lets the built
// dist/index.html open from any path. The only runtime dependency worth noting
// is @cityjson/flatcitybuf, wired via file:../../src/ts so npm install picks up
// its dist/ build.
export default defineConfig({
  base: './',
  plugins: [react()],
})
