//! Argos CLI entry point.
//!
//! T7.1 added `argos list` and `argos validate`; T7.2 wires up
//! `argos run` against the collection runner in `runner.rs`.

mod iteration;
mod reporters;
mod runner;

use std::path::{Path, PathBuf};

use argos_core::format::request::RequestVariant;
use argos_core::{TreeNode, Workspace};
use clap::{Parser, Subcommand};

use reporters::{IterationReport, ReporterFormat, RunReportAggregate};
use runner::{print_report, RunOptions, RunReport};

#[derive(Parser)]
#[command(
    name = "argos",
    version,
    about = "Argos — git-native API client",
    long_about = None,
)]
struct Cli {
    /// Path to the workspace root. Defaults to the current directory.
    /// Subcommands that take their own `<path>` argument override this.
    #[arg(global = true, long, env = "ARGOS_WORKSPACE")]
    workspace: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a collection or single request. (Execution lands in T7.2.)
    Run {
        /// Path to collection folder or request file.
        path: String,
        /// Override active environment.
        #[arg(long)]
        env: Option<String>,
        /// Stop on first failure.
        #[arg(long)]
        bail: bool,
        /// Data-driven iterations. Path to a `.csv` or `.json` file —
        /// each row / object is one full pass through `path`, with row
        /// values bound as env overrides.
        #[arg(long = "iteration-data", value_name = "FILE")]
        iteration_data: Option<PathBuf>,
        /// Structured report. Repeat for multiple formats.
        /// Syntax: `<format>` (writes to stdout) or `<format>=<path>`
        /// (writes to a file). Formats: `json`, `junit`, `html`.
        #[arg(long = "reporter", value_name = "FORMAT[=PATH]")]
        reporters: Vec<String>,
    },
    /// List requests / collections in the workspace.
    List {
        /// Path to workspace root (defaults to --workspace / cwd).
        path: Option<String>,
    },
    /// Validate a workspace, collection, or request file.
    Validate {
        /// Path to validate (defaults to --workspace / cwd).
        path: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    argos_core::init_tracing();
    let cli = Cli::parse();

    match cli.command {
        None => {
            println!("argos {}", argos_core::version());
            println!("Run `argos --help` to see commands.");
        }
        Some(Commands::Run {
            path,
            env,
            bail,
            iteration_data,
            reporters,
        }) => {
            let ws_root = cli
                .workspace
                .clone()
                .or_else(|| infer_workspace_root(Path::new(&path)))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "could not infer workspace root from {path}; pass --workspace explicitly"
                    )
                })?;
            let target = Path::new(&path).to_path_buf();

            let rows = match iteration_data.as_deref() {
                Some(p) => iteration::load(p)?,
                None => Vec::new(),
            };

            let reporter_specs = parse_reporters(&reporters)?;

            let workspace_name = Workspace::open(&ws_root)
                .map(|ws| ws.manifest.name)
                .unwrap_or_else(|_| ws_root.display().to_string());
            let started = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default();
            let mut iterations: Vec<IterationReport> = Vec::new();

            let exit_failed = if rows.is_empty() {
                let report = run_command(
                    &ws_root,
                    &target,
                    env,
                    bail,
                    std::collections::HashMap::new(),
                )?;
                print_report(&report);
                let failed = report.failed() > 0;
                iterations.push(IterationReport { index: 0, report });
                failed
            } else {
                let total = rows.len();
                let mut any_failed = false;
                for (i, row) in rows.into_iter().enumerate() {
                    println!("→ Iteration {} of {total}", i + 1);
                    let report = run_command(&ws_root, &target, env.clone(), bail, row)?;
                    print_report(&report);
                    if report.failed() > 0 {
                        any_failed = true;
                    }
                    iterations.push(IterationReport { index: i, report });
                    if any_failed && bail {
                        break;
                    }
                }
                any_failed
            };

            let aggregate = RunReportAggregate {
                workspace_name,
                started_at_unix_ms: started,
                iterations,
            };
            emit_reporters(&aggregate, &reporter_specs)?;

            if exit_failed {
                std::process::exit(1);
            }
        }
        Some(Commands::List { path }) => {
            let root = resolve_root(path.as_deref(), cli.workspace.as_deref())?;
            cmd_list(&root)?;
        }
        Some(Commands::Validate { path }) => {
            let root = resolve_root(path.as_deref(), cli.workspace.as_deref())?;
            cmd_validate(&root)?;
        }
    }

    Ok(())
}

