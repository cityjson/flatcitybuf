/** Attribute queries through the static B+tree.
 *
 *      node examples/query-attributes.ts f.fcb b3_h_dak_50p Gt 20
 *      node examples/query-attributes.ts f.fcb b3_h_dak_50p Gt 20 \
 *                                              b3_dak_type Eq slanted
 *
 *  Several conditions are ANDed. Matching is EXISTENTIAL over a
 *  feature's CityObjects: the feature matches if any one object does.
 *
 *  Unlike the C++ and Python ports, no typed key has to be constructed
 *  by hand -- `value` is a plain JS value and the reader coerces it to
 *  the column's own type, throwing `InvalidArgument` before any I/O if
 *  it cannot. So a string compared against a Double is an error here
 *  rather than reinterpreted bytes.
 */
import { ColumnType, type Operator } from '@cityjson/flatcitybuf'
import { fromFile } from '@cityjson/flatcitybuf/node'

const [path, ...rest] = process.argv.slice(2)
if (path === undefined || rest.length === 0 || rest.length % 3 !== 0) {
  console.log(
    'usage: node examples/query-attributes.ts <file.fcb> <field> <op> <value>...',
  )
  process.exit(2)
}

const OPS = new Set(['Eq', 'Ne', 'Gt', 'Ge', 'Lt', 'Le'])

const reader = await fromFile(path)
try {
  const byName = new Map(reader.header.info.columns.map((c) => [c.name, c]))

  const where = []
  for (let i = 0; i < rest.length; i += 3) {
    const [field, op, text] = [rest[i]!, rest[i + 1]!, rest[i + 2]!]
    const col = byName.get(field)
    if (col === undefined) {
      console.error(`error: no column named ${JSON.stringify(field)}`)
      process.exit(1)
    }
    if (!OPS.has(op)) {
      console.error(`error: unknown operator ${JSON.stringify(op)}`)
      process.exit(1)
    }
    // Numeric columns want a number, Bool a boolean, everything else the
    // string as given.
    const numeric =
      col.type !== ColumnType.String &&
      col.type !== ColumnType.Json &&
      col.type !== ColumnType.Binary &&
      col.type !== ColumnType.DateTime &&
      col.type !== ColumnType.Bool
    const value = col.type === ColumnType.Bool
      ? text === 'true' || text === '1'
      : numeric
        ? Number(text)
        : text
    console.log(`condition: ${field} ${op} ${text} (column type ${ColumnType[col.type]})`)
    where.push({ field, operator: op as Operator, value })
  }

  const hits = await reader.select({ where })
  console.log(`${hits.featuresCount} of ${reader.header.info.featuresCount} features matched`)

  let shown = 0
  for await (const feature of hits) {
    if (shown < 20) console.log(`  ${feature.id}`)
    shown += 1
  }
  if (shown > 20) console.log(`  ... ${shown - 20} more`)
} finally {
  await reader.close()
}
