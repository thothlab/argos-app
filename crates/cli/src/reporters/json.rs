//! JSON reporter — explicit schema, not derived from internal types.
//!
//! Downstream CI pipelines parse this; keep the schema stable. Bump
//! the `schema` discriminator when fields change shape.

use serde_json::{json, Value};

use super::RunReportAggregate;
use crate::runner::RequestOutcome;

/// Render the aggregate as pretty-printed JSON with a trailing
/// newline. Schema discriminator: `argos.run.v1`.
pub fn render(agg: &RunReportAggregate) -> String {
    let doc = json!({
        "schema": "argos.run.v1",
        "workspace": agg.workspace_name,
        "started_at_unix_ms": agg.started_at_unix_ms.to_string(),
        "summary": {
            "iterations": agg.iterations.len(),
            "requests_total": agg.total_requests(),
            "requests_failed": agg.failed_requests(),
            "tests_total": agg.total_tests(),
            "tests_failed": agg.failed_tests(),
            "duration_ms": agg.duration_ms(),
        },
        "iterations": agg
            .iterations
            .iter()
            .map(|it| json!({
                "index": it.index,
                "requests": it.report.requests.iter().map(request_value).collect::<Vec<_>>()
            }))
            .collect::<Vec<_>>(),
    });
    let mut s = serde_json::to_string_pretty(&doc).expect("json serialisation");
    s.push('\n');
    s
}

fn request_value(r: &RequestOutcome) -> Value {
    json!({
        "name": r.name,
        "method": r.method,
        "url": r.url,
        "status": r.status,
        "duration_ms": r.duration_ms,
        "ok": r.is_ok(),
        "error": r.error,
        "tests": r.tests.iter().map(|t| json!({
            "name": t.name,
            "passed": t.passed,
            "message": t.message,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reporters::IterationReport;
    use crate::runner::{RequestOutcome, RunReport};
    use argos_scripting::TestResult;
    use serde_json::Value;

    fn agg_with(reqs: Vec<RequestOutcome>) -> RunReportAggregate {
        RunReportAggregate {
            workspace_name: "ws".into(),
            started_at_unix_ms: 1_700_000_000_000,
            iterations: vec![IterationReport {
                index: 0,
                report: RunReport { requests: reqs },
            }],
        }
    }

    #[test]
    fn renders_passing_request_with_schema_discriminator() {
        let req = RequestOutcome {
            name: "List".into(),
            method: "GET".into(),
            url: "https://x/u".into(),
            status: Some(200),
            duration_ms: 12,
            tests: vec![TestResult {
                name: "ok".into(),
                passed: true,
                message: String::new(),
            }],
            error: None,
        };
        let s = render(&agg_with(vec![req]));
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["schema"], "argos.run.v1");
        assert_eq!(v["summary"]["requests_total"], 1);
        assert_eq!(v["summary"]["requests_failed"], 0);
        assert_eq!(v["summary"]["tests_total"], 1);
        assert_eq!(v["iterations"][0]["requests"][0]["ok"], true);
        assert_eq!(v["iterations"][0]["requests"][0]["status"], 200);
    }

    #[test]
    fn renders_transport_failure_as_null_status_and_error_message() {
        let req = RequestOutcome {
            name: "Bad".into(),
            method: "GET".into(),
            url: "https://x".into(),
            status: None,
            duration_ms: 4,
            tests: vec![],
            error: Some("dns blew up".into()),
        };
        let s = render(&agg_with(vec![req]));
        let v: Value = serde_json::from_str(&s).unwrap();
        assert!(v["iterations"][0]["requests"][0]["status"].is_null());
        assert_eq!(v["iterations"][0]["requests"][0]["error"], "dns blew up");
        assert_eq!(v["summary"]["requests_failed"], 1);
    }
}
