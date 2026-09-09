/**
 * Pure input parsing for the Git console panel.
 *
 * Ported verbatim from `my-git` (Graft), whose console does the same job
 * against the same downstream — `git` itself, with no shell in between.
 */

export type ShellSplit = { ok: true; args: string[] } | { ok: false; error: string };

/**
 * Minimal POSIX-ish shell-word splitting for the console's input box:
 * single/double quotes group a run of spaces into one argument, backslash
 * escapes the next character outside single quotes. Not a full shell — no
 * globbing, no variable expansion, no pipes: there is no shell downstream of
 * this, only `git`, so those were never meaningful here.
 *
 * A leading literal `git` token is dropped, so `status` and `git status` run
 * the same command.
 */
export function splitShellArgs(input: string): ShellSplit {
  const args: string[] = [];
  let current = '';
  let hasToken = false;
  let quote: '"' | "'" | null = null;

  for (let i = 0; i < input.length; i++) {
    const c = input[i]!;
    if (quote === "'") {
      if (c === "'") quote = null;
      else current += c;
      continue;
    }
    if (quote === '"') {
      if (c === '"') quote = null;
      else if (
        c === '\\' &&
        i + 1 < input.length &&
        (input[i + 1] === '"' || input[i + 1] === '\\')
      ) {
        current += input[++i];
      } else current += c;
      continue;
    }
    if (c === "'" || c === '"') {
      quote = c;
      hasToken = true;
      continue;
    }
    if (c === '\\' && i + 1 < input.length) {
      current += input[++i];
      hasToken = true;
      continue;
    }
    if (/\s/.test(c)) {
      if (hasToken) {
        args.push(current);
        current = '';
        hasToken = false;
      }
      continue;
    }
    current += c;
    hasToken = true;
  }

  if (quote) return { ok: false, error: `unmatched ${quote} quote` };
  if (hasToken) args.push(current);
  if (args[0] === 'git') args.shift();
  return { ok: true, args };
}
