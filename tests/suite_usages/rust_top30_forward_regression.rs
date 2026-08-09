use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::usages::get_definition::trace::{
    TraceCandidateRef, resolve_definition_batch_with_trace,
};
use brokk_bifrost::analyzer::usages::get_definition::{
    DefinitionLookupRequest, ResolutionTraceResult,
};
use brokk_bifrost::{AnalyzerConfig, CancellationToken, ProjectFile};
use std::sync::Arc;

fn trace_for(
    files: &[(&str, &str)],
    path: &str,
    expression: &str,
    focus: &str,
) -> ResolutionTraceResult {
    outcome_for(files, path, expression, focus).1
}

fn outcome_for(
    files: &[(&str, &str)],
    path: &str,
    expression: &str,
    focus: &str,
) -> (
    brokk_bifrost::usages::get_definition::DefinitionLookupOutcome,
    ResolutionTraceResult,
) {
    let mut builder = InlineTestProject::new();
    for (file, source) in files {
        builder = builder.file(file, *source);
    }
    let project = builder.build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let file: ProjectFile = project.file(path);
    let source = file.read_to_string().expect("fixture source");
    let expression_start = source.rfind(expression).expect("expression in source");
    let focus_start = expression
        .find(focus)
        .map(|offset| expression_start + offset)
        .expect("focus in expression");
    let request = DefinitionLookupRequest {
        file: file.clone(),
        line: None,
        column: None,
        start_byte: Some(focus_start),
        end_byte: Some(focus_start + focus.len()),
    };
    let mut traced = resolve_definition_batch_with_trace(
        workspace.analyzer(),
        vec![request],
        file,
        Arc::<str>::from(source),
        &CancellationToken::new(),
    );
    let (outcome, trace) = traced.pop().expect("one trace");
    (outcome, trace)
}

fn selected_names(trace: &ResolutionTraceResult) -> Vec<String> {
    trace
        .candidates
        .iter()
        .filter(|row| row.is_selected())
        .filter_map(|row| match &row.candidate {
            TraceCandidateRef::Unit(unit) => Some(unit.fq_name()),
            _ => None,
        })
        .collect()
}

fn selected_paths(trace: &ResolutionTraceResult) -> Vec<String> {
    trace
        .candidates
        .iter()
        .filter(|row| row.is_selected())
        .filter_map(|row| match &row.candidate {
            TraceCandidateRef::Unit(unit) => {
                Some(unit.source().rel_path().to_string_lossy().into_owned())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn rust_enum_variant_prefers_local_reexport_over_python_module_class() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod tokenizer;\npub use tokenizer::*;\n"),
            (
                "src/tokenizer/mod.rs",
                "pub mod pre_tokenizer;\nuse pre_tokenizer::OffsetType;\n\npub fn encode() {\n    let _ = OffsetType::None;\n}\n",
            ),
            (
                "src/tokenizer/pre_tokenizer.rs",
                "pub enum OffsetType { Byte, Char, None }\n",
            ),
            ("tokenizers/__init__.py", "class OffsetType:\n    pass\n"),
        ],
        "src/tokenizer/mod.rs",
        "OffsetType::None",
        "None",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.pre_tokenizer.OffsetType.None"],
        "the Rust enum variant must win over the same-named Python class: {trace:#?}"
    );
}

#[test]
fn rust_enum_variant_with_python_package_layout_prefers_rust_reexport() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod tokenizer;\npub use tokenizer::*;\n"),
            (
                "src/tokenizer/mod.rs",
                "pub mod pre_tokenizer;\npub use pre_tokenizer::*;\n\npub fn encode() {\n    let _ = OffsetType::None;\n}\n",
            ),
            (
                "src/tokenizer/pre_tokenizer.rs",
                "pub enum OffsetType { Byte, Char, None }\n",
            ),
            (
                "bindings/python/py_src/tokenizers/__init__.py",
                "class OffsetType:\n    pass\n",
            ),
        ],
        "src/tokenizer/mod.rs",
        "OffsetType::None",
        "None",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.pre_tokenizer.OffsetType.None"],
        "the Rust glob re-export must beat the Python package class: {trace:#?}"
    );
}

