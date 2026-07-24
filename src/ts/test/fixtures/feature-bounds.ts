/** The brute-force oracle for every spatial assertion.
 *
 *  Recomputes a feature's 2D extent from its OWN vertices, without touching
 *  the R-tree. Comparing an R-tree answer against another R-tree answer (a
 *  bbox query against a point query, say) lets both be identically wrong;
 *  comparing it against this cannot.
 *
 *  Transcribed from the writer, so it is the same quantity the index stores:
 *   * the raw bbox is min/max over the feature's QUANTISED integer vertices,
 *     with `unwrap_or(0)` when the feature has none
 *     (writer/serializer.rs:451-476);
 *   * it is then mapped to world coordinates with the header transform's x/y
 *     scale and translate -- and only x/y; z never enters a NodeItem
 *     (writer/mod.rs:133-144).
 *  An absent `transform` is CityJSON's identity-ish default (scale [1,1,1],
 *  translate [0,0,0]), not zeros -- see cityjson/index.ts. */
import type { Feature } from '../../src/feature/index.js'
import type { HeaderView } from '../../src/header/index.js'

export interface Bounds {
  minX: number
  minY: number
  maxX: number
  maxY: number
}

export function featureBounds(feature: Feature, header: HeaderView): Bounds {
  const flat = feature.vertices()
  const n = flat.length / 3

  let minX = 0
  let minY = 0
  let maxX = 0
  let maxY = 0
  if (n > 0) {
    minX = Number.POSITIVE_INFINITY
    minY = Number.POSITIVE_INFINITY
    maxX = Number.NEGATIVE_INFINITY
    maxY = Number.NEGATIVE_INFINITY
    for (let i = 0; i < flat.length; i += 3) {
      const x = flat[i]!
      const y = flat[i + 1]!
      if (x < minX) minX = x
      if (y < minY) minY = y
      if (x > maxX) maxX = x
      if (y > maxY) maxY = y
    }
  }

  const info = header.info
  const scale = info.hasTransform ? info.scale! : ([1, 1, 1] as const)
  const translate = info.hasTransform ? info.translate! : ([0, 0, 0] as const)

  return {
    minX: minX * scale[0] + translate[0],
    minY: minY * scale[1] + translate[1],
    maxX: maxX * scale[0] + translate[0],
    maxY: maxY * scale[1] + translate[1],
  }
}
