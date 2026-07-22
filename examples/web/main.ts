/** Browser demo for the native TypeScript FlatCityBuf reader.
 *
 *  Everything here is written against the package's PUBLIC export surface
 *  (`@cityjson/flatcitybuf`, see src/ts/src/index.ts) -- no deep import into
 *  `src/ts/src/...`. It replaces `examples/wasm/`, which drove a compiled
 *  WASM binding instead of this pure-TS reader and could only open a URL,
 *  never a local file.
 */
import {
  type AttrCondition,
  type ColumnInfo,
  ErrorCode,
  FcbError,
  type FeatureCursor,
  FcbReader,
  type HeaderView,
  type Operator,
} from '@cityjson/flatcitybuf'

// The FlatCityBuf wire enum for a column's type (src/fbs/header.fbs). The
// package exposes the NUMBER it read (`ColumnInfo.type`) but not the enum
// itself -- `ColumnType` is used structurally in the header module's types
// but never re-exported from src/index.ts, so a consumer has nothing to
// `import` for it. This table mirrors the spec's own numbering (a stable,
// documented wire format, not an implementation detail) so the demo can
// still show a name and coerce a query value correctly. Worth revisiting if
// a later task decides to export `ColumnType` itself.
const COLUMN_TYPE_NAMES = [
  'Byte', 'UByte', 'Bool', 'Short', 'UShort', 'Int', 'UInt', 'Long', 'ULong',
  'Float', 'Double', 'String', 'Json', 'DateTime', 'Binary',
] as const

function columnTypeName(type: number): string {
  return COLUMN_TYPE_NAMES[type] ?? `Unknown(${type})`
}

function el<T extends HTMLElement = HTMLElement>(id: string): T {
  const found = document.getElementById(id)
  if (found === null) throw new Error(`missing #${id} in index.html`)
  return found as T
}

const urlInput = el<HTMLInputElement>('url-input')
const loadUrlBtn = el<HTMLButtonElement>('load-url-btn')
const dropZone = el('drop-zone')
const fileInput = el<HTMLInputElement>('file-input')
const statusEl = el('status')

const headerSection = el('header-section')
const headerOutput = el('header-output')

const querySection = el('query-section')
const minXInput = el<HTMLInputElement>('min-x')
const minYInput = el<HTMLInputElement>('min-y')
const maxXInput = el<HTMLInputElement>('max-x')
const maxYInput = el<HTMLInputElement>('max-y')
const bboxQueryBtn = el<HTMLButtonElement>('bbox-query-btn')
const bboxStatus = el('bbox-status')
const bboxResults = el('bbox-results')

const attrSection = el('attr-section')
const attrFieldSelect = el<HTMLSelectElement>('attr-field')
const attrOperatorSelect = el<HTMLSelectElement>('attr-operator')
const attrValueInput = el<HTMLInputElement>('attr-value')
const attrQueryBtn = el<HTMLButtonElement>('attr-query-btn')
const attrStatus = el('attr-status')
const attrResults = el('attr-results')

let reader: FcbReader | undefined
let columns: ColumnInfo[] = []

function setStatus(target: HTMLElement, message: string): void {
  target.textContent = message
}

function describeError(err: unknown): string {
  if (err instanceof FcbError) return `${err.code}: ${err.message}`
  if (err instanceof Error) return err.message
  return String(err)
}

/** Renders the parts of `header.info` a user needs to know what they can
 *  query: version, feature count, geographical extent, CRS, and every
 *  attribute column with its type. */
