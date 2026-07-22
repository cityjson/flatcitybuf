/** Finds a feature by its CityObject id, by scanning `selectAll()`.
 *
 *  A test helper for the nearest-neighbour oracle: given the id `searchNearest`
 *  returned, re-read that feature so its own vertices can be measured with
 *  `featureBounds` -- the brute-force centroid the R-tree answer is checked
 *  against. Deliberately index-free: it walks the feature section, so it can
 *  never share a bug with the traversal it is meant to verify. */
import type { Feature } from '../../src/feature/index.js'
import type { FcbReader } from '../../src/reader.js'

export async function featureById(reader: FcbReader, id: string): Promise<Feature> {
  for await (const f of await reader.selectAll()) {
    if (f.id === id) return f
  }
  throw new Error(`no feature with id ${id}`)
}
