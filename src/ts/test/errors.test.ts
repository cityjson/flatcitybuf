import { describe, expect, it } from 'vitest'
import { ErrorCode, FcbError } from '../src/errors.js'

describe('FcbError', () => {
  it('carries its code and message', () => {
    const err = new FcbError(ErrorCode.MissingMagicBytes, 'bad magic')
    expect(err.code).toBe(ErrorCode.MissingMagicBytes)
    expect(err.message).toContain('bad magic')
  })

  it('is an Error, catchable and instanceof-checkable', () => {
    // Subclassing Error breaks instanceof unless the prototype is restored;
    // TS targeting ES5 silently loses it. This test pins that it works.
    try {
      throw new FcbError(ErrorCode.IoError, 'boom')
    } catch (e) {
      expect(e).toBeInstanceOf(FcbError)
      expect(e).toBeInstanceOf(Error)
      expect((e as FcbError).code).toBe(ErrorCode.IoError)
    }
  })

  it('has a name that identifies it in stack traces', () => {
    expect(new FcbError(ErrorCode.IoError, 'x').name).toBe('FcbError')
  })
})
