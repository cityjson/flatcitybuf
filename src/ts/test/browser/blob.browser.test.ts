/** Browser-mode test for the Blob path (`FcbReader.fromBlob`).
 *
 *  Runs in a REAL Chromium via Vitest browser mode (Playwright provider), so
 *  it exercises the actual shipping target rather than Node's Blob shim. The
 *  bytes are fetched from the range server (started by the browser project's
 *  globalSetup, `range-server-setup.ts`) and wrapped in a `File` -- the same
 *  object a `DataTransfer` drop or an `<input type=file>` pick hands to a web
 *  app -- then scanned end to end. The resulting feature ids are compared
 *  against the NODE reader's ids for the same file (provided via `inject`),
 *  so this asserts against the Node result, not against itself. */
import { inject, describe, expect, it } from 'vitest'
import { FcbReader } from '../../src/reader.js'

describe('FcbReader.fromBlob (browser)', () => {
  it('scans a File built from dropped bytes to the same feature ids as Node', async () => {
    const base = inject('rangeServerBase')
    const name = inject('corpusName')
    const nodeIds = inject('nodeIds')

    // A plain (no-Range) GET returns 200 with the whole body; the server sends
    // Access-Control-Allow-Origin: *, so this cross-origin read is allowed.
    const res = await fetch(`${base}/${name}`)
    expect(res.status).toBe(200)
    const buf = await res.arrayBuffer()

    // Exactly what a drag-and-drop / file-picker hands an app: a File (which
    // IS a Blob) carrying the dropped bytes.
    const file = new File([buf], name, { type: 'application/octet-stream' })
    expect(file.size).toBe(buf.byteLength)

    const reader = await FcbReader.fromBlob(file)
    const ids: string[] = []
    for await (const f of await reader.selectAll()) ids.push(f.id)

    expect(ids.length).toBeGreaterThan(0)
    expect(ids).toEqual(nodeIds)
  })
})
