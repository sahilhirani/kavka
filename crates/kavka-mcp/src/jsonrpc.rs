//! JSON-RPC 2.0, framed the way MCP's stdio transport frames it: one JSON
//! object per line, UTF-8, no embedded newlines, nothing else on the stream.
//!
//! This module is deliberately total and deliberately dumb — it decides whether
//! a line is a well-formed call, a well-formed notification, or neither, and it
//! never looks at what the method means. That is [`crate::server`]'s job, and
//! keeping the split lets the whole framing layer be tested with strings.
//!
//! **Stdout belongs to the protocol.** Anything this process wants to say to a
//! human goes to stderr; a stray `println!` corrupts the stream and the client
//! reports a parse error it cannot explain.

use serde_json::{json, Value};

/// The only version this speaks, and the only one MCP uses.
pub const VERSION: &str = "2.0";

// The five standard error codes. MCP adds none of its own: a tool that fails is
// a *successful* `tools/call` carrying `isError: true` (see
// `server::tool_result`), because a model has to be able to read the failure
// and try something else rather than have its transport report a fault.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// One well-formed incoming message.
#[derive(Debug, Clone, PartialEq)]
pub struct Incoming {
    /// `None` for a notification — the one thing that decides whether a reply
    /// is owed.
    pub id: Option<Value>,
    pub method: String,
    /// `Value::Null` when the message carried no `params`. Every MCP method
    /// takes an object or nothing, so anything else is rejected in [`parse`].
    pub params: Value,
}

impl Incoming {
    /// The `params` field as an object, or an empty one. Callers that need a
    /// specific key report their own error, with their own wording.
    pub fn args(&self) -> &Value {
        &self.params
    }
}

/// What one line of input turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    /// Has an id: a reply is owed.
    Call(Incoming),
    /// No id: a reply is forbidden.
    Notification(Incoming),
    /// Not a request at all. Carries the response that must go back — JSON-RPC
    /// answers a parse error with `"id": null`, which is the one place a null
    /// id is legal.
    Malformed { code: i64, message: String },
}

/// Parses one line of the stream.
///
/// Rejections are specific on purpose: "id must not be null" and "batching was
/// removed in MCP 2025-06-18" are things a client author can act on, and
/// `-32600 Invalid Request` alone is not.
pub fn parse(line: &str) -> Parsed {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(e) => {
            return Parsed::Malformed {
                code: PARSE_ERROR,
                message: format!("not valid JSON: {e}"),
            }
        }
    };

    let object = match &value {
        Value::Object(object) => object,
        Value::Array(_) => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: "JSON-RPC batching was removed in MCP revision 2025-06-18; send one \
                          request object per line"
                    .into(),
            }
        }
        _ => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: "a JSON-RPC message is an object".into(),
            }
        }
    };

    match object.get("jsonrpc") {
        Some(Value::String(v)) if v == VERSION => {}
        Some(other) => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: format!("\"jsonrpc\" must be \"{VERSION}\", got {other}"),
            }
        }
        None => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: format!("missing the \"jsonrpc\": \"{VERSION}\" field"),
            }
        }
    }

    let method = match object.get("method") {
        Some(Value::String(method)) => method.clone(),
        Some(_) => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: "\"method\" must be a string".into(),
            }
        }
        None => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: "missing \"method\"".into(),
            }
        }
    };

    let params = match object.get("params") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::Object(_)) => object["params"].clone(),
        Some(_) => {
            return Parsed::Malformed {
                code: INVALID_PARAMS,
                message: format!("\"params\" must be an object for {method}"),
            }
        }
    };

    // A request with `"id": null` is what a client sends when it has confused a
    // call with a notification. MCP forbids it outright, and answering it would
    // mean sending a response nobody can match to a request.
    let id = match object.get("id") {
        None => None,
        Some(Value::Null) => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: "\"id\" must not be null — omit it for a notification".into(),
            }
        }
        Some(id @ (Value::String(_) | Value::Number(_))) => Some(id.clone()),
        Some(_) => {
            return Parsed::Malformed {
                code: INVALID_REQUEST,
                message: "\"id\" must be a string or a number".into(),
            }
        }
    };

    let message = Incoming { id, method, params };
    if message.id.is_some() {
        Parsed::Call(message)
    } else {
        Parsed::Notification(message)
    }
}

/// A successful response to `id`.
pub fn result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": VERSION, "id": id, "result": result })
}

