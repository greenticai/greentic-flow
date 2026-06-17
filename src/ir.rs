use crate::error::{FlowError, FlowErrorLocation, Result};
use serde_json::Value;

/// Prefix that marks a node component string as an MCP tool invocation.
pub const MCP_PREFIX: &str = "mcp:";

/// Classification of a node's component type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A node backed by an adapter operation in the form `<namespace>.<adapter>.<operation>`.
    Adapter {
        namespace: String,
        adapter: String,
        operation: String,
    },
    /// A node that invokes an MCP tool, written as `mcp:<server_id>/<tool_name>`.
    ///
    /// `server_id` references an admin-configured tenant MCP server and
    /// `tool` is the raw MCP tool name. This classification is purely
    /// structural: the flow compiler never probes the server or the tool.
    Mcp { server_id: String, tool: String },
    /// Any other node type that does not match the adapter or MCP convention.
    Builtin(String),
}

/// Classify a component string into [`NodeKind`].
///
/// MCP nodes take precedence over the adapter convention: a string starting
/// with `mcp:` is split into `server_id` (everything up to the first `/`)
/// and `tool` (everything after it). A malformed MCP string with no `/`, an
/// empty server id, or an empty tool name falls back to [`NodeKind::Builtin`]
/// so that structural validation can surface a precise error rather than this
/// classifier silently inventing one.
pub fn classify_node_type(node_type: &str) -> NodeKind {
    if let Some(rest) = node_type.strip_prefix(MCP_PREFIX) {
        if let Some((server_id, tool)) = rest.split_once('/')
            && !server_id.is_empty()
            && !tool.is_empty()
        {
            return NodeKind::Mcp {
                server_id: server_id.to_string(),
                tool: tool.to_string(),
            };
        }
        // Malformed `mcp:` string: keep it as Builtin so callers can reject it
        // with a structural error instead of guessing a server/tool split.
        return NodeKind::Builtin(node_type.to_string());
    }

    let parts = node_type.split('.').collect::<Vec<_>>();
    if parts.len() >= 3 {
        let namespace = parts[0].to_string();
        let adapter = parts[1].to_string();
        let operation = parts[2..].join(".");
        NodeKind::Adapter {
            namespace,
            adapter,
            operation,
        }
    } else {
        NodeKind::Builtin(node_type.to_string())
    }
}

/// Structurally validate the `config` payload of an MCP node.
///
/// Per the MCP node contract the payload may carry:
/// - `arguments`: an object mapping flow state to MCP tool input (optional),
/// - `output`: a string flow-state key to bind the tool result under (optional).
///
/// This check is offline-only: it never contacts the MCP server. It rejects a
/// non-object `arguments` and a non-string `output`. Missing keys are allowed.
pub fn validate_mcp_config(node_id: &str, config: &Value) -> Result<()> {
    let location = || FlowErrorLocation::at_path(format!("nodes.{node_id}"));

    // A non-object config (e.g. a scalar or array under the mcp key) cannot
    // carry the documented `arguments`/`output` shape.
    let Some(obj) = config.as_object() else {
        return Err(FlowError::McpConfig {
            node_id: node_id.to_string(),
            message: "MCP node config must be an object".to_string(),
            location: location(),
        });
    };

    if let Some(arguments) = obj.get("arguments")
        && !arguments.is_object()
    {
        return Err(FlowError::McpConfig {
            node_id: node_id.to_string(),
            message: "MCP node config 'arguments' must be an object".to_string(),
            location: location(),
        });
    }

    if let Some(output) = obj.get("output")
        && !output.is_string()
    {
        return Err(FlowError::McpConfig {
            node_id: node_id.to_string(),
            message: "MCP node config 'output' must be a string".to_string(),
            location: location(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_mcp_node() {
        assert_eq!(
            classify_node_type("mcp:github/get_issue"),
            NodeKind::Mcp {
                server_id: "github".to_string(),
                tool: "get_issue".to_string(),
            }
        );
    }

    #[test]
    fn mcp_tool_name_may_contain_slashes() {
        // Only the FIRST '/' separates server id from tool name.
        assert_eq!(
            classify_node_type("mcp:github/issues/get"),
            NodeKind::Mcp {
                server_id: "github".to_string(),
                tool: "issues/get".to_string(),
            }
        );
    }

    #[test]
    fn mcp_without_slash_falls_back_to_builtin() {
        // No '/' means we cannot split server/tool: classify as Builtin so a
        // structural error can be raised downstream.
        assert_eq!(
            classify_node_type("mcp:github"),
            NodeKind::Builtin("mcp:github".to_string())
        );
    }

    #[test]
    fn mcp_with_empty_segments_falls_back_to_builtin() {
        assert_eq!(
            classify_node_type("mcp:/get_issue"),
            NodeKind::Builtin("mcp:/get_issue".to_string())
        );
        assert_eq!(
            classify_node_type("mcp:github/"),
            NodeKind::Builtin("mcp:github/".to_string())
        );
    }

    #[test]
    fn classifies_adapter_and_builtin_unchanged() {
        assert_eq!(
            classify_node_type("weather.api.forecast"),
            NodeKind::Adapter {
                namespace: "weather".to_string(),
                adapter: "api".to_string(),
                operation: "forecast".to_string(),
            }
        );
        assert_eq!(
            classify_node_type("questions"),
            NodeKind::Builtin("questions".to_string())
        );
    }

    #[test]
    fn validates_mcp_config_happy_path() {
        let config = json!({
            "arguments": { "owner": "{{ flow.owner }}", "number": "{{ input.issue_number }}" },
            "output": "issue"
        });
        validate_mcp_config("lookup_issue", &config).expect("valid config");
    }

    #[test]
    fn validates_mcp_config_allows_missing_optional_keys() {
        validate_mcp_config("lookup_issue", &json!({})).expect("empty config is valid");
    }

    #[test]
    fn rejects_non_object_arguments() {
        let config = json!({ "arguments": "not-an-object" });
        let err = validate_mcp_config("lookup_issue", &config).unwrap_err();
        assert!(matches!(err, FlowError::McpConfig { .. }));
    }

    #[test]
    fn rejects_non_string_output() {
        let config = json!({ "output": 42 });
        let err = validate_mcp_config("lookup_issue", &config).unwrap_err();
        assert!(matches!(err, FlowError::McpConfig { .. }));
    }

    #[test]
    fn rejects_non_object_config() {
        let err = validate_mcp_config("lookup_issue", &json!("scalar")).unwrap_err();
        assert!(matches!(err, FlowError::McpConfig { .. }));
    }
}
