/** Browser-mode test for the HTTP range path (`FcbReader.fromUrl`).
 *
 *  This is the test that only a real browser can carry: it runs in Chromium
 *  (Vitest browser mode, Playwright provider) and issues genuine CROSS-ORIGIN
 *  `fetch` range requests to `range_server.py`. The browser page is served by
 *  Vitest on one localhost port; the range server binds a different one, so
 *  every request is cross-origin and the browser enforces CORS -- which Node's
 *  `fetch` never does.
 *
 *  Two things are pinned:
 *    1. A full range-read scan produces the same CityJSON line stream the NODE
 *       reader produced for the same bytes (provided via `inject`).
 *    2. The CORS failure path: against `?no_cors_expose=1` the server sends the
 *       206 with Content-Range on the wire but WITHOUT
 *       Access-Control-Expose-Headers, so the browser hides Content-Range from
 *       JS and the reader must raise `RangeHeadersNotExposed` rather than guess
 *       a size. A Node `fetch` cannot reproduce this -- it would read the
 *       header fine -- which is the whole reason browser mode exists here. */
import { inject, describe, expect, it } from 'vitest'
import { ErrorCode, FcbError } from '../../src/errors.js'
import { FcbReader } from '../../src/reader.js'

describe('FcbReader.fromUrl (browser, cross-origin)', () => {
  it('range-reads a remote file to the same CityJSON as Node', async () => {
    const base = inject('rangeServerBase')
    const name = inject('corpusName')
    const nodeCityJson = inject('nodeCityJson')

    const reader = await FcbReader.fromUrl(`${base}/${name}`)
    const lines: unknown[] = []
    for await (const line of reader.cityjson()) {
      lines.push(JSON.parse(JSON.stringify(line)) as unknown)
    }

    expect(lines.length).toBeGreaterThan(1) // metadata + at least one feature
    expect(lines).toEqual(nodeCityJson)
  })

  it('raises RangeHeadersNotExposed when CORS hides Content-Range', async () => {
    const base = inject('rangeServerBase')
    const name = inject('corpusName')

    // The server still SENDS Content-Range on the wire here; it just omits
    // Access-Control-Expose-Headers, so a cross-origin browser cannot read it.
    // The reader must fail loudly rather than silently guess a size.
    let caught: unknown
    try {
      await FcbReader.fromUrl(`${base}/${name}?no_cors_expose=1`)
    } catch (err) {
      caught = err
    }
    expect(caught).toBeInstanceOf(FcbError)
    expect((caught as FcbError).code).toBe(ErrorCode.RangeHeadersNotExposed)
  })
})
