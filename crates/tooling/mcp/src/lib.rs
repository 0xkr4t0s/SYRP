//! `syrp-mcp`: an MCP server that exposes the `cargo syrp` closed loop (master plan §13.3).
//!
//! Every tool builds the same [`Cmd`] the CLI parses, runs it through
//! [`syrp_cli::cli::dispatch`] and wraps the outcome in the same [`Envelope`]. The envelope's
//! JSON is returned as the tool's structured content, so an agent sees exactly what
//! `cargo syrp <command> --json` prints. Only `duration_ms` can differ between runs.
//!
//! Tools: `doctor`, `check`, `test`, `explain`, `docs_search`, plus the project-internal
//! `wp_show`, `wp_ready` and `review_checklist` behind the `internal` feature (on by default).
//! Resources: `syrp://errors/{code}` and, with `internal`, `syrp://wp/{id}`.
//!
//! ```no_run
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! syrp_mcp::serve_stdio(syrp_mcp::SyrpServer::new(None)).await?;
//! # Ok(()) }
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Implementation, JsonObject, ListResourceTemplatesResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
    ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, schemars, tool, tool_handler};
use serde::Deserialize;
use syrp_cli::cli::{Cli, Cmd, DocsCmd};
use syrp_cli::envelope::Envelope;

#[cfg(feature = "internal")]
mod internal;

/// URI template of the error-catalogue resource. `{code}` is an `SYRP-*` code.
///
/// ```
/// assert_eq!(syrp_mcp::ERRORS_TEMPLATE, "syrp://errors/{code}");
/// ```
pub const ERRORS_TEMPLATE: &str = "syrp://errors/{code}";

/// URI template of the work-package resource (feature `internal`). `{id}` is a WP id.
///
/// ```
/// assert_eq!(syrp_mcp::WP_TEMPLATE, "syrp://wp/{id}");
/// ```
pub const WP_TEMPLATE: &str = "syrp://wp/{id}";

/// MIME type of every resource body: the envelope is JSON.
const JSON_MIME: &str = "application/json";

/// The MCP server. Cheap to clone; every clone shares nothing mutable.
///
/// ```
/// let server = syrp_mcp::SyrpServer::new(None);
/// assert!(server.root().is_none());
/// ```
#[derive(Debug, Clone)]
pub struct SyrpServer {
    root: Option<PathBuf>,
    tool_router: ToolRouter<Self>,
}

/// Parameters of the `check` tool.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CheckParams {
    /// Skip fmt and clippy (no compilation), like `cargo syrp check --lints-only`.
    #[serde(default)]
    lints_only: bool,
}

/// Parameters of the `test` tool.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct TestParams {
    /// Limit to one package, like `cargo syrp test --crate <package>`.
    #[serde(default, rename = "crate")]
    krate: Option<String>,
}

/// Parameters of the `explain` tool.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct ExplainParams {
    /// The code, for example SYRP-WP-0005.
    #[serde(default)]
    code: Option<String>,
    /// List every code instead of explaining one.
    #[serde(default)]
    list: bool,
}

/// Parameters of the `docs_search` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DocsSearchParams {
    /// Words to look for.
    query: String,
    /// Maximum hits (default 10, as in the CLI).
    #[serde(default)]
    limit: Option<usize>,
}

impl SyrpServer {
    /// A server for the repository at `root`, or, when `None`, the repository found by
    /// searching upwards from the working directory (as `cargo syrp` does without `--root`).
    ///
    /// ```
    /// let server = syrp_mcp::SyrpServer::new(Some("/path/to/syrp".into()));
    /// assert_eq!(server.root(), Some(std::path::Path::new("/path/to/syrp")));
    /// ```
    pub fn new(root: Option<PathBuf>) -> Self {
        #[allow(unused_mut)]
        let mut tool_router = Self::public_tools();
        #[cfg(feature = "internal")]
        {
            tool_router += Self::internal_tools();
        }
        SyrpServer { root, tool_router }
    }