#[test]
fn rust_enum_owner_with_python_package_layout_prefers_rust_reexport() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod tokenizer;\npub use tokenizer::*;\n"),
            (
                "src/tokenizer/mod.rs",
                "pub mod pre_tokenizer;\npub use pre_tokenizer::*;\n\npub fn encode() {\n    let _ = OffsetType::None;\n}\n",
            ),
            (
                "src/tokenizer/pre_tokenizer.rs",
                "pub enum OffsetType { Byte, Char, None }\n",
            ),
            (
                "bindings/python/py_src/tokenizers/__init__.py",
                "from enum import Enum\nclass OffsetType(Enum):\n    Byte = 1\n    Char = 2\n    None_ = 3\n",
            ),
        ],
        "src/tokenizer/mod.rs",
        "OffsetType::None",
        "OffsetType",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.pre_tokenizer.OffsetType"],
        "the focused Rust enum owner must beat the Python package class: {trace:#?}"
    );
}

#[test]
fn rust_enum_owner_with_python_crate_name_collision_prefers_rust_reexport() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"tokenizers\", \"bindings/python\"]\nresolver = \"2\"\n",
            ),
            (
                "tokenizers/Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "tokenizers/src/lib.rs",
                "pub mod tokenizer;\npub use tokenizer::*;\nmod private_scope;\n",
            ),
            (
                "tokenizers/src/tokenizer/mod.rs",
                "pub mod pre_tokenizer;\npub use pre_tokenizer::*;\n\npub fn encode() {\n    let _ = OffsetType::None;\n}\n",
            ),
            (
                "tokenizers/src/tokenizer/pre_tokenizer.rs",
                "#[derive(Clone, Copy, Debug)]\npub enum OffsetType { Byte, Char, None }\n",
            ),
            (
                "tokenizers/src/private_scope.rs",
                "pub enum OffsetType { Hidden }\n",
            ),
            (
                "bindings/python/Cargo.toml",
                "[package]\nname = \"tokenizers-python\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nname = \"tokenizers\"\npath = \"src/lib.rs\"\n[dependencies]\ntokenizers = { path = \"../../tokenizers\" }\n",
            ),
            (
                "bindings/python/src/lib.rs",
                "extern crate tokenizers as tk;\nmod tokenizer { pub struct OffsetType; }\n",
            ),
            (
                "bindings/python/py_src/tokenizers/__init__.py",
                "from enum import Enum\nclass OffsetType(Enum):\n    Byte = 1\n    Char = 2\n    None_ = 3\n",
            ),
        ],
        "tokenizers/src/tokenizer/mod.rs",
        "OffsetType::None",
        "OffsetType",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.pre_tokenizer.OffsetType"],
        "the Rust target must win when the Python crate uses the same lib name: {trace:#?}"
    );
}

#[test]
fn rust_enum_variant_prefers_imported_rust_type_over_python_module_class() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod tokenizer;\npub use tokenizer::*;\n"),
            (
                "src/tokenizer/mod.rs",
                "pub mod normalizer;\npub use normalizer::SplitDelimiterBehavior;\n",
            ),
            (
                "src/tokenizer/normalizer.rs",
                "pub enum SplitDelimiterBehavior { Removed, Isolated }\n",
            ),
            (
                "bindings/python/src/pre_tokenizers.rs",
                "use crate::tokenizer::normalizer::SplitDelimiterBehavior;\npub fn parse() {\n    let _ = SplitDelimiterBehavior::Isolated;\n}\n",
            ),
            ("bindings/python/src/lib.rs", "pub mod pre_tokenizers;\n"),
            (
                "tokenizers/__init__.py",
                "class SplitDelimiterBehavior:\n    pass\n",
            ),
        ],
        "bindings/python/src/pre_tokenizers.rs",
        "SplitDelimiterBehavior::Isolated",
        "Isolated",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.normalizer.SplitDelimiterBehavior.Isolated"],
        "the imported Rust enum variant must win over the same-named Python class: {trace:#?}"
    );
}

