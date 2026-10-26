//! Project-internal tools (feature `internal`): work packages and review helpers that only
//! agents working on the Syrp repository need (master plan §13.3).

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, schemars, tool};
use serde::Deserialize;
use syrp_cli::cli::{Cmd, ReviewCmd, WpCmd};

use crate::{SyrpServer, envelope_schema};

/// Parameters of the `wp_show` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct WpShowParams {
    /// Work-package id, for example WP-0.2-10.
    id: String,
}

/// Parameters of the `wp_ready` tool.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(crate) struct WpReadyParams {
    /// Extra WP ids to treat as done.
    #[serde(default)]
    done: Vec<String>,
}

#[rmcp::tool_router(router = internal_tools, vis = "pub(crate)")]
impl SyrpServer {
    /// `cargo syrp wp show`.
    #[tool(
        description = "Everything an agent needs to start a WP: definition, spec, owned paths, dependencies (cargo syrp wp show).",
        output_schema = envelope_schema()
    )]
    async fn wp_show(
        &self,
        Parameters(p): Parameters<WpShowParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tool(Cmd::Wp(WpCmd::Show { id: p.id })).await
    }

    /// `cargo syrp wp ready`.
    #[tool(
        description = "WPs whose dependencies are done, from the spec files alone (cargo syrp wp ready).",
        output_schema = envelope_schema()
    )]
    async fn wp_ready(
        &self,
        Parameters(p): Parameters<WpReadyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tool(Cmd::Wp(WpCmd::Ready { done: p.done })).await
    }

    /// `cargo syrp review checklist`.
    #[tool(
        description = "The checklist a review must answer (cargo syrp review checklist).",
        output_schema = envelope_schema()
    )]
    async fn review_checklist(&self) -> Result<CallToolResult, ErrorData> {
        self.tool(Cmd::Review(ReviewCmd::Checklist)).await
    }
}
