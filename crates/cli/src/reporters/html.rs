//! HTML reporter — single self-contained file, no JS.
//!
//! Goal: download the artifact from CI, open it in a browser, see what
//! failed. Layout is a static table of iterations / requests / tests
//! with red/green badges and `<details>` for failure messages. Inline
//! CSS only, no external assets — works offline, no CSP issues.

use std::fmt::Write as _;

use super::RunReportAggregate;
use crate::runner::RequestOutcome;

pub fn render(agg: &RunReportAggregate) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str(HEADER);
    let _ = writeln!(
        out,
        "<h1>argos run — <span class=\"badge {summary_class}\">{passed}/{total} requests</span></h1>",
        summary_class = if agg.failed_requests() == 0 { "ok" } else { "fail" },
        passed = agg.total_requests() - agg.failed_requests(),
        total = agg.total_requests(),
    );
    let _ = writeln!(
        out,
        "<p class=\"summary\">Workspace: <code>{ws}</code> · iterations: {iters} · tests: {tp}/{tt} passed · duration: {dur}ms</p>",
        ws = escape(&agg.workspace_name),
        iters = agg.iterations.len(),
        tp = agg.total_tests() - agg.failed_tests(),
        tt = agg.total_tests(),
        dur = agg.duration_ms(),
    );

    for it in &agg.iterations {
        let _ = write!(
            out,
            "<h2>Iteration {n}</h2>\n<table><thead><tr><th></th><th>Method</th><th>URL</th><th>Status</th><th>Time</th><th>Tests</th></tr></thead><tbody>\n",
            n = it.index + 1
        );
        for r in &it.report.requests {
            emit_row(&mut out, r);
        }
        out.push_str("</tbody></table>\n");
    }

    out.push_str(FOOTER);
    out
}

fn emit_row(out: &mut String, r: &RequestOutcome) {
    let ok = r.is_ok();
    let badge = if ok {
        "<span class=\"badge ok\">PASS</span>"
    } else {
        "<span class=\"badge fail\">FAIL</span>"
    };
    let status = r
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into());
    let tests_summary = if r.tests.is_empty() {
        "(no tests)".to_string()
    } else {
        let failed = r.failing_tests();
        let passed = r.tests.len() - failed;
        format!("{passed}/{} passed", r.tests.len())
    };
    let _ = writeln!(
        out,
        "<tr class=\"{cls}\"><td>{badge}</td><td><code>{m}</code></td><td><code>{u}</code></td><td>{st}</td><td>{dur}ms</td><td>{ts}</td></tr>",
        cls = if ok { "row-ok" } else { "row-fail" },
        m = escape(&r.method),
        u = escape(&r.url),
        st = escape(&status),
        dur = r.duration_ms,
        ts = escape(&tests_summary),
    );
    if let Some(err) = &r.error {
        let _ = writeln!(
            out,
            "<tr class=\"detail\"><td colspan=\"6\"><details open><summary>Transport error</summary><pre>{}</pre></details></td></tr>",
            escape(err),
        );
    }
    for t in &r.tests {
        if !t.passed {
            let _ = writeln!(
                out,
                "<tr class=\"detail\"><td colspan=\"6\"><details open><summary>✗ {n}</summary><pre>{m}</pre></details></td></tr>",
                n = escape(&t.name),
                m = escape(&t.message),
            );
        }
    }
}

fn escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

const HEADER: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>argos run report</title>
<style>
  body { font: 14px/1.4 -apple-system, BlinkMacSystemFont, system-ui, sans-serif; max-width: 1100px; margin: 2rem auto; padding: 0 1rem; color: #1c1c1f; }
  h1 { font-size: 22px; margin-bottom: 0.25rem; }
  h2 { font-size: 16px; margin-top: 1.5rem; border-bottom: 1px solid #e5e5ea; padding-bottom: 0.25rem; }
  .summary { color: #6b6b75; margin-top: 0; }
  table { border-collapse: collapse; width: 100%; margin-top: 0.5rem; font-size: 13px; }
  th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #f0f0f4; vertical-align: top; }
  th { font-weight: 600; background: #f7f7fa; }
  code { font: 12px/1 ui-monospace, SFMono-Regular, monospace; }
  .badge { display: inline-block; padding: 2px 8px; border-radius: 6px; font-size: 11px; font-weight: 700; letter-spacing: 0.04em; }
  .badge.ok   { background: #e6f7ec; color: #167a35; }
  .badge.fail { background: #fde7ea; color: #b3261e; }
  .row-fail { background: #fff5f6; }
  .detail pre { background: #1c1c1f; color: #f7f7fa; padding: 8px; border-radius: 6px; overflow-x: auto; font-size: 12px; margin: 0; }
  details summary { cursor: pointer; color: #b3261e; font-weight: 600; }
  @media (prefers-color-scheme: dark) {
    body { background: #1c1c1f; color: #f7f7fa; }
    th { background: #2a2a2e; }
    th, td { border-bottom-color: #2a2a2e; }
    .badge.ok { background: rgba(38, 175, 90, 0.15); color: #7ad29a; }
    .badge.fail { background: rgba(220, 78, 78, 0.18); color: #f29b9b; }
    .row-fail { background: rgba(220, 78, 78, 0.08); }
  }
</style>
</head>
<body>
"#;

const FOOTER: &str = "</body></html>\n";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reporters::IterationReport;
    use crate::runner::{RequestOutcome, RunReport};
    use argos_scripting::TestResult;

    fn agg(reqs: Vec<RequestOutcome>) -> RunReportAggregate {
        RunReportAggregate {
            workspace_name: "ws".into(),
            started_at_unix_ms: 0,
            iterations: vec![IterationReport {
                index: 0,
                report: RunReport { requests: reqs },
            }],
        }
    }

    #[test]
    fn includes_url_and_failure_message_when_a_test_fails() {
        let req = RequestOutcome {
            name: "X".into(),
            method: "GET".into(),
            url: "https://api.example.com/x".into(),
            status: Some(500),
            duration_ms: 12,
            tests: vec![TestResult {
                name: "expects 200".into(),
                passed: false,
                message: "got 500".into(),
            }],
            error: None,
        };
        let s = render(&agg(vec![req]));
        assert!(s.starts_with("<!DOCTYPE html>"));
        assert!(s.contains("https://api.example.com/x"));
        assert!(s.contains("got 500"));
        assert!(s.contains("class=\"badge fail\""));
        // No external assets, no JS.
        assert!(!s.contains("<script"));
        assert!(!s.contains("https://cdn"));
    }

    #[test]
    fn escapes_user_supplied_strings() {
        let req = RequestOutcome {
            name: "X".into(),
            method: "GET".into(),
            url: "https://x?q=<script>".into(),
            status: None,
            duration_ms: 1,
            tests: vec![],
            error: Some("oops & <hi>".into()),
        };
        let s = render(&agg(vec![req]));
        assert!(s.contains("&lt;script&gt;"));
        assert!(s.contains("oops &amp; &lt;hi&gt;"));
        assert!(!s.contains("<script>"));
    }
}
