//! stdio↔HTTP MCP bridge.
//!
//! [`run_stdio_bridge`] lets stdio-only MCP clients (e.g. Claude Desktop) reach
//! a daemon-hosted MCP endpoint. It connects to the daemon over the streamable
//! HTTP transport and serves a transparent MCP server over stdio that forwards
//! every request to the daemon. This is the single code path for stdio clients;
//! there is no embedded vault engine in the bridge.

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, Peer, ServerHandler, ServiceError, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ServerInfo,
    },
    service::{RequestContext, RoleClient, RoleServer},
    transport::{IntoTransport, StreamableHttpClientTransport, stdio},
};

/// Transparent MCP server (stdio side) that forwards every request to a
/// connected daemon peer (HTTP side).
struct BridgeServer {
    remote: Peer<RoleClient>,
    server_info: ServerInfo,
}

fn forward_error(error: ServiceError) -> McpError {
    McpError::internal_error(format!("notesmith daemon error: {error}"), None)
}

impl ServerHandler for BridgeServer {
    fn get_info(&self) -> ServerInfo {
        self.server_info.clone()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.remote.list_tools(request).await.map_err(forward_error)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.remote.call_tool(request).await.map_err(forward_error)
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        self.remote
            .list_resources(request)
            .await
            .map_err(forward_error)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        self.remote
            .read_resource(request)
            .await
            .map_err(forward_error)
    }
}

/// Connect to a daemon MCP endpoint over HTTP and bridge it to stdio.
///
/// `endpoint` is the full per-vault URL, e.g. `http://127.0.0.1:27183/mcp/work`
/// or `http://host/mcp-ro/work`. Blocks until the stdio client disconnects.
pub async fn run_stdio_bridge(endpoint: impl Into<Arc<str>>) -> anyhow::Result<()> {
    run_bridge(endpoint, stdio()).await
}

/// Connect to a daemon MCP endpoint over HTTP and bridge it to an arbitrary
/// server-side transport.
///
/// [`run_stdio_bridge`] is the production entry point (the transport is stdio);
/// this generic form exists so the proxy can be driven over an in-memory
/// transport in tests. Blocks until the client side disconnects.
pub async fn run_bridge<T, E, A>(
    endpoint: impl Into<Arc<str>>,
    server_transport: T,
) -> anyhow::Result<()>
where
    T: IntoTransport<RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let transport = StreamableHttpClientTransport::from_uri(endpoint);
    let client = ().serve(transport).await?;

    let server_info = client.peer_info().cloned().ok_or_else(|| {
        anyhow::anyhow!("daemon did not return server info during MCP initialization")
    })?;
    let remote = client.peer().clone();

    let service = BridgeServer {
        remote,
        server_info,
    }
    .serve(server_transport)
    .await?;

    service.waiting().await?;
    client.cancel().await?;
    Ok(())
}
