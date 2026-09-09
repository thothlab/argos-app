/**
 * Dynamic (`$`-prefixed) variables and the `{{…}}` autocomplete model.
 *
 * The generators themselves live in Rust (`crates/core/src/vars.rs`,
 * `BUILTIN_VARS`) — this file is the UI-side mirror used for the
 * suggestion popup. `dynamic-vars.test.ts` fails the build if the two
 * lists drift apart, so there is still exactly one place to add a
 * variable: the Rust catalogue, then here.
 *
 * The helpers below are deliberately pure (caret in, caret out) — the
 * component that owns the `<input>` stays a thin shell, and the fiddly
 * "which token is the caret inside" logic is unit-testable.
 */

export type VarSuggestion = {
  /** Name without braces, e.g. `$randomUuid` or `baseUrl`. */
  name: string;
  description: string;
  /** `dynamic` = generated per request, `env` = from the active environment. */
  kind: 'dynamic' | 'env';
};

/** Mirror of `argos_core::vars::BUILTIN_VARS`. Keep the order in sync too. */
export const DYNAMIC_VARS: VarSuggestion[] = [
  { name: '$randomUuid', description: 'Random UUID v4 — new on every send', kind: 'dynamic' },
  { name: '$guid', description: 'Alias of $randomUuid (Postman)', kind: 'dynamic' },
  { name: '$randomUUID', description: 'Alias of $randomUuid (Postman)', kind: 'dynamic' },
  { name: '$randomInt', description: 'Random 32-bit unsigned integer', kind: 'dynamic' },
  {
    name: '$randomHex',
    description: 'Random 16-char hex string — short request id',
    kind: 'dynamic',
  },
  {
    name: '$randomAlphaNumeric',
    description: 'Random 16-char alphanumeric string',
    kind: 'dynamic',
  },
  { name: '$timestamp', description: 'Unix epoch in milliseconds', kind: 'dynamic' },
  { name: '$timestampSeconds', description: 'Unix epoch in seconds', kind: 'dynamic' },
  { name: '$isoTimestamp', description: 'ISO 8601 timestamp at UTC', kind: 'dynamic' },
];

/** An unterminated `{{…` the caret currently sits inside. */
export type OpenPlaceholder = {
  /** Index of the first `{` of the opening `{{`. */
  start: number;
  /** Text typed between `{{` and the caret. */
  query: string;
};

/**
 * Find the `{{` the caret is inside of, if any.
 *
 * A placeholder counts as "open" while the caret is after its `{{` and
 * before any `}}` — so typing into the middle of an already-complete
 * `{{baseUrl}}` still offers suggestions, but a caret sitting after a
 * closed placeholder does not.
 */
export function findOpenPlaceholder(text: string, caret: number): OpenPlaceholder | null {
  const before = text.slice(0, caret);
  const start = before.lastIndexOf('{{');
  if (start === -1) return null;
  const query = before.slice(start + 2);
  // A `}}` between `{{` and the caret means that placeholder is already closed.
  if (query.includes('}}') || query.includes('{{')) return null;
  // Newlines never occur inside a placeholder — bail out rather than
  // suggesting across lines in a multi-line value.
  if (query.includes('\n')) return null;
  return { start, query };
}

/**
 * Rank suggestions for `query`: prefix matches first, then substring
 * matches, each group keeping the catalogue order. Matching ignores case
 * and a leading `$` the user may or may not have typed.
 */
export function filterSuggestions(all: VarSuggestion[], query: string): VarSuggestion[] {
  const q = query.trim().replace(/^\$/, '').toLowerCase();
  if (q === '') return all;
  const prefix: VarSuggestion[] = [];
  const infix: VarSuggestion[] = [];
  for (const s of all) {
    const name = s.name.replace(/^\$/, '').toLowerCase();
    if (name.startsWith(q)) prefix.push(s);
    else if (name.includes(q)) infix.push(s);
  }
  return [...prefix, ...infix];
}

/** Env variable names as suggestions, sorted alphabetically. */
export function envSuggestions(vars: Record<string, string>): VarSuggestion[] {
  return Object.keys(vars)
    .sort((a, b) => a.localeCompare(b))
    .map((name) => ({ name, description: 'Environment variable', kind: 'env' as const }));
}

/**
 * Replace the open `{{…` token at the caret with `{{name}}`.
 * Returns the new text plus where the caret should land (after `}}`).
 */
export function applySuggestion(
  text: string,
  caret: number,
  open: OpenPlaceholder,
  name: string,
): { text: string; caret: number } {
  // Swallow a `}}` that already follows the caret so accepting a
  // suggestion inside `{{foo}}` doesn't leave `{{bar}}}}` behind.
  const tail = text.slice(caret).startsWith('}}') ? text.slice(caret + 2) : text.slice(caret);
  const head = text.slice(0, open.start);
  const inserted = `{{${name}}}`;
  return { text: head + inserted + tail, caret: head.length + inserted.length };
}
