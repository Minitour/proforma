use rmcp::{
    handler::server::wrapper::Parameters, model::*, tool, tool_handler, tool_router,
    ErrorData as McpError, RoleServer, ServerHandler,
};

use proforma::tools::*;

#[derive(Clone)]
pub struct ProformaHandler {
    /// Preferred protocol version advertised in `initialize` (stdio vs HTTP differ).
    protocol_version: ProtocolVersion,
}

impl ProformaHandler {
    pub fn for_stdio() -> Self {
        Self {
            protocol_version: ProtocolVersion::V_2025_11_25,
        }
    }

    pub fn for_http() -> Self {
        Self {
            protocol_version: ProtocolVersion::V_2026_07_28,
        }
    }
}

/// Map tool failures to MCP tool-level errors (`isError: true`) so clients
/// surface the message instead of an opaque JSON-RPC error.
fn map_tool(result: ToolResult) -> Result<CallToolResult, McpError> {
    match result {
        Ok(text) => Ok(CallToolResult::success(vec![ContentBlock::text(text)])),
        Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(
            e.to_string(),
        )])),
    }
}

const REFERENCE_URI: &str = "proforma://reference";

#[tool_router]
impl ProformaHandler {
    #[tool(
        description = "Return the PROforma language reference: file structure, task kinds, decisions, data items, expression operators, built-in functions and editing conventions. Read this before writing PROforma text."
    )]
    async fn proforma_reference(
        &self,
        Parameters(p): Parameters<LanguageReference>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Overview of a guideline: metadata, the task tree from the root plan (with scheduling and decision candidates), data items and functions. Start here."
    )]
    async fn outline(
        &self,
        Parameters(p): Parameters<Outline>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "List top-level components (plans, decisions, actions, enquiries, tasks, data items, functions) with kind, caption, type and line. Optional kind and substring filters."
    )]
    async fn list_components(
        &self,
        Parameters(p): Parameters<ListComponents>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Get a component or nested element as PROforma text (default) or JSON. Selectors: `name`, `plan/component:x`, `decision/candidate:c`, `decision/candidate:c/argument:1`, `enquiry/source:s`, `data/warning_condition:1`."
    )]
    async fn get_component(
        &self,
        Parameters(p): Parameters<GetComponent>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Find every reference to a task, data item, function or component name: plan components, scheduling constraints, sources and expressions."
    )]
    async fn find_references(
        &self,
        Parameters(p): Parameters<FindReferences>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Return a plan's graph as JSON: components (task instances) with kind, caption, layout, flags and the components they wait for."
    )]
    async fn plan_graph(
        &self,
        Parameters(p): Parameters<PlanGraph>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Case-insensitive search across names, captions, descriptions and expressions."
    )]
    async fn search(&self, Parameters(p): Parameters<Search>) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Validate a guideline like Deontics Composer does (syntax, Composer compatibility, reference resolution, verifier type checks with the engine's own messages) plus extra checks (scheduling cycles, recursive plans, unknown candidates/tasks, crash-prone constructs). Codes: PF-L syntax, PF-K Composer compatibility (fix with format), PF-C/R/PP/G engine load checks, PF-V verifier, PF-X extra."
    )]
    async fn lint(&self, Parameters(p): Parameters<Lint>) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Parse and type-check an expression (optionally against a .pf file): reports syntax errors, the inferred type, what each name resolves to, and verifier warnings. Use before setting preconditions, recommendations, arguments or derivations."
    )]
    async fn check_expression(
        &self,
        Parameters(p): Parameters<CheckExpression>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Export a plan as a Mermaid flowchart: node shape per task kind (action rounded, enquiry diamond, decision circle, plan stadium/subgraph, task trapezoid), scheduling arrows, decision branch labels inferred from result_of preconditions, optional candidate and data nodes."
    )]
    async fn to_mermaid(
        &self,
        Parameters(p): Parameters<ToMermaid>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Export the whole guideline as query-friendly JSON (format proforma-export/1): metadata, every task with its attributes (preconditions, components with scheduling, candidates with arguments, sources), data items and functions; metaprops parsed into key/value maps; plan structure resolved (used_by, paths from the root, reachable). Use it to answer questions about a guideline or to check it with scripts. Optional output path."
    )]
    async fn export_json(
        &self,
        Parameters(p): Parameters<ExportJson>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Create a new, valid PROforma 1.9 guideline file (engine_version, metadata, metaprops and an empty root plan)."
    )]
    async fn create_guideline(
        &self,
        Parameters(p): Parameters<CreateGuideline>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Add or replace whole top-level components written as PROforma text (e.g. `decision :: 'triage'; ... end decision.`). Several blocks may be given; a block replaces the existing component with the same name. Note: adding a task definition does not put it in a plan (use add_component or add_task)."
    )]
    async fn upsert_component(
        &self,
        Parameters(p): Parameters<UpsertComponent>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Create a task (plan, decision, action, enquiry or generic task) inside a plan: adds the task definition and a component with the same name, optionally scheduled after other components and with a caption/precondition."
    )]
    async fn add_task(
        &self,
        Parameters(p): Parameters<AddTask>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Add a component to a plan that reuses an existing task definition, optionally after other components and with extra component attributes (optional, cycles, param_value)."
    )]
    async fn add_component(
        &self,
        Parameters(p): Parameters<AddComponent>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Remove a top-level task, data item or function. Cascades by default: plan components using it, constraints on those components and sources reading a removed data item are removed; remaining expression references are reported."
    )]
    async fn remove_component(
        &self,
        Parameters(p): Parameters<RemoveComponent>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Rename a task, data item or function and update all references (task_definition, scheduling constraints, sources, expressions). Use `plan/component:x` to rename only a component instance."
    )]
    async fn rename(&self, Parameters(p): Parameters<Rename>) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Set one attribute on a component or nested element (replaces an existing value). The value is PROforma text after `attribute ::` (captions/descriptions may be plain text). E.g. precondition='age >= 18', recommendation='netsupport() > 0', caption='Assess patient', choice_mode='multiple', optional='yes', cycle_repeat='1 days'."
    )]
    async fn set_attribute(
        &self,
        Parameters(p): Parameters<SetAttribute>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Insert attributes or nested elements written as PROforma text into a component or nested element, e.g. a candidate with arguments into a decision, a source into an enquiry, a warning_condition into a data item."
    )]
    async fn add_attributes(
        &self,
        Parameters(p): Parameters<AddAttributes>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Remove all attributes with the given keyword from a component or nested element."
    )]
    async fn remove_attribute(
        &self,
        Parameters(p): Parameters<RemoveAttribute>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Remove a nested element by selector: a plan component (`plan/component:x`, also drops constraints on it), candidate, source, argument or warning condition."
    )]
    async fn remove_element(
        &self,
        Parameters(p): Parameters<RemoveElement>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Add (or remove) a scheduling constraint: `component` starts only after `after` has completed. Both must be components of the same plan; self-loops, duplicates and cycles are rejected."
    )]
    async fn schedule_constraint(
        &self,
        Parameters(p): Parameters<ScheduleConstraint>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Add a candidate (option) to a decision with caption, recommendation rule (default `netsupport() > 0`) and priority."
    )]
    async fn add_candidate(
        &self,
        Parameters(p): Parameters<AddCandidate>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Add an argument (rule) to a decision candidate: support for/against/confirming/excluding (symbolic decisions) or a numeric weight (numeric decisions), a condition expression, optional name and caption."
    )]
    async fn add_argument(
        &self,
        Parameters(p): Parameters<AddArgument>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Add a data source to an enquiry (or decision): the data item whose value is requested. Can create the data item with create_type."
    )]
    async fn add_source(
        &self,
        Parameters(p): Parameters<AddSource>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Add a data item with type, caption and optional range, default value, derivation and unit."
    )]
    async fn add_data_item(
        &self,
        Parameters(p): Parameters<AddDataItem>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(description = "Set guideline metadata: title, desc, author, version, cdate, mdate.")]
    async fn set_metadata(
        &self,
        Parameters(p): Parameters<SetMetadata>,
    ) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }

    #[tool(
        description = "Rewrite a file in Deontics Composer's canonical layout (CRLF, 5-space indent, fixed attribute order) and upgrade legacy files to engine_version 1.9. With check=true only reports whether it is formatted."
    )]
    async fn format(&self, Parameters(p): Parameters<Format>) -> Result<CallToolResult, McpError> {
        map_tool(p.run())
    }
}

