//! The one door every tool argument comes through, and the sentence it
//! answers with when an argument is not what the schema says.
//!
//! ## What this exists to stop
//!
//! The SDK ships an argument extractor, `rmcp::handler::server::wrapper::
//! Parameters<T>`, and every tool in this server used it. It deserialises the
//! request's `arguments` into `T` and, when that fails, builds the refusal
//! like this:
//!
//! ```text
//! ErrorData::invalid_params(format!("failed to deserialize parameters: {error}"), None)
//! ```
//!
//! `error` is `serde_json`'s, and `serde_json` writes its messages for whoever
//! is debugging a payload: it quotes the value it did not understand, in full.
//! `invalid type: string "…", expected u32` carries the whole string, and
//! `unknown variant \`…\`, expected one of …` carries the whole variant. So a
//! client that sent three hundred kilobytes where a number belonged got three
//! hundred kilobytes back — measured, on the wire, in Phase 4.2R.R1:
//!
//! ```text
//! noteit_list      limit         = 300 KiB string    →  307 361 bytes answered
//! noteit_tasks_list state        = 300 KiB variant   →  307 387 bytes answered
//! noteit_context   include_tasks = 300 KiB string    →  307 367 bytes answered
//! noteit_edit      clear         = 300 KiB string    →  307 367 bytes answered
//! noteit_list      tags          = 300 KiB string    →  307 368 bytes answered
//! ```
//!
//! Phase 4.2R had closed this for every sentence the *domain* writes: a
//! `message` is a `&'static str` chosen by a `code`, so a runtime-built string
//! cannot reach the wire at all (ADR-054). It closed nothing here, because
//! this refusal happens **before** a handler in this crate runs — the
//! extractor answers the host without the tool body ever being entered. The
//! property was true of the second half of the boundary and not of the first.
//!
//! ## The property, and why it is one property rather than five fixes
//!
//! > No text derived from a client's arguments reaches the wire.
//!
//! Not "no `limit` echo", not "no `state` echo". Fixing the five fields the
//! reproduction found would have left the sixth to whoever adds it, and a rule
//! that depends on remembering is the kind of rule this repository does not
//! keep. [`SafeParameters`] is the whole class: it is the only extractor the
//! tools use, it is the only place a deserialisation error is produced, and it
//! **drops that error unread**. There is no argument, no field name and no
//! length in what it answers, because it never looks.
//!
//! ## What it deliberately does not change
//!
//! **The schema.** [`SafeParameters<T>`] publishes exactly `T`'s schema and
//! nothing else — the `JsonSchema` implementation delegates, the same way the
//! SDK's wrapper does, and a unit test below compares the two generated
//! schemas rather than trusting the sentence. A host sees the same required
//! fields, the same types and the same descriptions it saw before.
//!
//! **Which requests are accepted.** Every payload that deserialised into `T`
//! before deserialises into `T` now, through the same `serde_json::from_value`
//! over the same type. Nothing was loosened to make refusals smaller: an
//! argument of the wrong type is still refused, and `expected_revision` is
//! still required by the schema and by the type.
//!
//! **The channel.** A refusal is still the SDK's `invalid_params`, so a host
//! that already distinguished "the arguments were wrong" from "the tool
//! refused" goes on distinguishing them.

use rmcp::handler::server::common::FromContextPart;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::model::JsonObject;
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

/// What a host is told when a tool's arguments do not match its schema.
///
/// One sentence, for every tool, every field and every way of being wrong. It
/// names the schema because the schema is the answer: it is published in
/// `tools/list`, it says which fields exist and what each one is, and it is
/// the thing the caller has to go and read.
///
/// It deliberately does not say *which* field, and that is a real cost paid on
/// purpose. Naming the field would be safe on its own — a field name is a
/// short constant this crate wrote. Producing it would not be: `serde_json`
/// reports the failure as a sentence, not as a path, so the name would have to
/// be recovered by parsing that sentence, and a parser over an error message
/// that quotes its input is exactly the mechanism this module exists to
/// remove. The refusal is a constant because a constant cannot be made to
/// carry anything.
pub const INVALID_ARGUMENTS: &str = "the arguments do not match this tool's input schema";

