/** Browser test project, kept in a SEPARATE config on purpose.
 *
 *  The default `npx vitest run` resolves `vite.config.ts` (Node only, and
 *  that config excludes `test/browser/**`), so it never launches a browser --
 *  a developer with no browser installed still gets the full green Node suite.
 *  The browser tests run ONLY when this config is selected explicitly:
 *
 *      npx vitest run --config vitest.browser.config.ts
 *      # or, equivalently, the package script:
 *      npm run test:browser
 *
 *  Vitest 5 splits the browser providers into their own packages:
 *  `@vitest/browser` alone is not enough, `@vitest/browser-playwright` must be
 *  installed and its `playwright()` factory used here. Chromium must be present
 *  (`npx playwright install chromium`), and `range_server.py` is started by the
 *  globalSetup below (which also provides the Node reader's results to compare
 *  against). */
import { playwright } from '@vitest/browser-playwright'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    name: 'browser',
    include: ['test/browser/**/*.browser.test.ts'],
    globalSetup: ['./test/browser/range-server-setup.ts'],
    browser: {
      enabled: true,
      provider: playwright(),
      // Headless everywhere (locally and in CI); do not gate on process.env.CI.
      headless: true,
      instances: [{ browser: 'chromium' }],
    },
  },
})