    /// The repository root passed to [`SyrpServer::new`], if any.
    ///
    /// ```
    /// assert!(syrp_mcp::SyrpServer::new(None).root().is_none());
    /// ```
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Names of the tools this build serves, sorted.
    ///
    /// ```
    /// let names = syrp_mcp::SyrpServer::new(None).tool_names();
    /// assert!(names.contains(&"check".to_string()));
    /// ```
    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        names.sort();
        names
    }

    /// Runs one CLI command through [`syrp_cli::cli::dispatch`] and wraps it in the CLI's
    /// envelope, exactly as the `cargo syrp` binary does. Blocking: it may run cargo.
    ///
    /// ```
    /// use syrp_cli::cli::Cmd;
    /// let env = syrp_mcp::SyrpServer::new(None).envelope(Cmd::Explain { code: None, list: true });
    /// assert!(env.ok);
    /// assert_eq!(env.command, "explain");
    /// ```
    pub fn envelope(&self, command: Cmd) -> Envelope {
        let cli = Cli {
            json: true,
            root: self.root.clone(),
            command,
        };
        let start = Instant::now();
        let outcome = syrp_cli::cli::dispatch(&cli);
        Envelope::new(&cli.command.name(), outcome, start.elapsed().as_millis())
    }

    /// [`SyrpServer::envelope`] on the blocking pool, so cargo runs never stall the transport.
    async fn run(&self, command: Cmd) -> Result<Envelope, ErrorData> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.envelope(command))
            .await
            .map_err(|e| ErrorData::internal_error(format!("command panicked: {e}"), None))
    }

    /// Runs a command and turns its envelope into a tool result.
    async fn tool(&self, command: Cmd) -> Result<CallToolResult, ErrorData> {
        Ok(tool_result(&self.run(command).await?))
    }

    /// Reads one resource: the envelope of the command behind it, as JSON text.
    async fn resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
        let command = resource_command(uri).ok_or_else(|| {
            ErrorData::resource_not_found(format!("no Syrp resource at {uri}"), None)
        })?;
        let env = self.run(command).await?;
        let value = envelope_value(&env);
        if !env.ok {
            return Err(ErrorData::resource_not_found(
                format!("{uri}: {} failed (exit {})", env.command, env.exit_code),
                Some(value),
            ));
        }
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(value.to_string(), uri).with_mime_type(JSON_MIME),
        ]))
    }
}

impl Default for SyrpServer {
    /// Same as `SyrpServer::new(None)`.
    fn default() -> Self {
        Self::new(None)
    }
}

/// The public tools: what downstream users' agents also get.
#[rmcp::tool_router(router = public_tools)]
impl SyrpServer {
    /// `cargo syrp doctor`.
    #[tool(
        description = "Diagnose the toolchain and repository (cargo syrp doctor). Run once per session.",
        output_schema = envelope_schema()
    )]
    async fn doctor(&self) -> Result<CallToolResult, ErrorData> {
        self.tool(Cmd::Doctor).await
    }

    /// `cargo syrp check`.
    #[tool(
        description = "The fast gate (cargo syrp check): crate lints, layering, size, banned crates, WP specs, fmt and clippy.",
        output_schema = envelope_schema()
    )]
    async fn check(
        &self,
        Parameters(p): Parameters<CheckParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tool(Cmd::Check {
            lints_only: p.lints_only,
        })
        .await
    }

    /// `cargo syrp test`.
    #[tool(
        description = "Run tests with a structured summary (cargo syrp test).",
        output_schema = envelope_schema()
    )]
    async fn test(
        &self,
        Parameters(p): Parameters<TestParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tool(Cmd::Test { krate: p.krate }).await
    }

    /// `cargo syrp explain`.
    #[tool(
        description = "Explain an SYRP-* error code: cause, fix, example (cargo syrp explain). Set list to list every code.",
        output_schema = envelope_schema()
    )]
    async fn explain(
        &self,
        Parameters(p): Parameters<ExplainParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tool(Cmd::Explain {
            code: p.code,
            list: p.list,
        })
        .await
    }

    /// `cargo syrp docs search`.
    #[tool(
        description = "Search docs, error codes and work packages offline (cargo syrp docs search).",
        output_schema = envelope_schema()
    )]
    async fn docs_search(
        &self,
        Parameters(p): Parameters<DocsSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let query = p.query.split_whitespace().map(str::to_string).collect();
        self.tool(Cmd::Docs(DocsCmd::Search {
            query,
            limit: p.limit.unwrap_or(10),
        }))
        .await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SyrpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "The Syrp closed loop. Every tool returns the cargo syrp JSON envelope \
             (schema syrp.envelope/v1) as structured content; run `explain` on any SYRP-* code.",
        )
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(
            resource_templates(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        self.resource(&request.uri).await.map(Into::into)
    }
}

