mod handler;

use clap::{Parser, Subcommand, ValueEnum};
use handler::ProformaHandler;
use proforma::tools::*;
use rmcp::{transport::stdio, ServiceExt};

#[derive(Debug, Clone, ValueEnum)]
enum Transport {
    Stdio,
    Http,
}

#[derive(Debug, Parser)]
#[command(
    name = "proforma",
    version,
    about = "MCP server and CLI for PROforma (.pf) clinical guideline files: read, edit, lint and diagram"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the MCP server (stdio or HTTP transport)
    Serve {
        /// Transport to use
        #[arg(long, default_value = "stdio", env = "PROFORMA_TRANSPORT")]
        transport: Transport,
        /// Host to bind (HTTP transport only)
        #[arg(long, default_value = "127.0.0.1", env = "PROFORMA_HOST")]
        host: String,
        /// Port to bind (HTTP transport only)
        #[arg(long, default_value_t = 8080, env = "PROFORMA_PORT")]
        port: u16,
    },
    /// Print the PROforma language reference
    Reference(LanguageReference),
    /// Show the guideline outline (task tree, data items, functions)
    Outline(Outline),
    /// List top-level components
    List(ListComponents),
    /// Print one component or nested element (selector) as PROforma text or JSON
    Get(GetComponent),
    /// Find references to a name
    Refs(FindReferences),
    /// Print a plan's components and scheduling as JSON
    PlanGraph(PlanGraph),
    /// Search names, captions and expressions
    Search(Search),
    /// Validate a guideline (exit code 1 when errors are found)
    Lint(Lint),
    /// Parse and type-check an expression
    CheckExpression(CheckExpression),
    /// Export a plan as a Mermaid flowchart
    Mermaid(ToMermaid),
    /// Export the whole guideline as query-friendly JSON
    Export(ExportJson),
    /// Create a new guideline file
    Create(CreateGuideline),
    /// Add or replace whole components from PROforma text
    Upsert(UpsertComponent),
    /// Create a task (definition + component) in a plan
    AddTask(AddTask),
    /// Add a component reusing an existing task definition
    AddComponent(AddComponent),
    /// Remove a top-level component (cascades by default)
    Remove(RemoveComponent),
    /// Rename a component and update references
    Rename(Rename),
    /// Set one attribute on a component or nested element
    Set(SetAttribute),
    /// Insert attributes / nested elements written as PROforma text
    AddAttributes(AddAttributes),
    /// Remove an attribute from a component or nested element
    Unset(RemoveAttribute),
    /// Remove a nested element (component, candidate, source, argument, warning)
    RemoveElement(RemoveElement),
    /// Add or remove a scheduling constraint between two components
    Schedule(ScheduleConstraint),
    /// Add a candidate to a decision
    AddCandidate(AddCandidate),
    /// Add an argument to a decision candidate
    AddArgument(AddArgument),
    /// Add a data source to an enquiry or decision
    AddSource(AddSource),
    /// Add a data item
    AddData(AddDataItem),
    /// Set metadata fields
    SetMetadata(SetMetadata),
    /// Rewrite a file in Composer's canonical layout
    Format(Format),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let (result, is_lint) = match args.command {
        Command::Serve {
            transport,
            host,
            port,
        } => {
            let res = match transport {
                Transport::Stdio => run_stdio().await,
                Transport::Http => run_http(host, port).await,
            };
            if let Err(e) = res {
                eprintln!("Server error: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Command::Reference(t) => (t.run(), false),
        Command::Outline(t) => (t.run(), false),
        Command::List(t) => (t.run(), false),
        Command::Get(t) => (t.run(), false),
        Command::Refs(t) => (t.run(), false),
        Command::PlanGraph(t) => (t.run(), false),
        Command::Search(t) => (t.run(), false),
        Command::Lint(t) => (t.run(), true),
        Command::CheckExpression(t) => (t.run(), false),
        Command::Mermaid(t) => (t.run(), false),
        Command::Export(t) => (t.run(), false),
        Command::Create(t) => (t.run(), false),
        Command::Upsert(t) => (t.run(), false),
        Command::AddTask(t) => (t.run(), false),
        Command::AddComponent(t) => (t.run(), false),
        Command::Remove(t) => (t.run(), false),
        Command::Rename(t) => (t.run(), false),
        Command::Set(t) => (t.run(), false),
        Command::AddAttributes(t) => (t.run(), false),
        Command::Unset(t) => (t.run(), false),
        Command::RemoveElement(t) => (t.run(), false),
        Command::Schedule(t) => (t.run(), false),
        Command::AddCandidate(t) => (t.run(), false),
        Command::AddArgument(t) => (t.run(), false),
        Command::AddSource(t) => (t.run(), false),
        Command::AddData(t) => (t.run(), false),
        Command::SetMetadata(t) => (t.run(), false),
        Command::Format(t) => (t.run(), false),
    };
    match result {
        Ok(out) => {
            print!("{}", out);
            if !out.ends_with('\n') {
                println!();
            }
            // lint: non-zero exit when errors were reported (text mode summary line)
            if is_lint && lint_has_errors(&out) {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(2);
        }
    }
}

fn lint_has_errors(out: &str) -> bool {
    if let Some(line) = out.lines().rev().find(|l| l.contains("error(s)")) {
        return !line.trim_start().starts_with("0 error(s)");
    }
    out.contains("\"severity\": \"error\"")
}

async fn run_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let server = ProformaHandler::for_stdio().serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

async fn run_http(host: String, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::transport::streamable_http_server::{
        session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true);
    let service = StreamableHttpService::new(
        || Ok(ProformaHandler::for_http()),
        std::sync::Arc::new(NeverSessionManager::default()),
        config,
    );
    let app = axum::Router::new().nest_service("/mcp", service);
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("PROforma MCP server listening on http://{}/mcp", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
