/** Every wire read goes through here. DataView getters default to BIG-endian
 *  when the flag is omitted, so a single forgotten `true` yields plausible
 *  garbage -- a byteswapped f64 bbox is still a finite f64. Nothing outside
 *  this module may call a raw DataView getter. */
import { ErrorCode, FcbError } from './errors.js'

export const readU16 = (dv: DataView, o: number): number => dv.getUint16(o, true)
export const readU32 = (dv: DataView, o: number): number => dv.getUint32(o, true)
export const readI32 = (dv: DataView, o: number): number => dv.getInt32(o, true)
export const readU64 = (dv: DataView, o: number): bigint => dv.getBigUint64(o, true)
export const readI64 = (dv: DataView, o: number): bigint => dv.getBigInt64(o, true)
export const readF32 = (dv: DataView, o: number): number => dv.getFloat32(o, true)
export const readF64 = (dv: DataView, o: number): number => dv.getFloat64(o, true)

/** Converts a wire u64 that is known to be a file position. Throws rather
 *  than silently rounding: a 2^53+ offset read as a Number indexes nowhere. */
export function toSafeNumber(v: bigint, what: string): number {
  if (v > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new FcbError(ErrorCode.InvalidFlatbuffer,
      `${what} ${v} exceeds Number.MAX_SAFE_INTEGER`)
  }
  return Number(v)
}