#[test]
fn rust_external_crate_alias_follows_root_reexport_for_nested_type() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"tokenizers\", \"bindings/python\"]\nresolver = \"2\"\n",
            ),
            (
                "tokenizers/Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "tokenizers/src/lib.rs",
                "pub mod tokenizer;\npub use tokenizer::*;\n",
            ),
            (
                "tokenizers/src/tokenizer/mod.rs",
                "pub mod normalizer;\npub use normalizer::*;\n",
            ),
            (
                "tokenizers/src/tokenizer/normalizer.rs",
                "pub enum SplitDelimiterBehavior { Removed, Isolated }\n",
            ),
            (
                "bindings/python/Cargo.toml",
                "[package]\nname = \"tokenizers-python\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nname = \"tokenizers\"\npath = \"src/lib.rs\"\n[dependencies]\ntokenizers = { path = \"../../tokenizers\" }\n",
            ),
            ("bindings/python/src/lib.rs", "pub mod pre_tokenizers;\n"),
            (
                "bindings/python/src/pre_tokenizers.rs",
                "use tokenizers as tk;\nuse tk::normalizer::SplitDelimiterBehavior;\npub fn parse() {\n    let _ = SplitDelimiterBehavior::Isolated;\n}\n",
            ),
            (
                "tokenizers/__init__.py",
                "class SplitDelimiterBehavior:\n    pass\n",
            ),
        ],
        "bindings/python/src/pre_tokenizers.rs",
        "SplitDelimiterBehavior::Isolated",
        "Isolated",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.normalizer.SplitDelimiterBehavior.Isolated"],
        "an external crate alias must follow the root re-export to the nested Rust enum: {trace:#?}"
    );
}

#[test]
fn rust_tuple_enum_variant_prefers_local_reexport_over_python_module_class() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod tokenizer;\npub use tokenizer::*;\n"),
            (
                "src/tokenizer/mod.rs",
                "pub mod pre_tokenizer;\nuse pre_tokenizer::OffsetType;\n\npub fn encode() {\n    let _ = OffsetType::Byte(1);\n}\n",
            ),
            (
                "src/tokenizer/pre_tokenizer.rs",
                "pub enum OffsetType { Byte(u8), Char, None }\n",
            ),
            ("tokenizers/__init__.py", "class OffsetType:\n    pass\n"),
        ],
        "src/tokenizer/mod.rs",
        "OffsetType::Byte",
        "Byte",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.pre_tokenizer.OffsetType.Byte"],
        "a tuple enum variant must win over the same-named Python class: {trace:#?}"
    );
}

#[test]
fn rust_crate_segment_keeps_focus_on_crate_root() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"sqlx-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub mod types { pub struct Type; }\nmacro_rules! rows { ($($T:ident),+ $(,)?) => { fn use_type<$($T: crate::types::Type),+>() {} }; }\nrows!(Item);\n",
            ),
        ],
        "src/lib.rs",
        "crate",
        "crate",
    );
    assert_eq!(
        outcome.status,
        brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition,
        "a focused crate segment has no indexed declaration: {outcome:#?}"
    );
    assert!(
        selected_names(&trace).is_empty(),
        "the focused crate segment must not resolve the terminal Type: {trace:#?}"
    );
}

#[test]
fn rust_crate_segment_outside_macro_keeps_existing_no_definition() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"sqlx-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub mod types { pub struct Type; }\nfn use_type(_: crate::types::Type) {}\n",
            ),
        ],
        "src/lib.rs",
        "crate",
        "crate",
    );
    assert_eq!(
        outcome.status,
        brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition,
        "crate outside a macro must keep its existing segment result: {outcome:#?}"
    );
    assert!(selected_names(&trace).is_empty(), "{trace:#?}");
}