function renderHeader(header: HeaderView): void {
  const info = header.info
  const lines: string[] = []
  lines.push(`version:            ${info.version}`)
  lines.push(
    `features:           ${info.featuresCount === 0 ? 'unknown' : info.featuresCount}`,
  )
  lines.push(`reference system:   ${info.referenceSystem ?? '(none)'}`)
  if (info.geographicalExtent !== undefined) {
    const [minX, minY, minZ, maxX, maxY, maxZ] = info.geographicalExtent
    lines.push(`geographical extent:`)
    lines.push(`  min: [${minX}, ${minY}, ${minZ}]`)
    lines.push(`  max: [${maxX}, ${maxY}, ${maxZ}]`)
  } else {
    lines.push('geographical extent: (none)')
  }
  lines.push('')
  lines.push(`columns (${info.columns.length}):`)
  for (const c of info.columns) {
    lines.push(
      `  ${c.name.padEnd(30)} ${columnTypeName(c.type)}${c.nullable ? '' : ' NOT NULL'}`,
    )
  }
  headerOutput.textContent = lines.join('\n')
  headerSection.classList.remove('hidden')
}

/** Populates the attribute-query field dropdown, and seeds the bbox inputs
 *  from the file's own extent so a first query has a sane default. */
function populateQueryInputs(header: HeaderView): void {
  columns = header.info.columns
  attrFieldSelect.replaceChildren(
    ...columns.map((c) => {
      const opt = document.createElement('option')
      opt.value = c.name
      opt.textContent = `${c.name} (${columnTypeName(c.type)})`
      return opt
    }),
  )

  const extent = header.info.geographicalExtent
  if (extent !== undefined) {
    const [minX, minY, , maxX, maxY] = extent
    minXInput.value = String(minX)
    minYInput.value = String(minY)
    maxXInput.value = String(maxX)
    maxYInput.value = String(maxY)
  }

  querySection.classList.remove('hidden')
  attrSection.classList.remove('hidden')
}

/** Coerces a raw text-input value into the type `select`'s `where` expects
 *  for this column (src/ts/src/static-btree/query.ts's `toKeyValue`): a
 *  `boolean` for Bool, a `number` for the fixed-width integer and float
 *  kinds, a `bigint` for Long/ULong (a plain `number` would lose precision
 *  above 2^53), a `Date` for DateTime, and the raw string for String. Json
 *  and Binary columns are rejected outright, matching the reader's own
 *  policy (query.ts's divergence #2). */
function coerceAttrValue(column: ColumnInfo, raw: string): unknown {
  switch (columnTypeName(column.type)) {
    case 'Bool':
      if (raw === 'true') return true
      if (raw === 'false') return false
      throw new Error(`"${raw}" is not "true" or "false"`)
    case 'Byte':
    case 'UByte':
    case 'Short':
    case 'UShort':
    case 'Int':
    case 'UInt': {
      const n = Number(raw)
      if (!Number.isInteger(n)) throw new Error(`"${raw}" is not an integer`)
      return n
    }
    case 'Long':
    case 'ULong':
      return BigInt(raw)
    case 'Float':
    case 'Double': {
      const n = Number(raw)
      if (Number.isNaN(n)) throw new Error(`"${raw}" is not a number`)
      return n
    }
    case 'DateTime': {
      const d = new Date(raw)
      if (Number.isNaN(d.getTime())) throw new Error(`"${raw}" is not a valid date`)
      return d
    }
    case 'String':
      return raw
    default:
      throw new Error(
        `column "${column.name}" (${columnTypeName(column.type)}) does not support attribute queries`,
      )
  }
}

/** Drains a cursor into the feature ids, reporting the TOTAL match count
 *  (`cursor.featuresCount`) alongside them -- it is the count of every
 *  match, not merely how many this loop iterated. */
async function collectIds(cursor: FeatureCursor): Promise<{ ids: string[]; count: number }> {
  const ids: string[] = []
  for await (const feature of cursor) ids.push(feature.id)
  return { ids, count: cursor.featuresCount ?? ids.length }
}

function renderResults(target: HTMLElement, ids: string[]): void {
  if (ids.length === 0) {
    target.textContent = '(no features matched)'
    return
  }
  const list = document.createElement('ul')
  for (const id of ids) {
    const li = document.createElement('li')
    li.textContent = id
    list.appendChild(li)
  }
  target.replaceChildren(list)
}

