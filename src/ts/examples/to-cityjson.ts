/** The CityJSON representation, and how to reach into its fields.
 *
 *      node examples/to-cityjson.ts in.fcb [featureIndex]
 *
 *  `toCityJSONMetadata` gives the CityJSONSeq header line and
 *  `toCityJSONFeature` gives one feature line. Both return plain typed
 *  objects, so everything below is ordinary property access.
 */
import { toCityJSONFeature, toCityJSONMetadata } from '@cityjson/flatcitybuf'
import { fromFile } from '@cityjson/flatcitybuf/node'

const path = process.argv[2]
const index = process.argv[3] === undefined ? 0 : Number(process.argv[3])
if (path === undefined) {
  console.log('usage: node examples/to-cityjson.ts <file.fcb> [featureIndex]')
  process.exit(2)
}

const reader = await fromFile(path)
try {
  console.log('== metadata (toCityJSONMetadata) ==')
  const meta = toCityJSONMetadata(reader.header)
  console.log(`  version   ${meta.version}`)
  console.log(`  scale     [${meta.transform.scale.join(', ')}]`)
  console.log(`  translate [${meta.transform.translate.join(', ')}]`)
  if (meta.metadata?.referenceSystem !== undefined) {
    console.log(`  CRS       ${meta.metadata.referenceSystem}`)
  }
  if (meta.metadata?.geographicalExtent !== undefined) {
    console.log(`  extent    [${meta.metadata.geographicalExtent.join(', ')}]`)
  }

  // Present only when the file has them. The templates' material and
  // texture mappings index the header's OWN appearance palette, which is
  // why both must be emitted together -- emit one without the other and
  // those mappings point at nothing.
  const templates = meta['geometry-templates']
  if (templates !== undefined) console.log(`  templates ${templates.templates.length}`)
  if (meta.appearance !== undefined) {
    const ap = meta.appearance
    console.log(
      `  palette   ${ap.materials?.length ?? 0} material(s), ` +
        `${ap.textures?.length ?? 0} texture(s)`,
    )
  }
  if (meta.extensions !== undefined) {
    console.log(`  extensions ${Object.keys(meta.extensions).sort().join(', ')}`)
  }

  console.log()
  console.log(`== feature ${index} (toCityJSONFeature) ==`)
  let cj
  for await (const feature of await reader.select({ offset: index, limit: 1 })) {
    cj = toCityJSONFeature(feature, reader.header)
  }
  if (cj === undefined) {
    console.error(`no feature at index ${index}`)
    process.exit(1)
  }

  console.log(`  id        ${cj.id}`)
  console.log(`  vertices  ${cj.vertices.length}`)
  for (const [objId, obj] of Object.entries(cj.CityObjects)) {
    const geoms = obj.geometry ?? []
    console.log(`  object    ${objId}`)
    console.log(`    type      ${obj.type}`)
    console.log(`    geometry  ${geoms.length} (lod ${geoms.map((g) => g.lod).join(', ')})`)
    const attrs = obj.attributes
    if (attrs !== undefined && Object.keys(attrs).length > 0) {
      const keys = Object.keys(attrs)
      const head = keys.slice(0, 3).map((k) => `${k}=${JSON.stringify(attrs[k])}`)
      console.log(`    attrs     ${keys.length}, e.g. ${head.join(' ')}`)
    }
  }

  console.log()
  const bytes = JSON.stringify(cj).length
  console.log('  the whole feature as one JSON line is what read-local.ts')
  console.log(`  writes; it is ${bytes} bytes here`)
} finally {
  await reader.close()
}