#[test]
fn rust_local_module_import_beats_same_named_dependency_module() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
            ),
            (
                "crates/sqlx-core/Cargo.toml",
                "[package]\nname = \"sqlx-core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "crates/sqlx-core/src/lib.rs",
                "pub mod arguments;\npub mod driver_prelude { pub use crate::io; }\npub mod io;\n",
            ),
            (
                "crates/sqlx-core/src/arguments.rs",
                "pub struct CoreArguments;\n",
            ),
            ("crates/sqlx-core/src/io.rs", "pub struct Io;\n"),
            (
                "crates/sqlx-sqlite/Cargo.toml",
                "[package]\nname = \"sqlx-sqlite\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nsqlx-core = { path = \"../sqlx-core\" }\n",
            ),
            (
                "crates/sqlx-sqlite/src/lib.rs",
                "#[macro_use]\nextern crate sqlx_core;\n\nuse std::sync::atomic::AtomicBool;\npub use arguments::SqliteArguments;\npub(crate) use sqlx_core::driver_prelude::*;\nuse sqlx_core::io::Io;\nmod arguments;\n",
            ),
            (
                "crates/sqlx-sqlite/src/arguments.rs",
                "pub struct SqliteArguments;\n",
            ),
        ],
        "crates/sqlx-sqlite/src/lib.rs",
        "arguments::SqliteArguments",
        "arguments",
    );
    assert_eq!(
        outcome.status,
        brokk_bifrost::usages::get_definition::DefinitionLookupStatus::Resolved,
        "the local module import must resolve: {outcome:#?}"
    );
    assert_eq!(
        selected_names(&trace),
        ["sqlx_sqlite.arguments"],
        "the local SQLx module must beat the same-named dependency module: {trace:#?}"
    );
}

#[test]
fn rust_macro_export_import_beats_private_same_named_module() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
            ),
            (
                "crates/primitives/Cargo.toml",
                "[package]\nname = \"spacetimedb-primitives\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "crates/primitives/src/lib.rs",
                "mod col_list;\npub use col_list::ColList;\n",
            ),
            (
                "crates/primitives/src/col_list.rs",
                "pub struct ColList;\n#[macro_export]\nmacro_rules! col_list { ($($x:expr),*) => { $crate::ColList }; }\n",
            ),
            (
                "crates/bench/Cargo.toml",
                "[package]\nname = \"bench\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nspacetimedb-primitives = { path = \"../primitives\" }\n",
            ),
            (
                "crates/bench/src/lib.rs",
                "use spacetimedb_primitives::{col_list, TableId};\nfn run() { col_list![1, 2]; let _: TableId; }\n",
            ),
        ],
        "crates/bench/src/lib.rs",
        "spacetimedb_primitives::{col_list, TableId}",
        "col_list",
    );
    assert_eq!(
        outcome.status,
        brokk_bifrost::usages::get_definition::DefinitionLookupStatus::Resolved,
        "the macro export import must resolve: {outcome:#?}"
    );
    assert_eq!(
        selected_names(&trace),
        ["spacetimedb_primitives.col_list.col_list"],
        "the exported macro must beat the private module: {trace:#?}"
    );
}

#[test]
fn rust_macro_timestamp_does_not_claim_same_named_enum_field() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"bson\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub mod bson;\npub use bson::Timestamp;\npub mod serde_helpers;\nmacro_rules! serde_conv_doc { ($(#[$meta:meta])* $vis:vis $m:ident, $t:ty, $ser:expr, $de:expr) => {}; }\n",
            ),
            (
                "src/bson.rs",
                "pub struct Timestamp { pub time: u32 }\npub enum Bson { Timestamp(Timestamp) }\n",
            ),
            (
                "src/serde_helpers.rs",
                "use crate::Timestamp;\nserde_conv_doc!(pub Convert, Timestamp, |value: &Timestamp| -> Result<u32, String> { let _ = value; Ok(0) }, |value: u32| -> Result<Timestamp, String> { let _ = value; Ok(Timestamp { time: value }) });\n",
            ),
        ],
        "src/serde_helpers.rs",
        "pub Convert, Timestamp",
        "Timestamp",
    );
    assert!(
        matches!(
            outcome.status,
            brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition
                | brokk_bifrost::usages::get_definition::DefinitionLookupStatus::UnresolvableImportBoundary
        ),
        "an opaque macro type must not claim Bson::Timestamp: {outcome:#?}"
    );
    assert!(outcome.definitions.is_empty(), "{outcome:#?}");
    assert!(selected_names(&trace).is_empty(), "{trace:#?}");
}

