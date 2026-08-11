import { describe, expect, it } from 'vitest'
import {
  assembleCityJSONSeq, deriveFilename, FORMATS, stringifyCityJSON,
} from './index'

describe('FORMATS registry', () => {
  it('covers all three formats with distinct extensions', () => {
    expect(FORMATS.cityjson.ext).toBe('.city.json')
    expect(FORMATS.cityjsonseq.ext).toBe('.city.jsonl')
    expect(FORMATS.obj.ext).toBe('.obj')
    expect(FORMATS.cityjson.mime).toBe('application/json')
    expect(FORMATS.cityjsonseq.mime).toBe('application/x-ndjson')
    expect(FORMATS.obj.mime).toBe('text/plain')
  })

  it('has the exact display labels', () => {
    expect(FORMATS.cityjson.label).toBe('CityJSON')
    expect(FORMATS.cityjsonseq.label).toBe('CityJSONSeq')
    expect(FORMATS.obj.label).toBe('OBJ')
  })
})

describe('assembleCityJSONSeq', () => {
  it('emits the metadata line first, then one line per feature', () => {
    const meta = { type: 'CityJSON', version: '2.0' }
    const feats = [{ type: 'CityJSONFeature', id: 'a' }, { type: 'CityJSONFeature', id: 'b' }]
    const out = assembleCityJSONSeq(meta, feats)
    expect(out).toBe(
      [JSON.stringify(meta), JSON.stringify(feats[0]), JSON.stringify(feats[1])].join('\n'),
    )
  })

  it('produces a single metadata line when there are no features', () => {
    const meta = { type: 'CityJSON' }
    const out = assembleCityJSONSeq(meta, [])
    expect(out).toBe(JSON.stringify(meta))
  })
})

describe('deriveFilename', () => {
  it('strips .fcb and the URL path, then appends the format extension', () => {
    const url = 'https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb'
    expect(deriveFilename(url, 'cityjson')).toBe('3dbag_all_index.city.json')
    expect(deriveFilename(url, 'cityjsonseq')).toBe('3dbag_all_index.city.jsonl')
    expect(deriveFilename(url, 'obj')).toBe('3dbag_all_index.obj')
  })

  it('handles a bare local filename', () => {
    expect(deriveFilename('delft.fcb', 'obj')).toBe('delft.obj')
  })

  it('drops query strings and falls back when there is no source', () => {
    expect(deriveFilename('http://x/y/a.fcb?token=1', 'cityjson')).toBe('a.city.json')
    expect(deriveFilename(undefined, 'obj')).toBe('flatcitybuf-export.obj')
    expect(deriveFilename('', 'obj')).toBe('flatcitybuf-export.obj')
  })

  it('drops URL fragments', () => {
    expect(deriveFilename('http://x/y/b.fcb#section', 'obj')).toBe('b.obj')
  })
})

describe('stringifyCityJSON', () => {
  it('converts nested Maps (as fcb_wasm returns) into plain-object CityJSON', () => {
    const merged = new Map<string, unknown>([
      ['type', 'CityJSON'],
      ['version', '2.0'],
      ['CityObjects', new Map([['a', new Map([['type', 'Building']])]])],
      ['vertices', [[0, 0, 0], [1, 1, 1]]],
    ])
    const parsed = JSON.parse(stringifyCityJSON(merged))
    expect(parsed.type).toBe('CityJSON')
    expect(parsed.version).toBe('2.0')
    expect(parsed.CityObjects.a.type).toBe('Building')
    expect(parsed.vertices).toEqual([[0, 0, 0], [1, 1, 1]])
  })

  it('passes plain objects through unchanged', () => {
    expect(stringifyCityJSON({ type: 'CityJSON', CityObjects: {} }))
      .toBe('{"type":"CityJSON","CityObjects":{}}')
  })
})