async function onReaderReady(next: FcbReader): Promise<void> {
  reader = next
  renderHeader(next.header)
  populateQueryInputs(next.header)
  setStatus(statusEl, 'file opened.')
}

async function loadFromUrl(): Promise<void> {
  const url = urlInput.value.trim()
  if (url === '') {
    setStatus(statusEl, 'enter a URL first.')
    return
  }
  loadUrlBtn.disabled = true
  setStatus(statusEl, `opening ${url} ...`)
  try {
    const next = await FcbReader.fromUrl(url)
    await onReaderReady(next)
  } catch (err) {
    setStatus(statusEl, `failed to open URL: ${describeError(err)}`)
  } finally {
    loadUrlBtn.disabled = false
  }
}

async function loadFromFile(file: File): Promise<void> {
  setStatus(statusEl, `opening ${file.name} ...`)
  try {
    const next = await FcbReader.fromBlob(file)
    await onReaderReady(next)
  } catch (err) {
    setStatus(statusEl, `failed to open file: ${describeError(err)}`)
  }
}

async function runBboxQuery(): Promise<void> {
  if (reader === undefined) return
  const minX = Number(minXInput.value)
  const minY = Number(minYInput.value)
  const maxX = Number(maxXInput.value)
  const maxY = Number(maxYInput.value)
  if ([minX, minY, maxX, maxY].some((n) => Number.isNaN(n))) {
    setStatus(bboxStatus, 'all four bbox fields must be numbers.')
    return
  }

  bboxQueryBtn.disabled = true
  setStatus(bboxStatus, 'running bbox query...')
  try {
    const cursor = await reader.select({ spatial: { kind: 'bbox', value: [minX, minY, maxX, maxY] } })
    const { ids, count } = await collectIds(cursor)
    setStatus(bboxStatus, `${count} feature(s) matched.`)
    renderResults(bboxResults, ids)
  } catch (err) {
    setStatus(bboxStatus, `query failed: ${describeError(err)}`)
    bboxResults.textContent = ''
  } finally {
    bboxQueryBtn.disabled = false
  }
}

async function runAttrQuery(): Promise<void> {
  if (reader === undefined) return
  const fieldName = attrFieldSelect.value
  const column = columns.find((c) => c.name === fieldName)
  if (column === undefined) {
    setStatus(attrStatus, 'select a field first.')
    return
  }

  let value: unknown
  try {
    value = coerceAttrValue(column, attrValueInput.value)
  } catch (err) {
    setStatus(attrStatus, describeError(err))
    return
  }

  const condition: AttrCondition = {
    field: fieldName,
    operator: attrOperatorSelect.value as Operator,
    value,
  }

  attrQueryBtn.disabled = true
  setStatus(attrStatus, 'running attribute query...')
  try {
    const cursor = await reader.select({ where: [condition] })
    const { ids, count } = await collectIds(cursor)
    setStatus(attrStatus, `${count} feature(s) matched.`)
    renderResults(attrResults, ids)
  } catch (err) {
    setStatus(attrStatus, `query failed: ${describeError(err)}`)
    attrResults.textContent = ''
  } finally {
    attrQueryBtn.disabled = false
  }
}

loadUrlBtn.addEventListener('click', () => void loadFromUrl())
urlInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') void loadFromUrl()
})

fileInput.addEventListener('change', () => {
  const file = fileInput.files?.[0]
  if (file !== undefined) void loadFromFile(file)
})

dropZone.addEventListener('dragover', (e) => {
  e.preventDefault()
  dropZone.classList.add('dragover')
})
dropZone.addEventListener('dragleave', () => {
  dropZone.classList.remove('dragover')
})
dropZone.addEventListener('drop', (e) => {
  e.preventDefault()
  dropZone.classList.remove('dragover')
  const file = e.dataTransfer?.files?.[0]
  if (file !== undefined) void loadFromFile(file)
})

bboxQueryBtn.addEventListener('click', () => void runBboxQuery())
attrQueryBtn.addEventListener('click', () => void runAttrQuery())