#[tool_handler]
impl ServerHandler for ProformaHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("proforma", env!("CARGO_PKG_VERSION"))
                .with_title("PROforma MCP Server")
                .with_description(
                    "Read, edit, validate and diagram PROforma (.pf) clinical guideline files.",
                ),
        )
        .with_protocol_version(self.protocol_version.clone())
        .with_instructions(
            "Tools for PROforma clinical guideline files (.pf, Deontics Composer format). \
             Every tool takes the file path; the file on disk is the source of truth. \
             Start with `outline`, read `proforma_reference` before writing PROforma text, \
             use `check_expression` to validate expressions, prefer the structured edit tools \
             (add_task, add_candidate, add_argument, add_source, add_data_item, schedule_constraint, \
             set_attribute), and run `lint` after edits. `to_mermaid` renders plans as diagrams."
                .to_string(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![Resource::new(REFERENCE_URI, "reference".to_string())
                .with_description(
                    "PROforma language reference for authoring guidelines".to_string(),
                )
                .with_mime_type("text/markdown".to_string())],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _ctx: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        if request.uri != REFERENCE_URI {
            return Err(McpError::invalid_params(
                format!("Unknown resource URI: {}", request.uri),
                None,
            ));
        }
        let text = proforma::reference::get(None);
        Ok(ReadResourceResult::new(vec![ResourceContents::text(text, request.uri.clone())]).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_protocol_versions() {
        assert_eq!(
            ProformaHandler::for_stdio().get_info().protocol_version,
            ProtocolVersion::V_2025_11_25
        );
        assert_eq!(
            ProformaHandler::for_http().get_info().protocol_version,
            ProtocolVersion::V_2026_07_28
        );
    }

    #[test]
    fn errors_are_tool_errors() {
        let r = map_tool(Err(ToolError("boom".into()))).unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    #[test]
    fn registers_all_tools() {
        let tools = ProformaHandler::tool_router().list_all();
        assert_eq!(
            tools.len(),
            28,
            "{:?}",
            tools.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
    }
}