/// The resource templates this build serves.
///
/// ```
/// let t = syrp_mcp::resource_templates();
/// assert_eq!(t[0].uri_template, syrp_mcp::ERRORS_TEMPLATE);
/// ```
pub fn resource_templates() -> Vec<ResourceTemplate> {
    #[allow(unused_mut)]
    let mut out = vec![
        ResourceTemplate::new(ERRORS_TEMPLATE, "error")
            .with_description(
                "The catalogue entry for one SYRP-* code (cargo syrp explain <code>).",
            )
            .with_mime_type(JSON_MIME),
    ];
    #[cfg(feature = "internal")]
    out.push(
        ResourceTemplate::new(WP_TEMPLATE, "work-package")
            .with_description(
                "One work package: definition, spec, owned paths (cargo syrp wp show <id>).",
            )
            .with_mime_type(JSON_MIME),
    );
    out
}

/// Maps a resource URI to the CLI command behind it, or `None` if no template matches.
///
/// ```
/// use syrp_cli::cli::Cmd;
/// let cmd = syrp_mcp::resource_command("syrp://errors/SYRP-CLI-0001").unwrap();
/// assert!(matches!(cmd, Cmd::Explain { code: Some(c), list: false } if c == "SYRP-CLI-0001"));
/// assert!(syrp_mcp::resource_command("syrp://nothing/here").is_none());
/// ```
pub fn resource_command(uri: &str) -> Option<Cmd> {
    let segment = |prefix: &str| {
        uri.strip_prefix(prefix)
            .filter(|s| !s.is_empty() && !s.contains('/'))
            .map(str::to_string)
    };
    if let Some(code) = segment("syrp://errors/") {
        return Some(Cmd::Explain {
            code: Some(code),
            list: false,
        });
    }
    #[cfg(feature = "internal")]
    if let Some(id) = segment("syrp://wp/") {
        return Some(Cmd::Wp(syrp_cli::cli::WpCmd::Show { id }));
    }
    None
}

/// The envelope as a JSON value: the bytes `cargo syrp --json` prints.
fn envelope_value(env: &Envelope) -> serde_json::Value {
    serde_json::to_value(env).expect("the envelope always serialises")
}

/// A tool result carrying the envelope as structured content. A failed command is a tool
/// error (`isError`), with the envelope still attached so the agent can read the diagnostics.
fn tool_result(env: &Envelope) -> CallToolResult {
    let value = envelope_value(env);
    if env.ok {
        CallToolResult::structured(value)
    } else {
        CallToolResult::structured_error(value)
    }
}

/// The published envelope JSON Schema, used as every tool's output schema.
///
/// ```
/// let schema = syrp_mcp::envelope_schema();
/// assert_eq!(schema["type"], "object");
/// ```
pub fn envelope_schema() -> Arc<JsonObject> {
    Arc::new(
        serde_json::from_str(syrp_cli::ENVELOPE_JSON_SCHEMA)
            .expect("the envelope schema is a JSON object"),
    )
}

/// Serves `server` over stdin and stdout until the client disconnects.
///
/// ```no_run
/// # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
/// syrp_mcp::serve_stdio(syrp_mcp::SyrpServer::default()).await?;
/// # Ok(()) }
/// ```
pub async fn serve_stdio(server: SyrpServer) -> Result<(), Box<dyn std::error::Error>> {
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
