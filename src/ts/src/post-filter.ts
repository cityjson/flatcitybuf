/** The string post-filter -- ports the verification half of
 *  `FcbReader::select_attr` (src/cpp/src/reader.cpp:394-436) and its
 *  `value_satisfies` helper (reader.cpp:259-317).
 *
 *  ---------------------------------------------------------------------
 *  WHY THIS EXISTS: THE INDEX ANSWERS WITH CANDIDATES
 *  ---------------------------------------------------------------------
 *  A `String` column is indexed by a FIXED-WIDTH key: the value's UTF-8 bytes
 *  truncated to 50 and zero-padded to 50. Two consequences, and both of them
 *  are why a hit is a candidate rather than an answer:
 *
 *   1. TRUNCATION. Any two values sharing their first 50 bytes have the
 *      identical key. In conformance/colliding_strings.fcb, `'k'*50+'alpha'`,
 *      `'k'*50+'beta'` and `'k'*50` are one key, so `Eq('k'*50)` cannot be
 *      answered by the index at all -- it returns all three.
 *   2. ZERO PADDING. `'a'` and `'a\0'` also produce the identical key. This is
 *      why the post-filter is **NOT gated on the query's length**: a length
 *      gate would skip verification for `'a'` and hand back the wrong one of
 *      the two. It bites in practice even without an embedded NUL, because of
 *      the next paragraph.
 *
 *  ---------------------------------------------------------------------
 *  THE INDEX BOUNDS ARE DELIBERATELY LOOSE, AND MUST STAY THAT WAY
 *  ---------------------------------------------------------------------
 *  For string kinds the traversal uses NON-STRICT bounds for `Gt`/`Lt` and a
 *  full leaf scan for `Ne` (see `scanRange` in static-btree/stree.ts). That is
 *  not sloppiness: `'k'*50+'alpha'` is genuinely GREATER than `'k'*50`, yet
 *  its key compares EQUAL to the query's. A strict bound would drop the whole
 *  equal-prefix band and with it two real matches, and NO post-filter can
 *  recover a candidate the traversal never emitted. So the division of labour
 *  is: the index may only over-return, this module may only narrow. Never
 *  "tighten" a string bound in stree.ts to compensate for work done here.
 *
 *  The price is visible in `Gt('a')` on that fixture: the index returns all
 *  five features, including `short_a` whose value IS `'a'`, and this module is
 *  what removes it.
 *
 *  ---------------------------------------------------------------------
 *  ORDERING
 *  ---------------------------------------------------------------------
 *  `FcbReader.select` runs search -> intersect -> POST-FILTER -> count -> page.
 *  Filtering after counting would report candidate counts as match counts
 *  (`Eq('k'*50)` would say 3 where the answer is 1), and paging before
 *  filtering would page a list that still holds non-matches.
 *
 *  Only `String` columns reach here. `Json`/`Binary` columns are rejected
 *  upstream in static-btree/query.ts (divergence #2), and every other kind is
 *  indexed by its exact value, so the index answer IS the answer. */
import { ErrorCode, FcbError } from './errors.js'
import type { AttrValue } from './feature/index.js'
import type { Feature } from './feature/index.js'
import type { ColumnInfo, HeaderView } from './header/index.js'
import type { AttrCondition, Operator } from './reader.js'
import { keyKindForColumn, needsPostFilter } from './static-btree/index.js'

const utf8Encoder = new TextEncoder()

/** Compares two FULL, untruncated attribute values as UNSIGNED UTF-8 bytes.
 *
 *  Never `a < b` on the JS strings. JS relational operators compare UTF-16
 *  CODE UNITS, which disagrees with UTF-8 byte order for supplementary-plane
 *  text: `'｡' < '\u{10000}'` is `false` in JS (0xFF61 > 0xD800) but the byte
 *  comparison says `'｡'` (EF BD A1) sorts before `'\u{10000}'` (F0 90 80 80).
 *  Every ASCII test passes either way, which is exactly what makes the wrong
 *  version survive review. `compareKeys` in static-btree/key.ts already
 *  compares keys byte-wise for the same reason; this is the same rule applied
 *  to the untruncated value. */
export function compareFullStrings(a: string, b: string): number {
  return compareBytes(utf8Encoder.encode(a), utf8Encoder.encode(b))
}

function compareBytes(a: Uint8Array, b: Uint8Array): number {
  const len = Math.min(a.length, b.length)
  for (let i = 0; i < len; i++) {
    const diff = (a[i] as number) - (b[i] as number)
    if (diff !== 0) return diff < 0 ? -1 : 1
  }
  return a.length === b.length ? 0 : a.length < b.length ? -1 : 1
}