#[test]
fn rust_repeated_ident_macro_argument_resolves_imported_struct() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"bson\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub mod bson;\npub use bson::Timestamp;\npub mod serde_helpers;\nmacro_rules! adapters { ($( $ty:ident, $ser:tt, $de:ident );* $(;)?) => { $( fn generated(value: $ty) -> $ty { value } )* }; }\n",
            ),
            (
                "src/bson.rs",
                "pub struct Timestamp { pub time: u32 }\npub enum Bson { Timestamp(Timestamp) }\n",
            ),
            (
                "src/serde_helpers.rs",
                "use crate::Timestamp;\nadapters! { Timestamp, {}, decode; }\n",
            ),
        ],
        "src/serde_helpers.rs",
        "adapters! { Timestamp",
        "Timestamp",
    );
    assert_eq!(
        selected_names(&trace),
        ["bson.bson.Timestamp"],
        "a repeated ident macro argument in a generated type must resolve the imported struct: {trace:#?}"
    );
}

#[test]
fn rust_expr_macro_keeps_local_value_namespace() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"bson\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub struct Timestamp;\nmacro_rules! take { ($e:expr) => {}; }\nfn use_value() { let Timestamp = 1; take!(Timestamp); }\n",
            ),
        ],
        "src/lib.rs",
        "take!(Timestamp",
        "Timestamp",
    );
    assert_eq!(
        outcome.status,
        brokk_bifrost::usages::get_definition::DefinitionLookupStatus::Resolved,
        "an expr macro value must resolve its lexical local: {outcome:#?}"
    );
    assert!(outcome.definitions.is_empty(), "{outcome:#?}");
    assert_eq!(
        outcome
            .lexical_definition
            .as_ref()
            .map(|definition| definition.kind),
        Some(brokk_bifrost::DeclarationKind::LocalVariable),
        "an expr macro value must keep the local-variable namespace: {outcome:#?}"
    );
    assert!(selected_names(&trace).is_empty(), "{trace:#?}");
}

#[test]
fn rust_macro_decl_ident_does_not_claim_imported_type() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"macro_item\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub mod types { pub struct Item; }\nuse types::Item;\nmacro_rules! make { ($name:ident) => { struct $name; } }\nfn build() { make!(Item); }\n",
            ),
        ],
        "src/lib.rs",
        "make!(Item",
        "Item",
    );
    assert!(
        !selected_names(&trace)
            .iter()
            .any(|name| name == "macro_item.types.Item"),
        "a macro declaration ident must not select the imported type: {trace:#?}"
    );
}

#[test]
fn rust_scoped_struct_field_is_not_an_enum_variant_value() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub struct OffsetType { pub None: u8 }\nfn use_value() { let _ = OffsetType::None; }\n",
            ),
        ],
        "src/lib.rs",
        "OffsetType::None",
        "None",
    );
    assert!(
        matches!(
            outcome.status,
            brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition
                | brokk_bifrost::usages::get_definition::DefinitionLookupStatus::UnresolvableImportBoundary
        ),
        "a struct field must not enter the enum-variant value namespace: {outcome:#?}"
    );
    assert!(outcome.definitions.is_empty(), "{outcome:#?}");
    assert!(selected_names(&trace).is_empty(), "{trace:#?}");
}

#[test]
fn rust_type_alias_owner_prefers_aliased_impl_method_over_wrapper_method() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"tokenizers\", \"bindings\"]\nresolver = \"2\"\n",
            ),
            (
                "tokenizers/Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "tokenizers/src/lib.rs",
                "pub mod tokenizer;\npub use tokenizer::{Tokenizer, TokenizerImpl};\n",
            ),
            (
                "tokenizers/src/tokenizer.rs",
                "pub trait Model {}\npub trait Normalizer {}\npub trait PreTokenizer {}\npub trait PostProcessor {}\npub trait Decoder {}\npub struct TokenizerImpl<M, N, PT, PP, D> { _marker: core::marker::PhantomData<(M, N, PT, PP, D)> }\nimpl<M, N, PT, PP, D> TokenizerImpl<M, N, PT, PP, D>\nwhere\n    M: Model,\n    N: Normalizer,\n    PT: PreTokenizer,\n    PP: PostProcessor,\n    D: Decoder,\n{\n    pub fn from_file(_: &str) {}\n}\npub struct Tokenizer;\nimpl Tokenizer { pub fn from_file(_: &str) {} }\n",
            ),
            (
                "bindings/Cargo.toml",
                "[package]\nname = \"bindings\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\ntokenizers = { path = \"../tokenizers\" }\n",
            ),
            (
                "bindings/src/lib.rs",
                "use tokenizers::TokenizerImpl;\nstruct PyModel;\nstruct PyNormalizer;\nstruct PyPreTokenizer;\nstruct PyPostProcessor;\nstruct PyDecoder;\ntype Tokenizer = TokenizerImpl<PyModel, PyNormalizer, PyPreTokenizer, PyPostProcessor, PyDecoder>;\nfn load() { Tokenizer::from_file(\"x\"); }\n",
            ),
        ],
        "bindings/src/lib.rs",
        "Tokenizer::from_file",
        "from_file",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.TokenizerImpl.from_file"],
        "the alias owner must route to the aliased implementation: {trace:#?}"
    );
}

