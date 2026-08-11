/** Runs every script in examples/ so they cannot rot.
 *
 *  The examples are documentation, and documentation that is never
 *  executed drifts silently: an API rename or a changed accessor breaks
 *  them without breaking anything else the suite covers. Each is run as
 *  a real subprocess -- the way a reader would run it -- and asserted on
 *  exit status plus a line only a working run can print.
 *
 *  They import `@cityjson/flatcitybuf` by name, resolved through the
 *  package's own `exports` (Node self-referencing), so they exercise
 *  `dist/` exactly as a consumer would. That makes `just build` a
 *  prerequisite, which `just check` already satisfies by running `build`
 *  -- and a stale `dist/` is itself worth catching.
 *
 *  The live-HTTP example is exercised for its argument handling only;
 *  the network path is covered by test/http.test.ts's opt-in test.
 */
import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const ROOT = resolve(__dirname, '..')
const EXAMPLES = resolve(ROOT, 'examples')
const DELFT = resolve(ROOT, '../../examples/data/delft.fcb')
const CORPUS = resolve(ROOT, '../../conformance')
const BBOX = ['84500', '445800', '85000', '446500']

interface Result {
  status: number
  stdout: string
  stderr: string
}

function run(script: string, ...args: string[]): Result {
  try {
    const stdout = execFileSync('node', [resolve(EXAMPLES, script), ...args], {
      cwd: ROOT,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      // The whole-file CityJSONSeq run emits ~6.4 MB; the default 1 MB
      // buffer kills the child and surfaces as a bare `status: -1`.
      maxBuffer: 64 * 1024 * 1024,
    })
    return { status: 0, stdout, stderr: '' }
  } catch (e) {
    const err = e as { status?: number; stdout?: string; stderr?: string }
    return { status: err.status ?? -1, stdout: err.stdout ?? '', stderr: err.stderr ?? '' }
  }
}

const hasDelft = existsSync(DELFT)
const withDelft = hasDelft ? it : it.skip

describe('examples', () => {
  it('every script on disk is covered by a test here', () => {
    // A new example must be added below too, or it ships unexecuted --
    // exactly the drift these tests exist to prevent.
    const onDisk = readdirSync(EXAMPLES)
      .filter((f) => f.endsWith('.ts'))
      .sort()
    expect(onDisk).toEqual([
      'custom-reader.ts',
      'inspect-header.ts',
      'int64-policy.ts',
      'query-attributes.ts',
      'read-features.ts',
      'read-http.ts',
      'read-local.ts',
      'to-cityjson.ts',
    ])
  })

  withDelft('inspect-header reports the queryable columns', () => {
    const r = run('inspect-header.ts', DELFT)
    expect(r.status, r.stderr).toBe(0)
    expect(r.stdout).toContain('features      1115')
    expect(r.stdout).toContain('44 of 44 columns are queryable')
  })

  withDelft('read-local emits a header line plus every feature', () => {
    const r = run('read-local.ts', DELFT)
    expect(r.status, r.stderr).toBe(0)
    expect(r.stdout.trim().split('\n')).toHaveLength(1116)
  })

  withDelft('read-local with a bbox emits only the matches', () => {
    const r = run('read-local.ts', DELFT, ...BBOX)
    expect(r.status, r.stderr).toBe(0)
    // 170 features plus the CityJSON header line -- the same 170 the C++
    // and Python readers and the Rust writer's bbox filter agree on.
    expect(r.stdout.trim().split('\n')).toHaveLength(171)
  })

  withDelft('query-attributes matches on two ANDed conditions', () => {
    const r = run(
      'query-attributes.ts', DELFT,
      'b3_h_dak_50p', 'Gt', '20',
      'b3_dak_type', 'Eq', 'slanted',
    )
    expect(r.status, r.stderr).toBe(0)
    expect(r.stdout).toContain('1 of 1115 features matched')
    expect(r.stdout).toContain('NL.IMBAG.Pand.0503100000032914')
  })

  withDelft('query-attributes rejects an unknown column', () => {
    const r = run('query-attributes.ts', DELFT, 'nope', 'Eq', '1')
    expect(r.status).toBe(1)
    expect(r.stderr).toContain('no column named')
  })

  withDelft('read-features shows the per-object schema override', () => {
    const r = run('read-features.ts', DELFT, '1')
    expect(r.status, r.stderr).toBe(0)
    expect(r.stdout).toContain('schema   own')
    expect(r.stdout).toContain('1 object(s) carried their own schema')
  })

  withDelft('to-cityjson reaches into the decoded objects', () => {
    const r = run('to-cityjson.ts', DELFT, '0')
    expect(r.status, r.stderr).toBe(0)
    expect(r.stdout).toContain('== metadata (toCityJSONMetadata) ==')
    expect(r.stdout).toContain('NL.IMBAG.Pand.0503100000031902')
  })

  it('to-cityjson shows templates and their palette together', () => {
    // geom_temp's header carries BOTH geometry templates and the
    // appearance palette those templates index -- the pair finding #31
    // was about.
    const r = run('to-cityjson.ts', resolve(CORPUS, 'geom_temp.fcb'), '0')
    expect(r.status, r.stderr).toBe(0)
    expect(r.stdout).toContain('templates 3')
    expect(r.stdout).toContain('palette   2 material(s), 2 texture(s)')
  })

  withDelft('custom-reader shows what buffering costs and saves', () => {
    const r = run('custom-reader.ts', DELFT, ...BBOX)
    expect(r.status, r.stderr).toBe(0)
    // Both rows report the same 170 hits; the point is the read counts.
    expect(r.stdout).toMatch(/raw\s*: 170 hit\(s\), \d+ read\(s\)/)
    expect(r.stdout).toMatch(/buffered: 170 hit\(s\), \d+ read\(s\)/)

    const raw = Number(/raw\s*: 170 hit\(s\), (\d+) read/.exec(r.stdout)?.[1])
    const buffered = Number(/buffered: 170 hit\(s\), (\d+) read/.exec(r.stdout)?.[1])
    expect(buffered).toBeLessThan(raw)
  })

  it('int64-policy demonstrates the 53-bit hazard', () => {
    const r = run('int64-policy.ts', resolve(CORPUS, 'inferable_types.fcb'))
    expect(r.status, r.stderr).toBe(0)
    expect(r.stdout).toContain('round trip is lossless?  false')
    // The same value under both policies: a number, then a string.
    expect(r.stdout).toContain('a_long=-42')
    expect(r.stdout).toContain('a_long="-42"')
  })

  it('read-http prints usage without arguments', () => {
    const r = run('read-http.ts')
    expect(r.status).toBe(2)
    expect(r.stdout).toContain('usage:')
  })
})
