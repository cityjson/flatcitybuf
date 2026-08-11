/** Header only: extent, CRS, transform, and which columns are queryable.
 *
 *  Opening reads the header and nothing else, so this costs one small
 *  read even on a file of tens of gigabytes. Start here on an unfamiliar
 *  file.
 *
 *      node examples/inspect-header.ts ../../examples/data/delft.fcb
 */
import { ColumnType } from '@cityjson/flatcitybuf'
import { fromFile } from '@cityjson/flatcitybuf/node'

const path = process.argv[2]
if (path === undefined) {
  console.log('usage: node examples/inspect-header.ts <file.fcb>')
  process.exit(2)
}

const reader = await fromFile(path)
try {
  const info = reader.header.info

  console.log(`file          ${path}`)
  console.log(`features      ${info.featuresCount}`)
  console.log(`CityJSON      ${info.version}`)
  if (info.title !== undefined) console.log(`title         ${info.title}`)
  if (info.referenceSystem !== undefined) {
    console.log(`CRS           ${info.referenceSystem}`)
  }

  const e = info.geographicalExtent
  if (e !== undefined) {
    const f = (n: number) => n.toFixed(3)
    console.log(
      `extent        [${f(e[0])} ${f(e[1])} ${f(e[2])}]` +
        ` .. [${f(e[3])} ${f(e[4])} ${f(e[5])}]`,
    )
  }
  // `hasTransform` is a real flag, not `scale !== undefined`: an absent
  // transform must stay distinguishable from one that is genuinely zero.
  if (info.hasTransform && info.scale && info.translate) {
    const s = info.scale
    const t = info.translate
    console.log(
      `transform     scale [${s.join(' ')}]` +
        ` translate [${t.map((n) => n.toFixed(3)).join(' ')}]`,
    )
  }

  const rtree = reader.header.layout.rtreeSize > 0
  console.log(
    `R-tree        ${rtree ? `yes (node size ${info.indexNodeSize})` : 'no'}`,
  )

  // A column is queryable only if the writer gave it a B+tree. The header
  // lists those indices separately, keyed by column index -- so this is a
  // set membership test, not a flag on ColumnInfo.
  const indexed = new Set(info.attributeIndices.map((a) => a.columnIndex))
  console.log()
  console.log(`columns (${info.columns.length}; * = queryable via where)`)
  for (const col of info.columns) {
    const mark = indexed.has(col.index) ? '*' : ' '
    console.log(`  ${mark} ${col.name.padEnd(34)} ${ColumnType[col.type]}`)
  }
  console.log()
  console.log(`${indexed.size} of ${info.columns.length} columns are queryable`)
  if (info.semanticColumns.length > 0) {
    console.log(`semantic columns: ${info.semanticColumns.length}`)
  }
} finally {
  await reader.close()
}