/// A tool's arguments, deserialised into `P`, or a refusal that says nothing.
///
/// Used exactly as the SDK's own wrapper is — `SafeParameters(input):
/// SafeParameters<ListInput>` in a `#[tool]` handler — and imported in
/// [`crate::server`] under the name `Parameters`, which is the name the
/// `#[tool]` macro looks for when it works out a tool's input schema.
///
/// See [`INVALID_ARGUMENTS`] for what a failure answers, and the module
/// documentation for why it answers that and nothing else.
#[derive(Debug, Clone)]
pub struct SafeParameters<P>(pub P);

/// The schema is `P`'s, unchanged.
///
/// The wrapper is a place to put behaviour, not a level in the JSON. Both
/// methods delegate, so `schema_for_input::<SafeParameters<P>>()` and
/// `schema_for_input::<P>()` generate the same document — asserted below for
/// every input type this server publishes.
impl<P: JsonSchema> JsonSchema for SafeParameters<P> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        P::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        P::json_schema(generator)
    }
}

impl<S, P> FromContextPart<ToolCallContext<'_, S>> for SafeParameters<P>
where
    P: DeserializeOwned,
{
    fn from_context_part(context: &mut ToolCallContext<S>) -> Result<Self, ErrorData> {
        // An absent `arguments` member is an empty object, which is what the
        // SDK's wrapper does too: a tool whose fields all have defaults must
        // still be callable with no arguments at all.
        parse(context.arguments.take().unwrap_or_default())
    }
}

/// Deserialises one call's arguments, and answers the constant if it cannot.
///
/// Separate from the extractor above so the tests can drive the real thing:
/// this function *is* the boundary, and a `ToolCallContext` needs a live
/// service and request context that a unit test has no business building.
pub fn parse<P: DeserializeOwned>(arguments: JsonObject) -> Result<SafeParameters<P>, ErrorData> {
    match serde_json::from_value(serde_json::Value::Object(arguments)) {
        Ok(value) => Ok(SafeParameters(value)),
        // The error is discarded here, and the `_` is the whole point of this
        // module: it is bound to nothing, so there is no name in scope that
        // could be formatted, logged or attached as `data`. Whatever the
        // client sent stops at this line.
        Err(_) => Err(invalid_arguments()),
    }
}

