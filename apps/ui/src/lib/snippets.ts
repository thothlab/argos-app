/**
 * Ready-made script snippets surfaced in the request editor as a
 * one-click "insert" menu. Each snippet is keyed by where it makes
 * sense (`pre` = pre-request, `tests` = post-response).
 *
 * Snippets are intentionally short: they should read like documentation,
 * not full helpers. Trailing newline keeps the cursor on a fresh line
 * after insertion.
 */

export type SnippetKind = 'pre' | 'tests';

export type Snippet = {
  id: string;
  kind: SnippetKind;
  label: string;
  description: string;
  body: string;
};

export const SNIPPETS: Snippet[] = [
  {
    id: 'pre-bearer-from-env',
    kind: 'pre',
    label: 'Set Bearer from env',
    description: 'Reads {{token}} from the active environment and sets the Authorization header.',
    body: [
      "const token = bru.env.get('token');",
      'if (token) {',
      "  bru.req.setHeader('Authorization', 'Bearer ' + token);",
      '}',
      '',
    ].join('\n'),
  },
  {
    id: 'pre-trace-id',
    kind: 'pre',
    label: 'Add X-Trace-Id header',
    description: 'Generates a UUID via the Web Crypto API and stamps it on the request.',
    body: [
      'const traceId = crypto.randomUUID();',
      "bru.req.setHeader('X-Trace-Id', traceId);",
      "bru.info('trace=', traceId);",
      '',
    ].join('\n'),
  },
  {
    id: 'pre-timestamp',
    kind: 'pre',
    label: 'Add timestamp header',
    description: 'Sets X-Sent-At to the current ISO timestamp for server-side debugging.',
    body: [
      'const now = new Date().toISOString();',
      "bru.req.setHeader('X-Sent-At', now);",
      '',
    ].join('\n'),
  },
  {
    id: 'pre-json-body',
    kind: 'pre',
    label: 'Replace JSON body',
    description: 'Overwrites the request body with a fresh JSON object.',
    body: [
      'bru.req.setJsonBody({',
      "  greeting: 'hi from script',",
      '  at: Date.now(),',
      '});',
      '',
    ].join('\n'),
  },
  {
    id: 'pre-fail-if-no-env',
    kind: 'pre',
    label: 'Abort if env var missing',
    description: 'Throws a labelled error if a required env value is absent.',
    body: [
      "if (!bru.env.get('token')) {",
      "  bru.fail('token env var is required for this request');",
      '}',
      '',
    ].join('\n'),
  },
  {
    id: 'tests-status-2xx',
    kind: 'tests',
    label: 'Status is 2xx',
    description: 'Asserts the response status is in the success range.',
    body: [
      "bru.test('Status is 2xx', () => {",
      '  const s = bru.res.status;',
      '  bru.expect(s >= 200 && s < 300).toBeTruthy();',
      '});',
      '',
    ].join('\n'),
  },
  {
    id: 'tests-content-type-json',
    kind: 'tests',
    label: 'Content-Type is JSON',
    description: 'Verifies the response advertises an application/json body.',
    body: [
      "bru.test('Content-Type is JSON', () => {",
      "  const ct = bru.res.getHeader('content-type') || '';",
      "  bru.expect(ct).toContain('application/json');",
      '});',
      '',
    ].join('\n'),
  },
  {
    id: 'tests-save-id-to-env',
    kind: 'tests',
    label: 'Save id from response to env',
    description: 'Parses the JSON body and stores response.id under env.lastId.',
    body: [
      'const data = bru.res.json();',
      "bru.test('Has id', () => {",
      "  bru.expect(data.id).toBeTruthy();",
      '});',
      "bru.env.set('lastId', String(data.id));",
      '',
    ].join('\n'),
  },
  {
    id: 'tests-field-equals',
    kind: 'tests',
    label: 'Field equals expected',
    description: 'Boilerplate for asserting a specific JSON field matches.',
    body: [
      'const data = bru.res.json();',
      "bru.test('field matches', () => {",
      "  bru.expect(data.status).toBe('ok');",
      '});',
      '',
    ].join('\n'),
  },
  {
    id: 'pre-pm-bearer',
    kind: 'pre',
    label: 'pm: Set Bearer from env',
    description: 'Postman-style: read pm.environment and stamp Authorization.',
    body: [
      "const token = pm.environment.get('token');",
      'if (token) {',
      "  pm.request.headers.upsert({ key: 'Authorization', value: 'Bearer ' + token });",
      '}',
      '',
    ].join('\n'),
  },
  {
    id: 'tests-pm-status-200',
    kind: 'tests',
    label: 'pm: Status code is 200',
    description: 'Postman-style status assertion using pm.test + pm.expect.',
    body: [
      "pm.test('Status code is 200', function () {",
      '  pm.expect(pm.response.code).to.equal(200);',
      '});',
      '',
    ].join('\n'),
  },
  {
    id: 'tests-pm-save-token',
    kind: 'tests',
    label: 'pm: Save access_token',
    description: 'Postman snippet — store access_token from response into env.',
    body: [
      'const data = pm.response.json();',
      "pm.test('login returned token', function () {",
      "  pm.expect(data).to.have.property('access_token');",
      '});',
      "pm.environment.set('token', data.access_token);",
      '',
    ].join('\n'),
  },
];

export function snippetsFor(kind: SnippetKind): Snippet[] {
  return SNIPPETS.filter((s) => s.kind === kind);
}