/// An error response. `id` is `None` only for a message too malformed to carry
/// one, which JSON-RPC answers with a null id.
pub fn error(id: Option<&Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": VERSION,
        "id": id.cloned().unwrap_or(Value::Null),
        "error": { "code": code, "message": message.into() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(line: &str) -> Incoming {
        match parse(line) {
            Parsed::Call(message) => message,
            other => panic!("expected a call, got {other:?}"),
        }
    }

    fn malformed(line: &str) -> (i64, String) {
        match parse(line) {
            Parsed::Malformed { code, message } => (code, message),
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn a_call_carries_its_id_method_and_params() {
        let message = call(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"a":1}}"#);
        assert_eq!(message.id, Some(json!(1)));
        assert_eq!(message.method, "tools/list");
        assert_eq!(message.args(), &json!({"a": 1}));
    }

    #[test]
    fn a_string_id_is_a_call_too() {
        assert_eq!(
            call(r#"{"jsonrpc":"2.0","id":"abc","method":"ping"}"#).id,
            Some(json!("abc"))
        );
    }

    #[test]
    fn absent_params_read_as_null_not_as_a_failure() {
        // `tools/list` and `ping` are routinely sent with no params at all.
        assert_eq!(
            call(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).params,
            Value::Null
        );
        assert_eq!(
            call(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":null}"#).params,
            Value::Null
        );
    }

    #[test]
    fn a_message_without_an_id_is_a_notification() {
        let parsed = parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        match parsed {
            Parsed::Notification(message) => {
                assert_eq!(message.method, "notifications/initialized")
            }
            other => panic!("expected a notification, got {other:?}"),
        }
    }

    #[test]
    fn broken_json_is_a_parse_error() {
        let (code, message) = malformed("{not json");
        assert_eq!(code, PARSE_ERROR);
        assert!(message.contains("not valid JSON"), "{message}");
    }

    #[test]
    fn a_batch_is_refused_by_name() {
        let (code, message) = malformed(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#);
        assert_eq!(code, INVALID_REQUEST);
        // The client author has to learn *why*, not just that it failed.
        assert!(message.contains("2025-06-18"), "{message}");
    }

    #[test]
    fn the_version_field_is_checked() {
        assert_eq!(malformed(r#"{"id":1,"method":"ping"}"#).0, INVALID_REQUEST);
        assert_eq!(
            malformed(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#).0,
            INVALID_REQUEST
        );
    }

    #[test]
    fn a_null_id_is_refused_rather_than_treated_as_a_notification() {
        let (code, message) = malformed(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#);
        assert_eq!(code, INVALID_REQUEST);
        assert!(message.contains("omit it for a notification"), "{message}");
    }

    #[test]
    fn a_missing_or_non_string_method_is_refused() {
        assert_eq!(malformed(r#"{"jsonrpc":"2.0","id":1}"#).0, INVALID_REQUEST);
        assert_eq!(
            malformed(r#"{"jsonrpc":"2.0","id":1,"method":7}"#).0,
            INVALID_REQUEST
        );
    }

    #[test]
    fn non_object_params_are_an_invalid_params_error() {
        let (code, message) = malformed(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":[1]}"#);
        assert_eq!(code, INVALID_PARAMS);
        assert!(message.contains("must be an object"), "{message}");
    }

    #[test]
    fn responses_carry_the_envelope_and_the_id() {
        assert_eq!(
            result(&json!(7), json!({"ok": true})),
            json!({"jsonrpc":"2.0","id":7,"result":{"ok":true}})
        );
        assert_eq!(
            error(Some(&json!("x")), METHOD_NOT_FOUND, "nope"),
            json!({"jsonrpc":"2.0","id":"x","error":{"code":-32601,"message":"nope"}})
        );
        // The one legal null id: a message too broken to have carried one.
        assert_eq!(error(None, PARSE_ERROR, "bad")["id"], Value::Null);
    }

    /// The transport is line-delimited, so a serialized response containing a
    /// literal newline would split into two frames and desynchronize the
    /// stream. serde_json escapes them; this is the tripwire that says so.
    #[test]
    fn a_serialized_response_never_contains_a_raw_newline() {
        let line = serde_json::to_string(&result(
            &json!(1),
            json!({ "text": "two\nlines\r\nand a tab\t" }),
        ))
        .unwrap();
        assert!(!line.contains('\n'), "{line}");
        assert!(!line.contains('\r'), "{line}");
    }
}