#[test]
fn rust_nested_type_alias_owner_routes_to_nested_alias_target() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"aliases\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub struct TokenizerImpl;\nimpl TokenizerImpl { pub fn from_file(_: &str) {} }\npub mod nested;\n",
            ),
            (
                "src/nested.rs",
                "use super::TokenizerImpl;\ntype Tokenizer = TokenizerImpl;\nfn load() { Tokenizer::from_file(\"x\"); }\n",
            ),
        ],
        "src/nested.rs",
        "Tokenizer::from_file",
        "from_file",
    );
    assert_eq!(
        selected_names(&trace),
        ["aliases.TokenizerImpl.from_file"],
        "a nested alias must use its own lexical target: {trace:#?}"
    );
}

#[test]
fn rust_shadowed_local_type_alias_beats_outer_alias() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"aliases\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub struct TokenizerImpl;\nimpl TokenizerImpl { pub fn from_file(_: &str) {} }\npub struct Wrapper;\nimpl Wrapper { pub fn from_file(_: &str) {} }\ntype Tokenizer = TokenizerImpl;\nfn load() { type Tokenizer = Wrapper; Tokenizer::from_file(\"x\"); }\n",
            ),
        ],
        "src/lib.rs",
        "Tokenizer::from_file",
        "from_file",
    );
    assert_eq!(
        outcome.status,
        brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition,
        "a local alias must not select the outer alias: {outcome:#?}"
    );
    assert!(outcome.definitions.is_empty(), "{outcome:#?}");
    assert!(selected_names(&trace).is_empty(), "{trace:#?}");
}

#[test]
fn rust_unrelated_nested_alias_does_not_shadow_root_alias() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"aliases\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub struct TokenizerImpl;\nimpl TokenizerImpl { pub fn from_file(_: &str) {} }\npub mod nested { pub struct Wrapper; impl Wrapper { pub fn from_file(_: &str) {} } type Tokenizer = Wrapper; }\ntype Tokenizer = TokenizerImpl;\nfn load() { Tokenizer::from_file(\"x\"); }\n",
            ),
        ],
        "src/lib.rs",
        "Tokenizer::from_file",
        "from_file",
    );
    assert_eq!(
        selected_names(&trace),
        ["aliases.TokenizerImpl.from_file"],
        "an unrelated nested alias must not shadow the root alias: {trace:#?}"
    );
}

#[test]
fn rust_explicit_reexport_stays_in_its_cargo_target() {
    let trace = trace_for(
        &[
            (
                "tokenizers/Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "tokenizers/src/lib.rs",
                "pub mod model;\npub use model::Wrapper;\npub mod consumer;\n",
            ),
            ("tokenizers/src/model.rs", "pub struct Wrapper;\n"),
            (
                "tokenizers/src/consumer.rs",
                "use crate::Wrapper;\npub fn consume(_: Wrapper) {}\n",
            ),
            (
                "bindings/python/Cargo.toml",
                "[package]\nname = \"tokenizers-python\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nname = \"tokenizers\"\npath = \"src/lib.rs\"\n[dependencies]\ntokenizers = { path = \"../../tokenizers\" }\n",
            ),
            (
                "bindings/python/src/lib.rs",
                "mod model;\npub use model::Wrapper;\n",
            ),
            ("bindings/python/src/model.rs", "pub struct Wrapper;\n"),
        ],
        "tokenizers/src/consumer.rs",
        "consume(_: Wrapper)",
        "Wrapper",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.model.Wrapper"],
        "an explicit re-export must stay in the importing file's Cargo target: {trace:#?}"
    );
    assert_eq!(selected_paths(&trace), ["tokenizers/src/model.rs"]);
}