struct ReporterSpec {
    format: ReporterFormat,
    /// `None` means stdout.
    output: Option<PathBuf>,
}

fn parse_reporters(values: &[String]) -> anyhow::Result<Vec<ReporterSpec>> {
    let mut out = Vec::with_capacity(values.len());
    for raw in values {
        let (name, path) = match raw.split_once('=') {
            Some((n, p)) => (n.trim(), Some(PathBuf::from(p.trim()))),
            None => (raw.trim(), None),
        };
        let format = ReporterFormat::parse(name).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown reporter `{name}` — expected one of: json, junit, html",
            )
        })?;
        out.push(ReporterSpec {
            format,
            output: path,
        });
    }
    Ok(out)
}

fn emit_reporters(
    agg: &RunReportAggregate,
    specs: &[ReporterSpec],
) -> anyhow::Result<()> {
    for spec in specs {
        let payload = spec.format.render(agg);
        match &spec.output {
            None => {
                // Separate from the console summary with a blank line.
                println!();
                print!("{payload}");
            }
            Some(path) => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            anyhow::anyhow!("create {}: {e}", parent.display())
                        })?;
                    }
                }
                std::fs::write(path, payload)
                    .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))?;
            }
        }
    }
    Ok(())
}

fn run_command(
    workspace_root: &Path,
    target: &Path,
    env: Option<String>,
    bail: bool,
    data_row: std::collections::HashMap<String, String>,
) -> anyhow::Result<RunReport> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(runner::run(
        workspace_root,
        target,
        RunOptions {
            env_name: env,
            bail,
            data_row,
        },
    ))
}

/// Walk up from `target` looking for an `argos.yaml`; that directory
/// is the workspace root. Falls back to `None` if we hit the
/// filesystem root.
fn infer_workspace_root(target: &Path) -> Option<PathBuf> {
    let mut cursor = if target.is_file() {
        target.parent()?.to_path_buf()
    } else {
        target.to_path_buf()
    };
    loop {
        if cursor.join("argos.yaml").is_file() {
            return Some(cursor);
        }
        if !cursor.pop() {
            return None;
        }
    }
}

fn resolve_root(arg: Option<&str>, global: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = global {
        return Ok(p.to_path_buf());
    }
    Ok(std::env::current_dir()?)
}

fn cmd_list(root: &Path) -> anyhow::Result<()> {
    let ws = Workspace::open(root)
        .map_err(|e| anyhow::anyhow!("could not open workspace at {}: {e}", root.display()))?;

    println!("Workspace: {}", ws.manifest.name);
    println!("Root:      {}", ws.root.display());
    if !ws.environments.is_empty() {
        println!(
            "Envs:      {}",
            ws.environments
                .iter()
                .map(|e| e.env.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    println!();
    print_tree(&ws.tree, "");
    Ok(())
}

fn print_tree(node: &TreeNode, indent: &str) {
    match node {
        TreeNode::Folder { name, children, .. } => {
            println!("{indent}📁 {name}");
            let next_indent = format!("{indent}  ");
            for child in children {
                print_tree(child, &next_indent);
            }
        }
        TreeNode::Request { draft, .. } => {
            let RequestVariant::Rest(rest) = &draft.variant;
            println!(
                "{indent}- {method:<6} {url}  [{name}]",
                method = rest.method.as_str(),
                url = rest.url,
                name = draft.name,
            );
        }
    }
}

fn cmd_validate(root: &Path) -> anyhow::Result<()> {
    match Workspace::open(root) {
        Ok(ws) => {
            let (folders, requests) = count_nodes(&ws.tree);
            println!(
                "✓ {} valid — {} folder(s), {} request(s), {} env(s)",
                ws.manifest.name,
                folders,
                requests,
                ws.environments.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("✗ {}: {e}", root.display());
            anyhow::bail!("workspace failed validation");
        }
    }
}

fn count_nodes(node: &TreeNode) -> (usize, usize) {
    let mut folders = 0_usize;
    let mut requests = 0_usize;
    walk(node, &mut folders, &mut requests);
    (folders, requests)
}

fn walk(node: &TreeNode, folders: &mut usize, requests: &mut usize) {
    match node {
        TreeNode::Folder { children, .. } => {
            *folders += 1;
            for child in children {
                walk(child, folders, requests);
            }
        }
        TreeNode::Request { .. } => *requests += 1,
    }
}
