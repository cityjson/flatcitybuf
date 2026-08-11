/** `Long`/`ULong` attributes and JavaScript's 53-bit safe integer.
 *
 *      node examples/int64-policy.ts ../../conformance/inferable_types.fcb
 *
 *  This one has no counterpart in the C++ or Python examples, because
 *  the hazard is JavaScript's alone: a JS `number` is a float64, so it
 *  carries only 53 bits of integer precision, while the format's `Long`
 *  and `ULong` columns are full 64-bit. Above `Number.MAX_SAFE_INTEGER`
 *  a plain number silently stops being able to represent every value.
 *
 *  `toCityJSONFeature` therefore takes an `Int64Policy`:
 *
 *    'lossy-number'   (default) a JS number, rounding past 2^53-1. It is
 *                     what makes whole-line comparison against the
 *                     conformance oracle meaningful, since that is what
 *                     the Rust reader's JSON contains.
 *    'decimal-string' every digit kept, at the cost of changing the JSON
 *                     type from number to string.
 *    'error'          throws rather than lose a digit silently.
 *
 *  No policy ever leaks a `bigint` into the emitted object.
 */
import { ColumnType, toCityJSONFeature } from '@cityjson/flatcitybuf'
import { fromFile } from '@cityjson/flatcitybuf/node'

const path = process.argv[2]
if (path === undefined) {
  console.log('usage: node examples/int64-policy.ts <file.fcb>')
  process.exit(2)
}

// The hazard itself, in plain JS -- no file needed to see it.
console.log('== why the policy exists ==')
const big = 9007199254740993n // 2^53 + 1
console.log(`  Number.MAX_SAFE_INTEGER  ${Number.MAX_SAFE_INTEGER}`)
console.log(`  the i64 value            ${big}`)
console.log(`  as a JS number           ${Number(big)}   <- a digit is gone`)
console.log(`  round trip is lossless?  ${BigInt(Number(big)) === big}`)

const reader = await fromFile(path)
try {
  const int64Columns = reader.header.info.columns.filter(
    (c) => c.type === ColumnType.Long || c.type === ColumnType.ULong,
  )
  console.log()
  console.log('== this file ==')
  if (int64Columns.length === 0) {
    console.log('  no Long/ULong columns; the policy cannot apply here')
  } else {
    console.log(`  Long/ULong columns: ${int64Columns.map((c) => c.name).join(', ')}`)
  }

  let atRisk = 0
  for await (const feature of await reader.select({ limit: 1 })) {
    for (const policy of ['lossy-number', 'decimal-string'] as const) {
      const cj = toCityJSONFeature(feature, reader.header, { int64: policy })
      for (const obj of Object.values(cj.CityObjects)) {
        const attrs = obj.attributes ?? {}
        const shown = int64Columns
          .filter((c) => c.name in attrs)
          .map((c) => `${c.name}=${JSON.stringify(attrs[c.name])}`)
        if (shown.length > 0) console.log(`  ${policy.padEnd(15)} ${shown.join(' ')}`)
      }
    }

    // Whether any value in THIS file would actually lose a digit.
    const exact = toCityJSONFeature(feature, reader.header, { int64: 'decimal-string' })
    for (const obj of Object.values(exact.CityObjects)) {
      for (const c of int64Columns) {
        const v = (obj.attributes ?? {})[c.name]
        if (typeof v === 'string' && !Number.isSafeInteger(Number(v))) atRisk += 1
      }
    }
  }

  console.log()
  console.log(
    atRisk === 0
      ? '  no value here exceeds 2^53-1, so all three policies agree on the\n' +
          '  NUMBER -- they still differ in the JSON TYPE, as shown above'
      : `  ${atRisk} value(s) here exceed 2^53-1: 'lossy-number' would round them`,
  )
} finally {
  await reader.close()
}
