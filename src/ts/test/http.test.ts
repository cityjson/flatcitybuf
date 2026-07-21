import { spawn, type ChildProcess } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { FetchRangeReader } from '../src/io/fetch.js'
import { FcbReader } from '../src/reader.js'

// `__dirname` does not exist under ESM (this package is "type": "module");
// the port-wide convention is `import.meta.dirname`, as in every other test
// file under test/ (sources.test.ts, header.test.ts, conformance.test.ts,
// features.test.ts, generated.test.ts). The task brief's test spelled this
// `__dirname` -- adapted here, nothing else about the brief's test changed.
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const SERVER = resolve(import.meta.dirname, '../../cpp/tests/range_server.py')
let proc: ChildProcess
let base: string

beforeAll(async () => {
  proc = spawn('python3', [SERVER, CORPUS])
  base = await new Promise<string>((ok) => {
    proc.stdout!.on('data', (d: Buffer) => {
      const m = /(\d+)/.exec(d.toString())
      if (m) ok(`http://127.0.0.1:${m[1]}`)
    })
  })
})
afterAll(() => { proc.kill() })

describe('FetchRangeReader', () => {
  it('learns its size from Content-Range at open', async () => {
    const r = await FetchRangeReader.open(`${base}/small.fcb`)
    expect(r.size()).toBe(readFileSync(resolve(CORPUS, 'small.fcb')).length)
  })

  it('serves the same bytes as the local reader', async () => {
    const local = new Uint8Array(readFileSync(resolve(CORPUS, 'small.fcb')))
    const r = await FetchRangeReader.open(`${base}/small.fcb`)
    expect(Array.from(await r.read(8, 16))).toEqual(Array.from(local.subarray(8, 24)))
  })

  it('THROWS when the server ignores Range and returns 200', async () => {
    // The wasm client accepts this and every later offset reads garbage.
    await expect(FetchRangeReader.open(`${base}/small.fcb?ignore_range=1`))
      .rejects.toThrow(FcbError)
  })

  it('throws on a malformed Content-Range', async () => {
    await expect(FetchRangeReader.open(`${base}/small.fcb?bad_range=1`))
      .rejects.toThrow(FcbError)
  })

  it('throws when the server returns a DIFFERENT range than requested', async () => {
    // Indistinguishable from success unless the start/end are checked.
    await expect(FetchRangeReader.open(`${base}/small.fcb?wrong_offset=1`))
      .rejects.toThrow(FcbError)
  })

  it('aborts in-flight requests when the signal fires', async () => {
    const ac = new AbortController()
    const r = await FetchRangeReader.open(`${base}/small.fcb`)
    ac.abort()
    await expect(r.read(0, 16, { signal: ac.signal })).rejects.toThrow()
  })

  // The test above only proves the ALREADY-aborted case (Task 5's
  // BufferedRangeReader-style `checkAborted` alone would pass it). The
  // task brief for this reader specifically calls out that nothing
  // upstream re-checks mid-flight, so THIS reader's `fetch()` call must
  // itself observe the signal. Proven here with a `fetch` stub that never
  // settles on its own -- it only rejects when the signal it was given
  // fires -- so a passing test can only mean the signal actually reached
  // the underlying `fetch()` call while the request was in flight.
  it('really wires the signal into fetch -- rejects on a MID-FLIGHT abort, not just an already-aborted one', async () => {
    const ac = new AbortController()
    let sawSignal: AbortSignal | undefined
    const stub: typeof fetch = (_url, init) => {
      sawSignal = init?.signal as AbortSignal | undefined
      return new Promise((_resolve, reject) => {
        sawSignal?.addEventListener('abort', () => {
          reject(new DOMException('aborted by test stub', 'AbortError'))
        })
      })
    }

    const opening = FetchRangeReader.open(`${base}/small.fcb`, { fetch: stub, signal: ac.signal })
    // The signal is not aborted when the request is issued -- only after,
    // while `opening` is still pending -- so this is a genuine mid-flight
    // abort, not the already-aborted-at-entry shortcut.
    expect(ac.signal.aborted).toBe(false)
    ac.abort()
    await expect(opening).rejects.toThrow(FcbError)
    expect(sawSignal).toBeDefined()
  })
})

describe('FcbReader.fromUrl', () => {
  it('scans a remote file to the same CityJSON as the local one', async () => {
    const remote = await FcbReader.fromUrl(`${base}/small.fcb`)
    const local = await FcbReader.fromBytes(
      new Uint8Array(readFileSync(resolve(CORPUS, 'small.fcb'))))
    const ids = async (r: FcbReader) => {
      const out: string[] = []
      for await (const f of await r.selectAll()) out.push(f.id)
      return out
    }
    expect(await ids(remote)).toEqual(await ids(local))
  })

  it('opens with ONE request, not one per section', async () => {
    // The 12944-byte prefetch buys magic + header + the top 3 rtree levels.
    let calls = 0
    const counting: typeof fetch = (...args) => { calls++; return fetch(...args) }
    await FcbReader.fromUrl(`${base}/small.fcb`, { fetch: counting })
    expect(calls).toBe(1)
  })
})
