//! Reporters for `argos run`.
//!
//! Reporters consume a [`RunReportAggregate`] — the collected outcomes
//! across every iteration in the run — and render them to a target
//! string. Output destination (stdout vs a file path) is handled by
//! the caller in `main.rs`.
//!
//! Reporter formats:
//!   - `json`  → see [`json::render`] (`schema: "argos.run.v1"`).
//!   - `junit` → JUnit-XML for GitHub / GitLab CI.
//!   - `html`  → self-contained single-file report.
//!   - `console` (always-on, printed live per iteration) is rendered
//!     by `runner::print_report`, not here.

pub mod html;
pub mod json;
pub mod junit;

use crate::runner::RunReport;

/// Identifier of a structured reporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReporterFormat {
    Json,
    Junit,
    Html,
}

impl ReporterFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "junit" | "xml" => Some(Self::Junit),
            "html" => Some(Self::Html),
            _ => None,
        }
    }

    pub fn render(self, agg: &RunReportAggregate) -> String {
        match self {
            Self::Json => json::render(agg),
            Self::Junit => junit::render(agg),
            Self::Html => html::render(agg),
        }
    }
}

/// Cross-iteration aggregate handed to reporters. One `iterations[i]`
/// per iteration, in order; `iterations` has length 1 for a plain run
/// (no `--iteration-data`).
#[derive(Debug)]
pub struct RunReportAggregate {
    pub workspace_name: String,
    pub started_at_unix_ms: u128,
    pub iterations: Vec<IterationReport>,
}

#[derive(Debug)]
pub struct IterationReport {
    /// 0-based index.
    pub index: usize,
    pub report: RunReport,
}

impl RunReportAggregate {
    pub fn total_requests(&self) -> usize {
        self.iterations.iter().map(|i| i.report.total()).sum()
    }

    pub fn failed_requests(&self) -> usize {
        self.iterations.iter().map(|i| i.report.failed()).sum()
    }

    pub fn total_tests(&self) -> usize {
        self.iterations
            .iter()
            .flat_map(|i| i.report.requests.iter())
            .map(|r| r.tests.len())
            .sum()
    }

    pub fn failed_tests(&self) -> usize {
        self.iterations
            .iter()
            .flat_map(|i| i.report.requests.iter())
            .flat_map(|r| r.tests.iter())
            .filter(|t| !t.passed)
            .count()
    }

    pub fn duration_ms(&self) -> u64 {
        self.iterations
            .iter()
            .flat_map(|i| i.report.requests.iter())
            .map(|r| r.duration_ms)
            .sum()
    }
}

