/**
 * Byte-array helpers used by the response body viewer.
 */

/**
 * Encode a number array as base64. Uses chunked `String.fromCharCode` so we
 * don't blow the stack with `apply()` on large bodies.
 */
export function bytesToBase64(bytes: number[]): string {
  const CHUNK = 0x8000;
  let s = '';
  for (let i = 0; i < bytes.length; i += CHUNK) {
    s += String.fromCharCode(...bytes.slice(i, i + CHUNK));
  }
  return btoa(s);
}

/** Build a `data:` URL from raw bytes + a content type. */
export function bytesToDataUrl(bytes: number[], contentType: string): string {
  return `data:${contentType};base64,${bytesToBase64(bytes)}`;
}
