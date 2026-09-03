//! Making the published schemas portable.
//!
//! `schemars` writes an optional field as `"type": ["string", "null"]`. That is
//! legal JSON Schema and it is also, in practice, a problem: a good number of
//! MCP clients read `type` as a single string and will either reject a tool
//! outright or quietly drop the constraint. Neither is acceptable for a
//! contract whose whole point is that a client can trust it — a dropped
//! constraint on `expected_revision` is precisely the field this server exists
//! to make non-negotiable.
//!
//! So every schema is walked once, at start-up, and the array form is rewritten
//! into the branch form:
//!
//! ```text
//! {"type": ["string", "null"]}
//!     ↓
//! {"anyOf": [{"type": "string"}, {"type": "null"}]}
//! ```
//!
//! Same meaning, and one every client reads the same way. Nothing else about
//! the schema is touched: this rewrites how a type is *spelled* and never what
//! is required, what is allowed, or what anything is called. The rest of the
//! keywords on the node — a description, a format, an enumeration — are kept
//! where they are, beside the `anyOf`, exactly as they were beside the `type`.
//!
//! The alternative, making every optional field absent instead of nullable, is
//! a different contract and a worse one: "the note has no `updated_at`" and
//! "this answer does not mention `updated_at`" are not the same statement, and
//! a client that has to tell them apart deserves to be told which it got.

use rmcp::model::JsonObject;
use serde_json::Value;
use std::sync::Arc;

/// The same schema, with every array-valued `type` rewritten as `anyOf`.
pub fn portable(schema: &JsonObject) -> Arc<JsonObject> {
    let mut value = Value::Object(schema.clone());
    rewrite(&mut value);
    match value {
        Value::Object(object) => Arc::new(object),
        // Unreachable: an object goes in and `rewrite` never changes what kind
        // of value a node is. Answering with the original rather than
        // panicking, because a schema is not worth a crash.
        _ => Arc::new(schema.clone()),
    }
}

fn rewrite(node: &mut Value) {
    match node {
        Value::Object(object) => {
            if let Some(Value::Array(types)) = object.get("type") {
                // A single-element array is the same thing written oddly and
                // is collapsed rather than wrapped in a pointless `anyOf`.
                let branches: Vec<Value> = types
                    .iter()
                    .map(|kind| {
                        let mut branch = serde_json::Map::new();
                        branch.insert("type".to_string(), kind.clone());
                        Value::Object(branch)
                    })
                    .collect();
                match branches.len() {
                    0 => {}
                    1 => {
                        if let Some(Value::Object(single)) = branches.into_iter().next() {
                            if let Some(kind) = single.get("type") {
                                object.insert("type".to_string(), kind.clone());
                            }
                        }
                    }
                    _ => {
                        object.remove("type");
                        object.insert("anyOf".to_string(), Value::Array(branches));
                    }
                }
            }
            for child in object.values_mut() {
                rewrite(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(value: Value) -> JsonObject {
        match value {
            Value::Object(object) => object,
            other => panic!("{other}"),
        }
    }

    #[test]
    fn a_nullable_type_becomes_two_branches_and_keeps_its_neighbours() {
        let schema = object(json!({
            "type": "object",
            "properties": {
                "revision": {
                    "type": ["string", "null"],
                    "description": "sixty-four hexadecimal characters",
                },
            },
            "required": ["revision"],
        }));
        let rewritten = portable(&schema);
        let property = &rewritten["properties"]["revision"];
        assert_eq!(
            property["anyOf"],
            json!([{ "type": "string" }, { "type": "null" }])
        );
        assert!(property.get("type").is_none());
        assert_eq!(
            property["description"], "sixty-four hexadecimal characters",
            "the rewrite lost a neighbouring keyword"
        );
        // And nothing else moved.
        assert_eq!(rewritten["type"], "object");
        assert_eq!(rewritten["required"], json!(["revision"]));
    }

    #[test]
    fn a_plain_type_is_left_exactly_as_it_was() {
        let schema = object(json!({ "type": "string", "minLength": 64 }));
        let rewritten = portable(&schema);
        assert_eq!(rewritten["type"], "string");
        assert_eq!(rewritten["minLength"], 64);
        assert!(rewritten.get("anyOf").is_none());
    }

    #[test]
    fn a_one_element_array_is_collapsed_rather_than_wrapped() {
        let schema = object(json!({ "type": ["string"] }));
        let rewritten = portable(&schema);
        assert_eq!(rewritten["type"], "string");
        assert!(rewritten.get("anyOf").is_none());
    }

    #[test]
    fn nested_definitions_and_arrays_are_reached() {
        let schema = object(json!({
            "$defs": {
                "Note": {
                    "properties": { "label": { "type": ["string", "null"] } },
                },
            },
            "anyOf": [{ "properties": { "x": { "type": ["integer", "null"] } } }],
        }));
        let rewritten = portable(&schema);
        assert_eq!(
            rewritten["$defs"]["Note"]["properties"]["label"]["anyOf"],
            json!([{ "type": "string" }, { "type": "null" }])
        );
        assert_eq!(
            rewritten["anyOf"][0]["properties"]["x"]["anyOf"],
            json!([{ "type": "integer" }, { "type": "null" }])
        );
    }
}
