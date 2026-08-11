/** Whole file (or one bbox) out as CityJSONSeq, on stdout.
 *
 *      node examples/read-local.ts in.fcb > out.city.jsonl
 *      node examples/read-local.ts in.fcb 84500 445800 85000 446500
 *
 *  With a bbox the R-tree answers first and only the matching features
 *  are read; without one this is a straight sequential scan. Progress
 *  goes to stderr so stdout stays a clean CityJSONSeq stream.
 */
import { toCityJSONFeature, toCityJSONMetadata } from '@cityjson/flatcitybuf'
import { fromFile } from '@cityjson/flatcitybuf/node'

const [path, ...rest] = process.argv.slice(2)
if (path === undefined || (rest.length !== 0 && rest.length !== 4)) {
  console.log('usage: node examples/read-local.ts <file.fcb> [minX minY maxX maxY]')
  process.exit(2)
}

const reader = await fromFile(path)
try {
  const info = reader.header.info
  const crs = info.referenceSystem === undefined ? '' : `, ${info.referenceSystem}`
  console.error(`${info.featuresCount} features, CityJSON ${info.version}${crs}`)

  // Line 0 is the CityJSON header: transform, metadata, and the geometry
  // templates and appearance palette when the file has them.
  console.log(JSON.stringify(toCityJSONMetadata(reader.header)))

  const cursor =
    rest.length === 4
      ? await reader.select({
          spatial: {
            kind: 'bbox',
            value: rest.map(Number) as [number, number, number, number],
          },
        })
      : await reader.selectAll()

  // featuresCount is the TOTAL number of matches, not the page size --
  // it is unaffected by limit/offset.
  if (rest.length === 4) console.error(`${cursor.featuresCount} feature(s) in the bbox`)

  let n = 0
  for await (const feature of cursor) {
    console.log(JSON.stringify(toCityJSONFeature(feature, reader.header)))
    n += 1
  }
  console.error(`wrote ${n} feature(s)`)
} finally {
  await reader.close()
}
