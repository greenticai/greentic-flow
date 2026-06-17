use crate::error::{FlowError, FlowErrorLocation, Result};
use serde_json::Value;

/// The op-key / component token that marks a node as an MCP tool invocation.
///
/// In the authoring YGTC this is the literal operation key (`mcp:`), and after
/// lowering it becomes the runtime node's `component` string. It is a valid
/// `greentic_types::ComponentId` (only `[A-Za-z0-9._-]`), so it survives
/// pack/runtime load — server, tool, arguments and output live in the node
/// PAYLOAD, never encoded into the key.
pub const MCP_COMPONENT: &str = "mcp";

/// Classification of a node's component type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A node backed by an adapter operation in the form `<namespace>.<adapter>.<operation>`.
    Adapter {
        namespace: String,
        adapter: String,
        operation: String,
    },
    /// A node that invokes an MCP tool.
    ///
    /// The op key / component token is the literal `mcp`. `server_id` references
    /// an admin-configured tenant MCP server and `tool` is the raw MCP tool
    /// name; both are read from the node PAYLOAD, not from the key string. This
    /// classification is purely structural: the flow compiler never probes the
    /// server or the tool.
    Mcp { server_id: String, tool: String },
    /// Any other node type that does not match the adapter or MCP convention.
    Builtin(String),
}

/// Classify a node's op-key / component token into [`NodeKind`].
///
/// The MCP convention keys on the exact token `mcp`. Because `server` and
/// `tool` now live in the payload (not the key), this classifier returns an
/// [`NodeKind::Mcp`] with empty `server_id`/`tool`; callers populate those from
/// the payload via [`mcp_server_and_tool`] during load/validate.
///
/// MCP takes precedence over the adapter convention. Everything else falls back
/// to the adapter split or [`NodeKind::Builtin`].
pub fn classify_node_type(node_type: &str) -> NodeKind {
    if node_type == MCP_COMPONENT {
        return NodeKind::Mcp {
            server_id: String::new(),
            tool: String::new(),
        };
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

/// Extract `(server, tool)` from a validated MCP node payload.
///
/// Returns the raw, non-empty `server` and `tool` strings. Use this after
/// [`validate_mcp_config`] has confirmed they are present and well-formed.
pub fn mcp_server_and_tool(config: &Value) -> Option<(String, String)> {
    let server = config.get("server").and_then(Value::as_str)?;
    let tool = config.get("tool").and_then(Value::as_str)?;
    Some((server.to_string(), tool.to_string()))
}

/// Structurally validate the `config` payload of an MCP node.
///
/// Per the MCP node contract the payload carries:
/// - `server`: a non-empty string admin server id (required),
/// - `tool`: a non-empty string MCP tool name (required),
/// - `arguments`: an object mapping flow state to MCP tool input (optional),
/// - `output`: a string flow-state key to bind the tool result under (optional).
///
/// This check is offline-only: it never contacts the MCP server. Missing or
/// empty `server`/`tool`, a non-object `arguments`, or a non-string `output`
/// are all rejected with [`FlowError::McpConfig`].
pub fn validate_mcp_config(node_id: &str, config: &Value) -> Result<()> {
    let location = || FlowErrorLocation::at_path(format!("nodes.{node_id}"));
    let reject = |message: &str| {
        Err(FlowError::McpConfig {
            node_id: node_id.to_string(),
            message: message.to_string(),
            location: location(),
        })
    };

    // A non-object config (e.g. a scalar or array under the mcp key) cannot
    // carry the documented server/tool/arguments/output shape.
    let Some(obj) = config.as_object() else {
        return reject("MCP node config must be an object");
    };

    match obj.get("server").and_then(Value::as_str) {
        Some(server) if !server.is_empty() => {}
        _ => return reject("MCP node config 'server' must be a non-empty string"),
    }

    match obj.get("tool").and_then(Value::as_str) {
        Some(tool) if !tool.is_empty() => {}
        _ => return reject("MCP node config 'tool' must be a non-empty string"),
    }

    if let Some(arguments) = obj.get("arguments")
        && !arguments.is_object()
    {
        return reject("MCP node config 'arguments' must be an object");
    }

    if let Some(output) = obj.get("output")
        && !output.is_string()
    {
        return reject("MCP node config 'output' must be a string");
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
            classify_node_type("mcp"),
            NodeKind::Mcp {
                server_id: String::new(),
                tool: String::new(),
            }
        );
    }

    #[test]
    fn mcp_server_and_tool_read_from_payload() {
        let config = json!({ "server": "github", "tool": "get_issue" });
        assert_eq!(
            mcp_server_and_tool(&config),
            Some(("github".to_string(), "get_issue".to_string()))
        );
    }

    #[test]
    fn legacy_mcp_prefix_is_no_longer_special() {
        // The old `mcp:<server>/<tool>` key form is just a Builtin now: it is
        // not a valid ComponentId and carries no special meaning.
        assert_eq!(
            classify_node_type("mcp:github/get_issue"),
            NodeKind::Builtin("mcp:github/get_issue".to_string())
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
            "server": "github",
            "tool": "get_issue",
            "arguments": { "owner": "{{ flow.owner }}", "number": "{{ input.issue_number }}" },
            "output": "issue"
        });
        validate_mcp_config("lookup_issue", &config).expect("valid config");
    }

    #[test]
    fn validates_mcp_config_allows_missing_optional_keys() {
        let config = json!({ "server": "github", "tool": "get_issue" });
        validate_mcp_config("lookup_issue", &config).expect("server+tool only is valid");
    }

    #[test]
    fn rejects_missing_server() {
        let config = json!({ "tool": "get_issue" });
        let err = validate_mcp_config("lookup_issue", &config).unwrap_err();
        match err {
            FlowError::McpConfig { message, .. } => assert!(message.contains("server")),
            other => panic!("expected McpConfig, got {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_server() {
        let config = json!({ "server": "", "tool": "get_issue" });
        let err = validate_mcp_config("lookup_issue", &config).unwrap_err();
        assert!(matches!(err, FlowError::McpConfig { .. }));
    }

    #[test]
    fn rejects_missing_tool() {
        let config = json!({ "server": "github" });
        let err = validate_mcp_config("lookup_issue", &config).unwrap_err();
        match err {
            FlowError::McpConfig { message, .. } => assert!(message.contains("tool")),
            other => panic!("expected McpConfig, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_object_arguments() {
        let config =
            json!({ "server": "github", "tool": "get_issue", "arguments": "not-an-object" });
        let err = validate_mcp_config("lookup_issue", &config).unwrap_err();
        assert!(matches!(err, FlowError::McpConfig { .. }));
    }

    #[test]
    fn rejects_non_string_output() {
        let config = json!({ "server": "github", "tool": "get_issue", "output": 42 });
        let err = validate_mcp_config("lookup_issue", &config).unwrap_err();
        assert!(matches!(err, FlowError::McpConfig { .. }));
    }

    #[test]
    fn rejects_non_object_config() {
        let err = validate_mcp_config("lookup_issue", &json!("scalar")).unwrap_err();
        assert!(matches!(err, FlowError::McpConfig { .. }));
    }
}
