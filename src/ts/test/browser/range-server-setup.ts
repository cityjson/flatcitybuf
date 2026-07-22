/** Vitest `globalSetup` for the browser project. It runs in NODE (not the
 *  browser), which is the whole point: a browser test file cannot spawn a
 *  child process or touch `node:fs`, so everything that needs the Node
 *  runtime happens here and is handed to the browser via `provide()` /
 *  `inject()`.
 *
 *  Two things are set up:
 *    1. `range_server.py` is started (the same server the Node `http.test.ts`
 *       uses) so the browser's real cross-origin `fetch` has something to
 *       range-read against, WITH the CORS headers Task 11 added. The browser
 *       page is served by Vitest on one localhost port; this server binds a
 *       different one, so every request from the test is genuinely
 *       cross-origin -- which is exactly what makes the `no_cors_expose`
 *       failure path reachable (a Node `fetch` does not enforce CORS at all).
 *    2. The NODE reader's own result for `small.fcb` (feature ids + the full
 *       CityJSON line stream) is computed here and provided to the browser,
 *       so the browser tests compare their output against the Node reader's
 *       output for the same bytes -- not against themselves. */
import { spawn, type ChildProcess } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { FcbReader } from '../../src/reader.js'

// Provided-context typing so `inject()` in the browser tests is type-safe.
declare module 'vitest' {
  interface ProvidedContext {
    /** e.g. `http://127.0.0.1:54321` -- the running range_server.py. */
    rangeServerBase: string
    /** Feature ids the NODE reader produced for `small.fcb`. */
    nodeIds: string[]
    /** The full CityJSON line stream (metadata + features) the NODE reader
     *  produced for `small.fcb`, as plain JSON-serialisable objects. */
    nodeCityJson: unknown[]
    /** Basename of the corpus file both browser tests scan. */
    corpusName: string
  }
}

const HERE = import.meta.dirname
// From src/ts/test/browser/: four levels up is the repo root.
const CORPUS = resolve(HERE, '../../../../conformance')
// From src/ts/test/browser/: three levels up is src/, then cpp/tests/.
const SERVER = resolve(HERE, '../../../cpp/tests/range_server.py')
const CORPUS_NAME = 'small.fcb'

interface Provide {
  provide: (key: string, value: unknown) => void
}

export default async function setup({ provide }: Provide): Promise<() => Promise<void>> {
  // --- 1. Start range_server.py, failing fast if it never prints a port. ---
  // Mirrors http.test.ts's beforeAll: a bare spawn + on('data') hangs forever
  // if python3 is missing or the server dies before printing a port.
  const proc: ChildProcess = spawn('python3', [SERVER, CORPUS])
  const base = await new Promise<string>((ok, reject) => {
    const timer = setTimeout(
      () => reject(new Error('range_server.py did not print a port within 10s')), 10_000)
    const settle = (fn: () => void) => { clearTimeout(timer); fn() }
    proc.once('error', (err) => settle(() => reject(err)))
    proc.once('exit', (code) =>
      settle(() => reject(new Error(`range_server.py exited early (code ${code})`))))
    proc.stdout?.on('data', (d: Buffer) => {
      const m = /(\d+)/.exec(d.toString())
      if (m) settle(() => ok(`http://127.0.0.1:${m[1]}`))
    })
  })

  // --- 2. Compute the NODE reader's result for the same file. ---
  const bytes = new Uint8Array(readFileSync(resolve(CORPUS, CORPUS_NAME)))
  const nodeReader = await FcbReader.fromBytes(bytes)
  const nodeIds: string[] = []
  for await (const f of await nodeReader.selectAll()) nodeIds.push(f.id)
  const nodeCityJson: unknown[] = []
  for await (const line of nodeReader.cityjson()) {
    // Round-trip through JSON so what crosses the Node->browser boundary is
    // exactly what the browser will later `JSON.parse`/`toEqual` against, and
    // is guaranteed structured-cloneable (no BigInt leaks under any policy).
    nodeCityJson.push(JSON.parse(JSON.stringify(line)) as unknown)
  }

  provide('rangeServerBase', base)
  provide('nodeIds', nodeIds)
  provide('nodeCityJson', nodeCityJson)
  provide('corpusName', CORPUS_NAME)

  // --- Teardown: SIGKILL the server and AWAIT its exit. ---
  // A fire-and-forget kill lets the run proceed while the child and its
  // keep-alive sockets stay open, which pins the process alive until vitest's
  // ~10-minute forced-exit fallback. Killing and awaiting closes the sockets
  // promptly. This is the exact hazard http.test.ts documents; do not weaken.
  return async () => {
    if (proc.exitCode !== null || proc.signalCode !== null) return
    await new Promise<void>((done) => {
      proc.once('exit', () => done())
      proc.kill('SIGKILL')
    })
  }
}