#[test]
fn rust_imported_type_alias_owner_routes_to_aliased_impl_method() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"aliases\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/lib.rs",
                "pub mod aliases;\npub mod consumer;\npub struct TokenizerImpl;\nimpl TokenizerImpl { pub fn from_file(_: &str) {} }\npub struct Tokenizer;\nimpl Tokenizer { pub fn from_file(_: &str) {} }\n",
            ),
            (
                "src/aliases.rs",
                "use crate::TokenizerImpl;\npub type Tokenizer = TokenizerImpl;\n",
            ),
            (
                "src/consumer.rs",
                "use crate::aliases::Tokenizer;\npub fn load() { Tokenizer::from_file(\"x\"); }\n",
            ),
        ],
        "src/consumer.rs",
        "Tokenizer::from_file",
        "from_file",
    );
    assert_eq!(
        selected_names(&trace),
        ["aliases.TokenizerImpl.from_file"],
        "an imported type alias must route to its aliased implementation: {trace:#?}"
    );
}

#[test]
fn rust_huggingface_tokenizers_type_alias_uses_python_binding_lexical_owner() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"tokenizers\", \"bindings/python\"]\nresolver = \"2\"\n",
            ),
            (
                "tokenizers/Cargo.toml",
                "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("tokenizers/src/lib.rs", "pub mod tokenizer;\n"),
            (
                "tokenizers/src/tokenizer/mod.rs",
                "pub struct Tokenizer;\nimpl Tokenizer { pub fn from_file(_: &str) {} }\n\npub struct TokenizerImpl<M, N, PT, PP, D> { _marker: core::marker::PhantomData<(M, N, PT, PP, D)> }\nimpl<M, N, PT, PP, D> TokenizerImpl<M, N, PT, PP, D>\nwhere\n    M: Model,\n    N: Normalizer,\n    PT: PreTokenizer,\n    PP: PostProcessor,\n    D: Decoder,\n{\n    pub fn from_file(_: &str) {}\n}\npub trait Model {}\npub trait Normalizer {}\npub trait PreTokenizer {}\npub trait PostProcessor {}\npub trait Decoder {}\n",
            ),
            (
                "bindings/python/Cargo.toml",
                "[package]\nname = \"tokenizers-python\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nname = \"tokenizers\"\npath = \"src/lib.rs\"\n[dependencies]\ntokenizers = { path = \"../../tokenizers\" }\n",
            ),
            ("bindings/python/src/lib.rs", "pub mod tokenizer;\n"),
            (
                "bindings/python/src/tokenizer.rs",
                "use tokenizers as tk;\nuse tk::tokenizer::TokenizerImpl;\n\nstruct PyModel;\nstruct PyNormalizer;\nstruct PyPreTokenizer;\nstruct PyPostProcessor;\nstruct PyDecoder;\nimpl tk::tokenizer::Model for PyModel {}\nimpl tk::tokenizer::Normalizer for PyNormalizer {}\nimpl tk::tokenizer::PreTokenizer for PyPreTokenizer {}\nimpl tk::tokenizer::PostProcessor for PyPostProcessor {}\nimpl tk::tokenizer::Decoder for PyDecoder {}\n\ntype Tokenizer = TokenizerImpl<PyModel, PyNormalizer, PyPreTokenizer, PyPostProcessor, PyDecoder>;\n\npub struct PyTokenizer;\nimpl PyTokenizer {\n    pub fn from_file(path: &str) {\n        Tokenizer::from_file(path);\n    }\n}\n",
            ),
        ],
        "bindings/python/src/tokenizer.rs",
        "Tokenizer::from_file",
        "from_file",
    );
    assert_eq!(
        selected_names(&trace),
        ["tokenizers.tokenizer.TokenizerImpl.from_file"],
        "the Python binding alias must route to TokenizerImpl, not the wrapper: {trace:#?}"
    );
}

