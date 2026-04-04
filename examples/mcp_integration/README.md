# Example: Model Context Protocol (MCP) Integration 🔌
DreamSwarm can connect to any MCP-compliant server to extend its toolset.

## Concept
MCP allows for a standardized way to expose tools and resources across different platforms.

## Setup
1. **Define the Server**: In your `config.toml`, add the MCP server details:
```toml
[[mcp_servers]]
name = "my_custom_mcp"
url = "http://localhost:8080"
api_key = "${MY_MCP_API_KEY}"
```
2. **Discovery**: DreamSwarm's `SwarmCoordinator` will automatically discover and register tools from the MCP server at startup.
3. **Usage**: Tools from MCP are available alongside native tools and follow the same permission model.

## Implementation Details
The `mcp_client` module (found in `src/query/mcp/`) handles the JSON-RPC communication and trait translation.
