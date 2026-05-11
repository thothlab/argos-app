//! JUnit XML reporter.
//!
//! Targets the [testmoapp / Surefire dialect][1] that GitHub Actions
//! and GitLab CI consume. Hand-rolled — pulling a crate for ~40 lines
//! of `<tag>` writing isn't a fair trade.
//!
//! Layout:
//!
//! ```xml
//! <testsuites tests=".." failures=".." errors="..">
//!   <testsuite name="iteration 1" tests=".." failures="..">
//!     <testcase classname="<request name>" name="<test name>" time="0.123">
//!       <failure message="..">..</failure>   <!-- only on fail -->
//!     </testcase>
//!   </testsuite>
//! </testsuites>
//! ```
//!
//! `time` is **seconds, decimal**. A request with no tests still emits
//! one `<testcase>` (named `request`) so CI shows it ran; a transport
//! failure becomes one `<testcase>` with an `<error>` child.
//!
//! [1]: https://github.com/testmoapp/junitxml

use std::fmt::Write as _;

use super::RunReportAggregate;
use crate::runner::RequestOutcome;

pub fn render(agg: &RunReportAggregate) -> String {
    let mut out = String::with_capacity(512);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let total_tests: usize = agg
        .iterations
        .iter()
        .map(|i| testcase_count(&i.report.requests))
        .sum();
    let total_failures: usize = agg
        .iterations
        .iter()
        .map(|i| failure_count(&i.report.requests))
        .sum();
    let total_errors: usize = agg
        .iterations
        .iter()
        .map(|i| error_count(&i.report.requests))
        .sum();
    let total_time = seconds(agg.duration_ms());
    let _ = writeln!(
        out,
        "<testsuites name=\"argos run\" tests=\"{total_tests}\" failures=\"{total_failures}\" errors=\"{total_errors}\" time=\"{total_time}\">"
    );

    for it in &agg.iterations {
        let tests = testcase_count(&it.report.requests);
        let fails = failure_count(&it.report.requests);
        let errs = error_count(&it.report.requests);
        let time = seconds(
            it.report
                .requests
                .iter()
                .map(|r| r.duration_ms)
                .sum::<u64>(),
        );
        let suite_name = format!("iteration {}", it.index + 1);
        let _ = writeln!(
            out,
            "  <testsuite name=\"{name}\" tests=\"{tests}\" failures=\"{fails}\" errors=\"{errs}\" time=\"{time}\">",
            name = escape_attr(&suite_name),
        );
        for r in &it.report.requests {
            emit_request(&mut out, r);
        }
        out.push_str("  </testsuite>\n");
    }

    out.push_str("</testsuites>\n");
    out
}

fn emit_request(out: &mut String, r: &RequestOutcome) {
    let classname = format!("{} {}", r.method, r.url);
    if let Some(err) = &r.error {
        // Transport / pre-request failure: one testcase, one <error>.
        let _ = write!(
            out,
            "    <testcase classname=\"{cls}\" name=\"{name}\" time=\"{time}\">\n      <error message=\"{msg}\">{body}</error>\n    </testcase>\n",
            cls = escape_attr(&classname),
            name = escape_attr(if r.name.is_empty() { "request" } else { &r.name }),
            time = seconds(r.duration_ms),
            msg = escape_attr(err),
            body = escape_text(err),
        );
        return;
    }
    if r.tests.is_empty() {
        // No tests: emit a synthetic passing testcase named "request"
        // so the run is still visible in CI.
        let _ = writeln!(
            out,
            "    <testcase classname=\"{cls}\" name=\"request\" time=\"{time}\" />",
            cls = escape_attr(&classname),
            time = seconds(r.duration_ms),
        );
        return;
    }
    let per_test_ms = r.duration_ms / (r.tests.len() as u64).max(1);
    for t in &r.tests {
        if t.passed {
            let _ = writeln!(
                out,
                "    <testcase classname=\"{cls}\" name=\"{name}\" time=\"{time}\" />",
                cls = escape_attr(&classname),
                name = escape_attr(&t.name),
                time = seconds(per_test_ms),
            );
        } else {
            let _ = write!(
                out,
                "    <testcase classname=\"{cls}\" name=\"{name}\" time=\"{time}\">\n      <failure message=\"{msg}\">{body}</failure>\n    </testcase>\n",
                cls = escape_attr(&classname),
                name = escape_attr(&t.name),
                time = seconds(per_test_ms),
                msg = escape_attr(&t.message),
                body = escape_text(&t.message),
            );
        }
    }
}

fn testcase_count(reqs: &[RequestOutcome]) -> usize {
    reqs.iter()
        .map(|r| {
            if r.error.is_some() || r.tests.is_empty() {
                1
            } else {
                r.tests.len()
            }
        })
        .sum()
}

