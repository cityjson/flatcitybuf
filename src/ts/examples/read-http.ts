/** Reading over HTTP range requests -- the point of the format.
 *
 *      node examples/read-http.ts <url> [minX minY maxX maxY]
 *
 *  Opening reads only the header. A bbox query then reads a few index
 *  pages plus the matching features, so a query against a file of tens
 *  of gigabytes transfers kilobytes. The request count is printed
 *  throughout because that, not the wall clock, is what the format is
 *  designed to minimise.
 *
 *  `fromUrl` wraps the source in a BufferedRangeReader for you (unlike
 *  `fromReader`, which takes the source exactly as given) -- see
 *  custom-reader.ts for the difference that makes.
 *
 *  ALWAYS pass a bbox on a large file: with no bbox this would scan the
 *  whole thing.
 */
import { FcbReader, toCityJSONFeature } from '@cityjson/flatcitybuf'

const [url, ...rest] = process.argv.slice(2)
if (url === undefined || (rest.length !== 0 && rest.length !== 4)) {
  console.log('usage: node examples/read-http.ts <url> [minX minY maxX maxY]')
  process.exit(2)
}

// Counting fetch: the reader takes any fetch-compatible function, which
// is also how you would add auth headers, retries or a cache.
let requests = 0
const countingFetch: typeof globalThis.fetch = (...args) => {
  requests += 1
  return globalThis.fetch(...args)
}

const t0 = Date.now()
const reader = await FcbReader.fromUrl(url, { fetch: countingFetch })
const info = reader.header.info
const secs = (since: number) => ((Date.now() - since) / 1000).toFixed(1)

console.log(`${info.featuresCount} features, CityJSON ${info.version}`)
console.log(`opened in ${requests} HTTP request(s), ${secs(t0)}s`)

if (rest.length !== 4) {
  console.log('no bbox given; not scanning the whole file -- pass one')
} else {
  const t1 = Date.now()
  const hits = await reader.select({
    spatial: { kind: 'bbox', value: rest.map(Number) as [number, number, number, number] },
  })
  // featuresCount is the TOTAL number of matches, known before iterating.
  console.log(
    `${hits.featuresCount} feature(s) in the query bbox, ` +
      `${requests} HTTP request(s), ${secs(t1)}s`,
  )

  const t2 = Date.now()
  let n = 0
  for await (const feature of hits) {
    toCityJSONFeature(feature, reader.header)
    n += 1
  }
  console.log(`decoded all ${n}, ${requests} HTTP request(s) total, +${secs(t2)}s`)
}
await reader.close()