/// The refusal, built in one place so every caller answers the same thing.
///
/// `data` is `None`. The JSON-RPC `data` member is the other way a payload
/// travels beside a message, and a refusal that carried the offending value
/// there would have moved the problem rather than fixed it.
pub fn invalid_arguments() -> ErrorData {
    ErrorData::invalid_params(INVALID_ARGUMENTS, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        AppendInput, ContextInput, CreateInput, EditInput, ListInput, PropertyRemoveInput,
        PropertySetInput, ReadInput, SearchInput, TagAddInput, TagRemoveInput, TaskCompleteInput,
        TaskReopenInput, TasksListInput, TrashRestoreInput,
    };
    use rmcp::handler::server::common::schema_for_input;
    use serde_json::{json, Map, Value};

    /// The wrapper publishes the wrapped type's schema, for every input this
    /// server has.
    ///
    /// This is the half of the change that has to be invisible. The extractor
    /// was replaced to alter what a *refusal* says; a host reading
    /// `tools/list` must not be able to tell that anything happened at all, and
    /// "must not be able to tell" is checked by generating both documents and
    /// comparing them rather than by reasoning about the delegation above.
    macro_rules! same_schema_as_the_wrapped_type {
        ($($name:ident: $ty:ty),+ $(,)?) => {
            $(
                #[test]
                fn $name() {
                    let wrapped = schema_for_input::<SafeParameters<$ty>>()
                        .expect("the wrapper's schema");
                    let plain = schema_for_input::<$ty>().expect("the type's own schema");
                    assert_eq!(
                        wrapped, plain,
                        concat!(
                            "SafeParameters changed the published schema of ",
                            stringify!($ty),
                        )
                    );
                }
            )+
        };
    }

    same_schema_as_the_wrapped_type! {
        the_schema_of_list_is_untouched: ListInput,
        the_schema_of_read_is_untouched: ReadInput,
        the_schema_of_search_is_untouched: SearchInput,
        the_schema_of_tasks_list_is_untouched: TasksListInput,
        the_schema_of_context_is_untouched: ContextInput,
        the_schema_of_create_is_untouched: CreateInput,
        the_schema_of_append_is_untouched: AppendInput,
        the_schema_of_edit_is_untouched: EditInput,
        the_schema_of_tag_add_is_untouched: TagAddInput,
        the_schema_of_tag_remove_is_untouched: TagRemoveInput,
        the_schema_of_property_set_is_untouched: PropertySetInput,
        the_schema_of_property_remove_is_untouched: PropertyRemoveInput,
        the_schema_of_task_complete_is_untouched: TaskCompleteInput,
        the_schema_of_task_reopen_is_untouched: TaskReopenInput,
        the_schema_of_trash_restore_is_untouched: TrashRestoreInput,
    }

    /// Every object is turned into a `JsonObject` the way the SDK hands one
    /// over, so the tests below call [`parse`] with exactly what it will get.
    fn arguments(value: Value) -> JsonObject {
        match value {
            Value::Object(object) => object,
            other => unreachable!("the tests build objects, not {other}"),
        }
    }

    /// Deserialising through the wrapper is deserialising into the type.
    ///
    /// The refusal is the thing that changed; acceptance must not have.
    #[test]
    fn a_valid_payload_still_becomes_the_input_it_always_did() {
        let value =
            json!({ "note_id": "0123abcd", "text": "olá", "expected_revision": "a".repeat(64) });
        let direct: AppendInput = serde_json::from_value(value.clone()).expect("the type");
        let SafeParameters(wrapped) = parse::<AppendInput>(arguments(value))
            .unwrap_or_else(|_| panic!("the wrapper refused a valid payload"));
        assert_eq!(direct, wrapped);
    }

    /// The refusal is the constant, whatever went wrong and however big it was.
    ///
    /// The unit test can only reach the function that builds it; that this is
    /// the sentence a real process writes to a real pipe is the business of
    /// `noteit-mcp/tests/mcp_argument_boundary.rs`, which measures the bytes.
    #[test]
    fn the_refusal_is_a_constant_and_carries_no_data() {
        let refusal = invalid_arguments();
        assert_eq!(refusal.message, INVALID_ARGUMENTS);
        assert!(refusal.data.is_none(), "a refusal carried a data member");
        assert!(
            INVALID_ARGUMENTS.len() < 120,
            "the refusal sentence is {} bytes",
            INVALID_ARGUMENTS.len()
        );
    }

    /// And nothing the arguments contain survives the extraction.
    ///
    /// The same shapes the wire suite sends, at a size a unit test can afford:
    /// a number given a string, an enum given an unknown variant, a boolean
    /// given a string, and a list given a scalar. Each one carries a canary,
    /// and the refusal is compared against the constant — so a future change
    /// that starts naming the field, the value or the length fails here as
    /// well as on the wire.
    #[test]
    fn no_shape_of_wrong_argument_puts_anything_of_its_own_in_the_refusal() {
        const CANARY: &str = "CANARIO-PARAMS-4E2R-R1";
        let long = CANARY.to_owned() + &"Z".repeat(4096);

        fn refuse<P: DeserializeOwned>(value: Value) -> ErrorData {
            parse::<P>(arguments(value))
                .map(|_| ())
                .expect_err("this payload must not deserialise")
        }

        let shapes: Vec<(&str, ErrorData)> = vec![
            (
                "number given a string",
                refuse::<ListInput>(json!({ "limit": &long })),
            ),
            (
                "enum given an unknown variant",
                refuse::<TasksListInput>(json!({ "state": &long })),
            ),
            (
                "boolean given a string",
                refuse::<ContextInput>(json!({ "include_tasks": &long })),
            ),
            (
                "list given a scalar",
                refuse::<ListInput>(json!({ "tags": &long })),
            ),
            (
                "a missing precondition",
                refuse::<AppendInput>(json!({ "note_id": &long, "text": "x" })),
            ),
        ];

        for (shape, refusal) in shapes {
            assert_eq!(refusal.message, INVALID_ARGUMENTS, "{shape}");
            let rendered = serde_json::to_string(&refusal).expect("serialise");
            assert!(!rendered.contains(CANARY), "{shape} published the canary");
            assert!(
                rendered.len() < 200,
                "{shape} answered {} bytes",
                rendered.len()
            );
        }
    }

    /// An absent `arguments` member is an empty object, exactly as before.
    ///
    /// Every input in this server derives `Default` or has only optional
    /// fields where that is the intent, so a tool called with no arguments at
    /// all must still reach its handler rather than be refused by the
    /// extractor.
    #[test]
    fn no_arguments_at_all_is_an_empty_object() {
        let SafeParameters(empty) = parse::<ListInput>(Map::new()).expect("an empty object");
        assert_eq!(empty, ListInput::default());
    }
}