fn failure_count(reqs: &[RequestOutcome]) -> usize {
    reqs.iter()
        .map(|r| if r.error.is_some() { 0 } else { r.failing_tests() })
        .sum()
}

fn error_count(reqs: &[RequestOutcome]) -> usize {
    reqs.iter().filter(|r| r.error.is_some()).count()
}

fn seconds(ms: u64) -> String {
    let secs = (ms as f64) / 1000.0;
    // Three decimals — enough precision for CI test reporters.
    format!("{secs:.3}")
}

fn escape_attr(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            '\n' => "&#10;".to_string(),
            '\r' => "&#13;".to_string(),
            '\t' => "&#9;".to_string(),
            c if (c as u32) < 0x20 => String::new(),
            c => c.to_string(),
        })
        .collect()
}

fn escape_text(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reporters::IterationReport;
    use crate::runner::{RequestOutcome, RunReport};
    use argos_scripting::TestResult;
    use quick_xml::events::Event;
    use quick_xml::Reader;

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

    fn well_formed(xml: &str) -> Vec<(String, std::collections::HashMap<String, String>)> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let mut elements = Vec::new();
        loop {
            match reader.read_event_into(&mut buf).unwrap() {
                Event::Eof => break,
                Event::Start(e) | Event::Empty(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let mut attrs = std::collections::HashMap::new();
                    for a in e.attributes() {
                        let a = a.unwrap();
                        let k = String::from_utf8_lossy(a.key.as_ref()).into_owned();
                        let v = String::from_utf8_lossy(&a.value).into_owned();
                        attrs.insert(k, v);
                    }
                    elements.push((name, attrs));
                }
                _ => {}
            }
            buf.clear();
        }
        elements
    }

    #[test]
    fn emits_well_formed_xml_with_required_attrs() {
        let req = RequestOutcome {
            name: "List".into(),
            method: "GET".into(),
            url: "https://x/u".into(),
            status: Some(200),
            duration_ms: 1234,
            tests: vec![TestResult {
                name: "status 200".into(),
                passed: true,
                message: String::new(),
            }],
            error: None,
        };
        let s = render(&agg(vec![req]));
        let els = well_formed(&s);
        let suites = els.iter().find(|(n, _)| n == "testsuites").unwrap();
        assert_eq!(suites.1.get("tests").unwrap(), "1");
        assert_eq!(suites.1.get("failures").unwrap(), "0");
        let suite = els.iter().find(|(n, _)| n == "testsuite").unwrap();
        assert_eq!(suite.1.get("name").unwrap(), "iteration 1");
        let tc = els.iter().find(|(n, _)| n == "testcase").unwrap();
        assert_eq!(tc.1.get("name").unwrap(), "status 200");
        assert_eq!(tc.1.get("classname").unwrap(), "GET https://x/u");
        assert!(tc.1.get("time").unwrap().contains('.'));
    }

    #[test]
    fn emits_failure_child_when_test_fails() {
        let req = RequestOutcome {
            name: "X".into(),
            method: "GET".into(),
            url: "https://x".into(),
            status: Some(500),
            duration_ms: 100,
            tests: vec![TestResult {
                name: "expects 200".into(),
                passed: false,
                message: "got 500 instead of 200".into(),
            }],
            error: None,
        };
        let s = render(&agg(vec![req]));
        // Well-formedness first.
        let _ = well_formed(&s);
        assert!(s.contains("<failure"), "missing <failure>: {s}");
        assert!(s.contains("got 500 instead of 200"));
        // Summary updates.
        assert!(s.contains("failures=\"1\""));
    }

    #[test]
    fn emits_error_child_on_transport_failure() {
        let req = RequestOutcome {
            name: "X".into(),
            method: "GET".into(),
            url: "https://x".into(),
            status: None,
            duration_ms: 5,
            tests: vec![],
            error: Some("dns: <unreachable>".into()),
        };
        let s = render(&agg(vec![req]));
        let _ = well_formed(&s);
        assert!(s.contains("<error"));
        // `<` in error message must be escaped in the attribute.
        assert!(s.contains("&lt;unreachable&gt;"));
        assert!(s.contains("errors=\"1\""));
    }

    #[test]
    fn synthesises_testcase_when_no_tests_attached() {
        let req = RequestOutcome {
            name: "Ping".into(),
            method: "GET".into(),
            url: "https://x".into(),
            status: Some(204),
            duration_ms: 10,
            tests: vec![],
            error: None,
        };
        let s = render(&agg(vec![req]));
        let els = well_formed(&s);
        let suites = els.iter().find(|(n, _)| n == "testsuites").unwrap();
        assert_eq!(suites.1.get("tests").unwrap(), "1");
        let tc = els.iter().find(|(n, _)| n == "testcase").unwrap();
        assert_eq!(tc.1.get("name").unwrap(), "request");
    }
}
