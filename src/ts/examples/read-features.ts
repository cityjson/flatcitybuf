/** Raw feature access, without converting to CityJSON.
 *
 *      node examples/read-features.ts in.fcb [count]
 *
 *  Shows the thing most easily got wrong: **attribute schemas are per
 *  object**. A CityObject that carries its own `columns` overrides the
 *  header's, and that is the normal case, not the exception. Attribute
 *  blobs are not self-delimiting, so decoding one against the wrong
 *  schema yields plausible garbage rather than an error.
 */
import { fromFile } from '@cityjson/flatcitybuf/node'

const path = process.argv[2]
const limit = process.argv[3] === undefined ? 1 : Number(process.argv[3])
if (path === undefined) {
  console.log('usage: node examples/read-features.ts <file.fcb> [count]')
  process.exit(2)
}

const reader = await fromFile(path)
try {
  const headerColumns = reader.header.info.columns
  let ownSchema = 0
  let shown = 0

  for await (const feature of await reader.select({ limit })) {
    const objects = feature.cityObjects()
    console.log(`feature ${feature.id}  (${objects.length} CityObjects)`)

    for (const obj of objects) {
      console.log(`  object ${obj.id}`)

      // Presence, not emptiness: an object that declares an empty column
      // list still overrides the header's schema. `columns()` already
      // encodes that fallback, so this is only here to report which won.
      if (obj.hasColumns()) {
        ownSchema += 1
        console.log(`    schema   own (${obj.columns().length} columns)`)
      } else {
        console.log(`    schema   header (${headerColumns.length} columns)`)
      }

      if (!obj.hasAttributes()) {
        console.log('    (no attributes)')
        continue
      }
      const attrs = obj.attributes()
      const keys = Object.keys(attrs)
      if (keys.length === 0) {
        console.log('    (attribute blob present but empty)')
        continue
      }
      for (const key of keys.slice(0, 4)) {
        console.log(`    ${key} = ${JSON.stringify(attrs[key])}`)
      }
      if (keys.length > 4) console.log(`    ... ${keys.length - 4} more attribute(s)`)
    }
    shown += 1
  }

  console.log()
  console.log(`${shown} feature(s) shown; ${ownSchema} object(s) carried their own schema`)
} finally {
  await reader.close()
}
