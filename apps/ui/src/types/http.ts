/**
 * TypeScript mirror of `argos_core::http` types.
 *
 * Kept hand-written for clarity and to surface drift loudly during code
 * review. If this drifts in subtle ways from the Rust serde shape, IPC will
 * fail with a deserialisation error — see `apps/ui/src/lib/api.ts` for how
 * those map to friendly UI errors.
 */

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD' | 'OPTIONS';

export const HTTP_METHODS: readonly HttpMethod[] = [
  'GET',
  'POST',
  'PUT',
  'PATCH',
  'DELETE',
  'HEAD',
  'OPTIONS',
] as const;

export type HttpHeader = {
  name: string;
  value: string;
};

export type HttpBody =
  | { kind: 'text'; content: string; content_type: string }
  | { kind: 'json'; value: unknown }
  | { kind: 'form_url_encoded'; fields: Array<[string, string]> }
  | { kind: 'raw'; bytes: number[]; content_type: string };

export type HttpRequest = {
  method: HttpMethod;
  url: string;
  headers: HttpHeader[];
  /** `Vec<(String, String)>` on the Rust side serialises to a tuple array. */
  query: Array<[string, string]>;
  body: HttpBody | null;
  /** Seconds (`f64` from Rust). `null` means use the client default. */
  timeout: number | null;
};

export type Timing = {
  total_ms: number;
  ttfb_ms: number | null;
  download_ms: number | null;
  dns_ms: number | null;
  connect_ms: number | null;
  tls_ms: number | null;
};

export type ResponseBody = {
  /** Raw body bytes serialised as a number array. UI helpers below decode it. */
  bytes: number[];
  size_bytes: number;
  content_type: string | null;
};

export type HttpResponse = {
  status: number;
  status_text: string;
  headers: HttpHeader[];
  body: ResponseBody;
  timing: Timing;
  final_url: string;
};

// ---- helpers --------------------------------------------------------------

export function emptyRequest(method: HttpMethod = 'GET'): HttpRequest {
  return {
    method,
    url: '',
    headers: [],
    query: [],
    body: null,
    timeout: null,
  };
}

export function bytesToString(bytes: number[]): string {
  // The byte array round-trips losslessly even for non-UTF8 — TextDecoder
  // returns the replacement char for invalid sequences without throwing.
  return new TextDecoder('utf-8', { fatal: false }).decode(new Uint8Array(bytes));
}

export function isJsonContentType(ct: string | null | undefined): boolean {
  if (!ct) return false;
  return ct === 'application/json' || ct.endsWith('+json');
}

export function isTextContentType(ct: string | null | undefined): boolean {
  if (!ct) return true; // assume text by default
  return ct.startsWith('text/') || isJsonContentType(ct) || ct === 'application/xml';
}
