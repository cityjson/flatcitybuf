import { describe, expect, it } from 'vitest'
import {
  assembleCityJSONSeq, deriveFilename, FORMATS,
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
})

describe('assembleCityJSONSeq', () => {
  it('emits the metadata line first, then one line per feature', () => {
    const meta = { type: 'CityJSON', version: '2.0' }
    const feats = [{ type: 'CityJSONFeature', id: 'a' }, { type: 'CityJSONFeature', id: 'b' }]
    const out = assembleCityJSONSeq(meta, feats)
    const lines = out.split('\n')
    expect(lines).toHaveLength(3)
    expect(JSON.parse(lines[0]).type).toBe('CityJSON')
    expect(JSON.parse(lines[1]).id).toBe('a')
    expect(JSON.parse(lines[2]).id).toBe('b')
  })

  it('produces a single metadata line when there are no features', () => {
    const out = assembleCityJSONSeq({ type: 'CityJSON' }, [])
    expect(out.split('\n')).toHaveLength(1)
  })
})

describe('deriveFilename', () => {
  it('strips .fcb and the URL path, then appends the format extension', () => {
    const url = 'https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb'
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
})