#[test]
fn rust_unindexed_external_type_import_does_not_claim_same_named_local_struct() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"ron\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nserde = \"1\"\n",
            ),
            ("src/lib.rs", "pub mod de;\n"),
            (
                "src/de/mod.rs",
                "pub struct Deserializer<'de> { _marker: core::marker::PhantomData<&'de ()> }\npub mod value;\n",
            ),
            (
                "src/de/value.rs",
                "use serde::Deserializer;\npub fn decode<D: Deserializer<'static>>() {}\n",
            ),
        ],
        "src/de/value.rs",
        "D: Deserializer",
        "Deserializer",
    );
    assert!(
        matches!(
            outcome.status,
            brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition
                | brokk_bifrost::usages::get_definition::DefinitionLookupStatus::UnresolvableImportBoundary
        ),
        "an unindexed serde trait must not resolve to ron::de::Deserializer: {outcome:#?}"
    );
    assert!(outcome.definitions.is_empty(), "{outcome:#?}");
    assert!(selected_names(&trace).is_empty(), "{trace:#?}");
}

#[test]
fn rust_nested_unindexed_self_associated_type_does_not_claim_outer_impl() {
    let (outcome, trace) = outcome_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"bson\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nserde = \"1\"\n",
            ),
            ("src/lib.rs", "pub mod de;\n"),
            ("src/de/mod.rs", "pub mod serde;\n"),
            (
                "src/de/serde.rs",
                "use serde::de::Visitor;\npub struct BsonVisitor;\nimpl<'de> Visitor<'de> for BsonVisitor {\n    type Value = ();\n    fn visit_map(&self) {\n        struct BytesOrHexVisitor;\n        impl<'de> Visitor<'de> for BytesOrHexVisitor {\n            fn visit_borrowed_str<E>(self, _v: &'de str) -> std::result::Result<Self::Value, E>\n            where\n                E: serde::de::Error,\n            {\n                unimplemented!()\n            }\n        }\n    }\n}\n",
            ),
        ],
        "src/de/serde.rs",
        "Self::Value",
        "Value",
    );
    assert!(
        matches!(
            outcome.status,
            brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition
                | brokk_bifrost::usages::get_definition::DefinitionLookupStatus::UnresolvableImportBoundary
        ),
        "an unindexed nested impl owner must not resolve Self::Value to BsonVisitor.Value: {outcome:#?}"
    );
    assert!(outcome.definitions.is_empty(), "{outcome:#?}");
    assert!(selected_names(&trace).is_empty(), "{trace:#?}");
}

#[test]
fn rust_imported_enum_type_is_not_selected_for_a_bare_value_call() {
    let files = [
        (
            "Cargo.toml",
            "[package]\nname = \"nom\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        ("src/lib.rs", "pub mod internal;\npub use internal::Err;\n"),
        ("src/internal.rs", "pub enum Err<E> { Error(E) }\n"),
        (
            "benchmarks/benches/json.rs",
            "fn parse() { let _ = Err(()); }\nuse nom::Err;\n",
        ),
    ];
    let (outcome, trace) = outcome_for(&files, "benchmarks/benches/json.rs", "Err(())", "Err");
    assert!(outcome.definitions.is_empty(), "{outcome:#?}");
    assert!(
        !selected_names(&trace)
            .iter()
            .any(|name| name.ends_with(".Err")),
        "{trace:#?}"
    );
    assert!(matches!(
        outcome.status,
        brokk_bifrost::usages::get_definition::DefinitionLookupStatus::NoDefinition
            | brokk_bifrost::usages::get_definition::DefinitionLookupStatus::UnresolvableImportBoundary
    ), "{outcome:#?}");
}

#[test]
fn rust_imported_tuple_struct_remains_a_callable_value() {
    let trace = trace_for(
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod types;\npub use types::Token;\n"),
            ("src/types.rs", "pub struct Token(u8);\n"),
            (
                "src/consumer.rs",
                "use crate::Token;\nfn parse() { let _ = Token(1); }\n",
            ),
        ],
        "src/consumer.rs",
        "Token(1)",
        "Token",
    );
    assert_eq!(
        selected_names(&trace),
        ["demo.types.Token"],
        "a tuple struct import remains callable in the value namespace: {trace:#?}"
    );
}