function satisfies(cmp: number, op: Operator): boolean {
  switch (op) {
    case 'Eq': return cmp === 0
    case 'Ne': return cmp !== 0
    case 'Gt': return cmp > 0
    case 'Ge': return cmp >= 0
    case 'Lt': return cmp < 0
    case 'Le': return cmp <= 0
    default: {
      const exhaustive: never = op
      throw new FcbError(ErrorCode.InvalidArgument, `unknown operator ${String(exhaustive)}`)
    }
  }
}

/** The untruncated operand a string condition was written with, as UTF-8
 *  bytes -- encoded once per condition, then reused for every candidate.
 *
 *  `static-btree/query.ts` also accepts a pre-encoded `Uint8Array` operand for
 *  callers driving the index directly -- but those 50 bytes ARE the truncated
 *  key, so there is nothing left to verify against and silently accepting one
 *  would report candidates as answers. Rejecting is the honest answer, and it
 *  is the same policy `query.ts` applies to Json/Binary columns. */
function operandBytes(cond: AttrCondition): Uint8Array {
  if (typeof cond.value === 'string') return utf8Encoder.encode(cond.value)
  throw new FcbError(
    ErrorCode.InvalidArgument,
    `condition on "${cond.field}" needs the full string value to post-filter; `
    + 'a pre-encoded key operand is already truncated and cannot be verified',
  )
}

function columnFor(
  columns: readonly ColumnInfo[], name: string,
): ColumnInfo | undefined {
  return columns.find((c) => c.name === name)
}

/** True iff `value` is a string that satisfies `op` against `want`. A
 *  non-string (a number, a `Uint8Array`, `null`) is a schema mismatch, and
 *  reader.cpp:259-289 answers `false` for it rather than coercing -- coercion
 *  would invent an ordering the index was never built with. */
function valueSatisfies(value: AttrValue | undefined, op: Operator, want: Uint8Array): boolean {
  if (typeof value !== 'string') return false
  // `want` is encoded ONCE per condition by the caller, not once per
  // candidate: a long operand against a wide equal-prefix band would
  // otherwise re-encode the same megabytes for every hit.
  return satisfies(compareBytes(utf8Encoder.encode(value), want), op)
}

/** True iff any condition in `conditions` targets a column whose index needs
 *  verification. Lets `select` skip reading candidate features entirely when
 *  nothing is post-filterable -- the common case. */
export function requiresPostFilter(
  header: HeaderView, conditions: readonly AttrCondition[],
): boolean {
  return conditions.some((cond) => {
    const column = columnFor(header.info.columns, cond.field)
    return column !== undefined && needsPostFilter(keyKindForColumn(column.type))
  })
}

/** Re-evaluates every post-filterable condition against `feature`'s DECODED,
 *  untruncated attributes. `true` keeps the candidate.
 *
 *  Semantics, matching reader.cpp:405-425 exactly:
 *   * Conditions on non-string columns are SKIPPED, not re-checked: their keys
 *     are exact, so the index already answered them. (They are still ANDed --
 *     by the index intersection in `searchAttributes`, which ran first.)
 *   * A condition is EXISTENTIAL over the feature's CityObjects: the feature
 *     survives if ANY one of its objects carries a matching value. This is the
 *     same rule `searchAttributes` uses, and it is why a feature holding both
 *     `'a'` and `'z'` matches `Gt('m')`.
 *   * Each CityObject is decoded with ITS OWN column schema. That is not an
 *     optimisation: `CityObject.columns` overrides `Header.columns` whenever
 *     present, which is the normal case (in examples/data/delft.fcb the
 *     header's 44 columns are never used), and attribute blobs are not
 *     self-delimiting -- decoding with the wrong schema yields plausible
 *     garbage, not an error. `CityObjectView.attributes()` resolves this.
 *   * Conditions are ANDed; the first failure short-circuits. */
export function postFilterCandidates(
  feature: Feature,
  header: HeaderView,
  conditions: readonly AttrCondition[],
): boolean {
  for (const cond of conditions) {
    const column = columnFor(header.info.columns, cond.field)
    // Unreachable through `select` -- `searchAttributes` already threw for an
    // unknown column -- but a direct caller gets "no match" rather than a
    // silently satisfied condition.
    if (column === undefined) return false
    if (!needsPostFilter(keyKindForColumn(column.type))) continue

    const want = operandBytes(cond)
    let matched = false
    for (const object of feature.cityObjects()) {
      if (valueSatisfies(object.attributes()[cond.field], cond.operator, want)) {
        matched = true
        break
      }
    }
    if (!matched) return false
  }
  return true
}
