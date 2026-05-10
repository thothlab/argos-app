//! Argos CLI entry point.
//!
//! T7.1 deliverable: `argos list` and `argos validate` work against an
//! opened workspace; `argos run` accepts the flags but defers execution
//! to T7.2 (collection runner).

use std::path::{Path, PathBuf};

use argos_core::format::request::RequestVariant;
use argos_core::{TreeNode, Workspace};
use clap::{Parser, Subcommand};

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
        Some(Commands::Run { path, env, bail }) => {
            tracing::info!(?path, ?env, bail, "run command (deferred to T7.2)");
            anyhow::bail!("`run` is not yet implemented (Epic E7, T7.2).");
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
