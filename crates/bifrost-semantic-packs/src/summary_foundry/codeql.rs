//! Translation of CodeQL Models-as-Data rows into the foundry IR.
//!
//! A Models-as-Data file is a YAML document of extension blocks. Each block
//! names an extensible predicate and carries rows of fixed width. Milestone 1
//! reads two of them:
//!
//! * `summaryModel` rows, `[package, type, subtypes, name, signature, ext,
//!   input, output, kind, provenance]`, one value flow each.
//! * `neutralModel` rows, `[package, type, name, signature, kind, provenance]`,
//!   which state for `kind: summary` that the member carries no flow.
//!
//! `sourceModel` and `sinkModel` rows are out of scope for this milestone and
//! are counted as skipped rows, never dropped in silence.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use brokk_bifrost_analysis::analyzer::semantic_model::{
    AuthoredSummaryExitKind, AuthoredSummaryInput, AuthoredSummaryOutput, AuthoredSummaryTransfer,
};
use serde::Deserialize;

use super::ir::{
    FoundryCorpus, FoundryEntry, FoundryEntryBuilder, FoundryEvidence, FoundryNote,
    FoundrySignature, FoundrySkip, FoundrySkipReason, FoundryTarget, class_file_path,
    nested_type_simple_name, split_parameter_types,
};

const SUMMARY_MODEL: &str = "summaryModel";
const NEUTRAL_MODEL: &str = "neutralModel";
const SUMMARY_ROW_COLUMNS: usize = 10;
const NEUTRAL_ROW_COLUMNS: usize = 6;

/// Everything one Models-as-Data corpus translation produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeqlTranslation {
    pub entries: Vec<FoundryEntry>,
    pub files_read: u32,
    pub rows_by_kind: BTreeMap<String, u32>,
    pub skips: Vec<FoundrySkip>,
}

/// A Models-as-Data file the translator could not read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeqlParseError {
    pub file: String,
    pub message: String,
}

impl std::fmt::Display for CodeqlParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.file, self.message)
    }
}

impl std::error::Error for CodeqlParseError {}

#[derive(Debug, Deserialize)]
struct MadDocument {
    #[serde(default)]
    extensions: Vec<MadExtension>,
}

#[derive(Debug, Deserialize)]
struct MadExtension {
    #[serde(rename = "addsTo")]
    adds_to: MadAddsTo,
    #[serde(default)]
    data: Vec<Vec<MadCell>>,
}

#[derive(Debug, Deserialize)]
struct MadAddsTo {
    extensible: String,
}

/// One row cell. Models-as-Data spells the `subtypes` column as a YAML boolean
/// and every other column as a string.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum MadCell {
    Bool(bool),
    Text(String),
}

impl MadCell {
    fn text(&self) -> &str {
        match self {
            Self::Bool(true) => "true",
            Self::Bool(false) => "false",
            Self::Text(value) => value,
        }
    }

    /// The cell read as the `subtypes` flag.
    ///
    /// The corpus writes `True`/`False`; a reader that resolves YAML 1.1
    /// booleans returns [`Self::Bool`] and one that does not returns the
    /// spelling, so both are accepted here and nothing else is.
    fn boolean(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Text(value) => match value.as_str() {
                "true" | "True" | "TRUE" => Some(true),
                "false" | "False" | "FALSE" => Some(false),
                _ => None,
            },
        }
    }
}

fn yaml_options() -> serde_saphyr::Options {
    serde_saphyr::options! {
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
        merge_keys: serde_saphyr::MergeKeyPolicy::Error,
        strict_booleans: false,
    }
}

/// Translate a set of Models-as-Data files, read in the given order.
///
/// The caller supplies file name and bytes so the report never carries a host
/// path and two runs over the same pin produce the same bytes.
pub fn translate_codeql_models(
    files: &[(String, Vec<u8>)],
) -> Result<CodeqlTranslation, CodeqlParseError> {
    let mut builders: BTreeMap<FoundryTarget, FoundryEntryBuilder> = BTreeMap::new();
    let mut rows_by_kind: BTreeMap<String, u32> = BTreeMap::new();
    let mut skips = Vec::new();
    let mut declared_arity: BTreeMap<FoundryTarget, Option<u32>> = BTreeMap::new();

    for (file, bytes) in files {
        let document: MadDocument = serde_saphyr::from_slice_with_options(bytes, yaml_options())
            .map_err(|error| CodeqlParseError {
                file: file.clone(),
                message: error.to_string(),
            })?;
        for extension in &document.extensions {
            let kind = extension.adds_to.extensible.as_str();
            let mut ordinal = 0u32;
            for row in &extension.data {
                ordinal += 1;
                *rows_by_kind.entry(kind.to_owned()).or_default() += 1;
                match kind {
                    SUMMARY_MODEL => translate_summary_row(
                        file,
                        ordinal,
                        row,
                        &mut builders,
                        &mut declared_arity,
                        &mut skips,
                    ),
                    NEUTRAL_MODEL => translate_neutral_row(
                        file,
                        ordinal,
                        row,
                        &mut builders,
                        &mut declared_arity,
                        &mut skips,
                    ),
                    _ => skips.push(FoundrySkip {
                        file: file.clone(),
                        row: ordinal,
                        reason: FoundrySkipReason::RowKindOutOfScope,
                        detail: format!("{kind} rows are out of scope for summary translation"),
                    }),
                }
            }
        }
    }

    let entries = builders
        .into_iter()
        .map(|(target, builder)| {
            let arity = declared_arity
                .get(&target)
                .copied()
                .unwrap_or_else(|| target.signature.arity());
            builder.finish(FoundryCorpus::Codeql, target, arity)
        })
        .collect::<Vec<_>>();
    skips.sort();
    Ok(CodeqlTranslation {
        entries,
        files_read: files.len() as u32,
        rows_by_kind,
        skips,
    })
}

#[allow(clippy::too_many_arguments)]
fn translate_summary_row(
    file: &str,
    ordinal: u32,
    row: &[MadCell],
    builders: &mut BTreeMap<FoundryTarget, FoundryEntryBuilder>,
    declared_arity: &mut BTreeMap<FoundryTarget, Option<u32>>,
    skips: &mut Vec<FoundrySkip>,
) {
    let mut skip = |reason: FoundrySkipReason, detail: String| {
        skips.push(FoundrySkip {
            file: file.to_owned(),
            row: ordinal,
            reason,
            detail,
        });
    };
    if row.len() != SUMMARY_ROW_COLUMNS {
        skip(
            FoundrySkipReason::MalformedRow,
            format!(
                "summaryModel row has {} columns, expected {SUMMARY_ROW_COLUMNS}",
                row.len()
            ),
        );
        return;
    }
    let package = row[0].text();
    let type_name = row[1].text();
    let Some(subtypes) = row[2].boolean() else {
        skip(
            FoundrySkipReason::MalformedRow,
            format!("subtypes column is not a boolean: {}", row[2].text()),
        );
        return;
    };
    let name = row[3].text();
    let signature_text = row[4].text();
    let extension_qualifier = row[5].text();
    let input_text = row[6].text();
    let output_text = row[7].text();
    let flow_kind = row[8].text();
    let provenance = row[9].text();

    if !extension_qualifier.is_empty() {
        skip(
            FoundrySkipReason::RowQualifierUnsupported,
            format!("row selects a declaration through ext `{extension_qualifier}`"),
        );
        return;
    }
    let signature = match parse_signature(signature_text) {
        Ok(signature) => signature,
        Err(message) => {
            skip(FoundrySkipReason::MalformedRow, message);
            return;
        }
    };
    let input = match parse_access_path(input_text) {
        Ok(path) => path,
        Err(message) => {
            skip(FoundrySkipReason::MalformedRow, message);
            return;
        }
    };
    let output = match parse_access_path(output_text) {
        Ok(path) => path,
        Err(message) => {
            skip(FoundrySkipReason::MalformedRow, message);
            return;
        }
    };
    if !input.components.is_empty() || !output.components.is_empty() {
        skip(
            FoundrySkipReason::AccessPathUnsupported,
            format!("access path `{input_text}` -> `{output_text}` has no authored spelling"),
        );
        return;
    }
    let inputs = match &input.root {
        MadRoot::ReturnValue => {
            skip(
                FoundrySkipReason::InputPortUnsupported,
                "the authored IR has no return-value input".to_owned(),
            );
            return;
        }
        MadRoot::Argument(items) => items
            .iter()
            .flat_map(|item| match item {
                MadArgumentItem::This => vec![AuthoredSummaryInput::Receiver {}],
                MadArgumentItem::Index(index) => {
                    vec![AuthoredSummaryInput::Parameter { ordinal: *index }]
                }
                MadArgumentItem::Range(low, high) => (*low..=*high)
                    .map(|index| AuthoredSummaryInput::Parameter { ordinal: index })
                    .collect(),
            })
            .collect::<Vec<_>>(),
    };
    let authored_output = match &output.root {
        MadRoot::ReturnValue => AuthoredSummaryOutput::NormalReturn {},
        MadRoot::Argument(items) if items.as_slice() == [MadArgumentItem::This] => {
            AuthoredSummaryOutput::Receiver {}
        }
        MadRoot::Argument(_) => {
            skip(
                FoundrySkipReason::OutputPortUnsupported,
                format!("the authored IR has no parameter output for `{output_text}`"),
            );
            return;
        }
    };

    let member = member_name(type_name, name);
    let target = FoundryTarget {
        artifact_path: class_file_path(package, type_name),
        member,
        signature,
    };
    if let Some(arity) = target.signature.arity() {
        for input in &inputs {
            if let AuthoredSummaryInput::Parameter { ordinal: index } = input
                && *index >= arity
            {
                skip(
                    FoundrySkipReason::OrdinalOutOfRange,
                    format!("`{input_text}` exceeds the arity of `{signature_text}`"),
                );
                return;
            }
        }
    }
    declared_arity.insert(target.clone(), target.signature.arity());
    let builder = builders.entry(target).or_default();
    for input in inputs {
        builder.add_transfer(AuthoredSummaryTransfer {
            input,
            exit_kind: AuthoredSummaryExitKind::Normal,
            output: authored_output.clone(),
        });
    }
    builder.add_evidence(FoundryEvidence {
        file: file.to_owned(),
        row: ordinal,
        text: format!(
            "{package}.{type_name}#{name}{signature_text} {input_text} -> {output_text} {flow_kind} {provenance}"
        ),
    });
    if subtypes {
        builder.add_note(FoundryNote::SubtypeClaimNotCarried);
    }
    if !signature_text.is_empty() {
        builder.add_note(FoundryNote::SignatureTypesUnqualified);
    }
}

fn translate_neutral_row(
    file: &str,
    ordinal: u32,
    row: &[MadCell],
    builders: &mut BTreeMap<FoundryTarget, FoundryEntryBuilder>,
    declared_arity: &mut BTreeMap<FoundryTarget, Option<u32>>,
    skips: &mut Vec<FoundrySkip>,
) {
    if row.len() != NEUTRAL_ROW_COLUMNS {
        skips.push(FoundrySkip {
            file: file.to_owned(),
            row: ordinal,
            reason: FoundrySkipReason::MalformedRow,
            detail: format!(
                "neutralModel row has {} columns, expected {NEUTRAL_ROW_COLUMNS}",
                row.len()
            ),
        });
        return;
    }
    let package = row[0].text();
    let type_name = row[1].text();
    let name = row[2].text();
    let signature_text = row[3].text();
    let neutral_kind = row[4].text();
    let provenance = row[5].text();
    if neutral_kind != "summary" {
        skips.push(FoundrySkip {
            file: file.to_owned(),
            row: ordinal,
            reason: FoundrySkipReason::RowKindOutOfScope,
            detail: format!("neutral `{neutral_kind}` rows are out of scope"),
        });
        return;
    }
    let signature = match parse_signature(signature_text) {
        Ok(signature) => signature,
        Err(message) => {
            skips.push(FoundrySkip {
                file: file.to_owned(),
                row: ordinal,
                reason: FoundrySkipReason::MalformedRow,
                detail: message,
            });
            return;
        }
    };
    let target = FoundryTarget {
        artifact_path: class_file_path(package, type_name),
        member: member_name(type_name, name),
        signature,
    };
    declared_arity.insert(target.clone(), target.signature.arity());
    let builder = builders.entry(target).or_default();
    builder.declare_no_flow();
    builder.add_evidence(FoundryEvidence {
        file: file.to_owned(),
        row: ordinal,
        text: format!("neutral {package}.{type_name}#{name}{signature_text} summary {provenance}"),
    });
}

/// The member name a JVM class file carries.
///
/// Models-as-Data names a constructor after its type, including the innermost
/// segment of a nested type. A class file names every constructor `<init>`,
/// which is also how Joern spells it, so both corpora reduce to one key.
fn member_name(type_name: &str, name: &str) -> String {
    if name == type_name || name == nested_type_simple_name(type_name) {
        "<init>".to_owned()
    } else {
        name.to_owned()
    }
}

/// The root port of a Models-as-Data access path.
///
/// `Argument[..]` selects a set: `Argument[this,0]` and `Argument[0..1]` both
/// name more than one port in one row.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MadRoot {
    Argument(Vec<MadArgumentItem>),
    ReturnValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MadArgumentItem {
    This,
    Index(u32),
    Range(u32, u32),
}

/// A parsed Models-as-Data access path: one root port and its qualifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MadAccessPath {
    root: MadRoot,
    components: Vec<MadComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MadComponent {
    name: String,
    argument: Option<String>,
}

/// Parse `Argument[0].SyntheticField[java.lang.Throwable.message]` and friends.
fn parse_access_path(text: &str) -> Result<MadAccessPath, String> {
    let mut components = Vec::new();
    let mut rest = text;
    loop {
        let (component, remainder) = parse_component(rest)?;
        components.push(component);
        match remainder.strip_prefix('.') {
            Some(next) => rest = next,
            None => {
                if remainder.is_empty() {
                    break;
                }
                return Err(format!("unexpected `{remainder}` in access path `{text}`"));
            }
        }
    }
    let mut components = components.into_iter();
    let head = components
        .next()
        .expect("parse_component produces at least one component");
    let root = match head.name.as_str() {
        "Argument" => {
            let argument = head
                .argument
                .ok_or_else(|| format!("`Argument` has no index in `{text}`"))?;
            MadRoot::Argument(parse_argument_set(&argument, text)?)
        }
        "ReturnValue" if head.argument.is_none() => MadRoot::ReturnValue,
        other => return Err(format!("unknown access-path root `{other}` in `{text}`")),
    };
    Ok(MadAccessPath {
        root,
        components: components.collect(),
    })
}

fn parse_component(text: &str) -> Result<(MadComponent, &str), String> {
    let name_end = text
        .find(['.', '['])
        .filter(|index| *index > 0)
        .unwrap_or(text.len());
    if name_end == 0 {
        return Err(format!("empty access-path component in `{text}`"));
    }
    let name = text[..name_end].to_owned();
    let rest = &text[name_end..];
    let Some(inner_start) = rest.strip_prefix('[') else {
        return Ok((
            MadComponent {
                name,
                argument: None,
            },
            rest,
        ));
    };
    let mut depth = 1usize;
    for (index, character) in inner_start.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((
                        MadComponent {
                            name,
                            argument: Some(inner_start[..index].to_owned()),
                        },
                        &inner_start[index + 1..],
                    ));
                }
            }
            _ => {}
        }
    }
    Err(format!("unterminated `[` in access path `{text}`"))
}

fn parse_argument_set(argument: &str, text: &str) -> Result<Vec<MadArgumentItem>, String> {
    split_parameter_types(argument)
        .iter()
        .map(|item| parse_argument_item(item, text))
        .collect()
}

fn parse_argument_item(argument: &str, text: &str) -> Result<MadArgumentItem, String> {
    if argument == "this" {
        return Ok(MadArgumentItem::This);
    }
    if let Some((low, high)) = argument.split_once("..") {
        let low = parse_ordinal(low, text)?;
        let high = parse_ordinal(high, text)?;
        if high < low {
            return Err(format!("argument range `{argument}` is empty in `{text}`"));
        }
        return Ok(MadArgumentItem::Range(low, high));
    }
    Ok(MadArgumentItem::Index(parse_ordinal(argument, text)?))
}

fn parse_ordinal(value: &str, text: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|error| format!("argument ordinal `{value}` in `{text}` is not a count: {error}"))
}

/// Parse a Models-as-Data signature column.
///
/// An empty column states the claim for every overload of the member. A
/// present column is a parenthesised list of erased parameter types.
fn parse_signature(text: &str) -> Result<FoundrySignature, String> {
    if text.is_empty() {
        return Ok(FoundrySignature::AnyOverload);
    }
    let inner = text
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .ok_or_else(|| format!("signature `{text}` is not parenthesised"))?;
    Ok(FoundrySignature::Overload {
        types: split_parameter_types(inner),
    })
}

/// Render a translation's skip counts, for the report and for logging.
pub fn skip_counts(skips: &[FoundrySkip]) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for skip in skips {
        *counts.entry(skip.reason.as_str().to_owned()).or_default() += 1;
    }
    counts
}

/// A stable digest over the exact corpus bytes a translation read.
pub fn corpus_digest(files: &[(String, Vec<u8>)]) -> String {
    use sha2::{Digest, Sha256};

    let mut lines = files
        .iter()
        .map(|(name, bytes)| {
            let mut file_digest = Sha256::new();
            file_digest.update(bytes);
            let mut hex = String::with_capacity(64);
            for byte in file_digest.finalize() {
                write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
            }
            format!("{name} {hex}")
        })
        .collect::<Vec<_>>();
    lines.sort();
    let mut digest = Sha256::new();
    for line in &lines {
        digest.update(line.as_bytes());
        digest.update(b"\n");
    }
    let mut hex = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary_foundry::ir::FoundryClaim;

    const SLICE: &str = include_str!("../../testdata/summary-corpora/codeql/java.lang.model.yml");

    fn slice_files() -> Vec<(String, Vec<u8>)> {
        vec![("java.lang.model.yml".to_owned(), SLICE.as_bytes().to_vec())]
    }

    fn entry<'a>(translation: &'a CodeqlTranslation, symbol: &str) -> &'a FoundryEntry {
        translation
            .entries
            .iter()
            .find(|entry| entry.target.signature.symbol(&entry.target.member) == symbol)
            .unwrap_or_else(|| {
                panic!(
                    "no entry for {symbol}; have {:#?}",
                    translation
                        .entries
                        .iter()
                        .map(|entry| entry.target.signature.symbol(&entry.target.member))
                        .collect::<Vec<_>>()
                )
            })
    }

    #[test]
    fn access_paths_parse_into_root_and_qualifiers() {
        assert_eq!(
            parse_access_path("Argument[this]").unwrap(),
            MadAccessPath {
                root: MadRoot::Argument(vec![MadArgumentItem::This]),
                components: Vec::new(),
            }
        );
        assert_eq!(
            parse_access_path("Argument[0..1]").unwrap().root,
            MadRoot::Argument(vec![MadArgumentItem::Range(0, 1)])
        );
        assert_eq!(
            parse_access_path("Argument[this,0]").unwrap().root,
            MadRoot::Argument(vec![MadArgumentItem::This, MadArgumentItem::Index(0)])
        );
        let qualified = parse_access_path("Argument[this].SyntheticField[java.lang.T.message]")
            .expect("qualified path parses");
        assert_eq!(
            qualified.root,
            MadRoot::Argument(vec![MadArgumentItem::This])
        );
        assert_eq!(
            qualified.components,
            vec![MadComponent {
                name: "SyntheticField".to_owned(),
                argument: Some("java.lang.T.message".to_owned()),
            }]
        );
        assert!(parse_access_path("Receiver[0]").is_err());
    }

    #[test]
    fn signatures_split_on_top_level_commas_only() {
        assert_eq!(parse_signature("").unwrap(), FoundrySignature::AnyOverload);
        assert_eq!(
            parse_signature("()").unwrap(),
            FoundrySignature::Overload { types: Vec::new() }
        );
        assert_eq!(
            parse_signature("(Locale,String,Object[])").unwrap(),
            FoundrySignature::Overload {
                types: vec![
                    "Locale".to_owned(),
                    "String".to_owned(),
                    "Object[]".to_owned()
                ],
            }
        );
        assert_eq!(
            split_parameter_types("Map<String,Object>,int"),
            vec!["Map<String,Object>".to_owned(), "int".to_owned()]
        );
    }

    #[test]
    fn summary_rows_become_transfers_on_the_named_target() {
        let translation = translate_codeql_models(&slice_files()).expect("slice parses");

        let concat = entry(&translation, "concat(String)");
        assert_eq!(concat.claim, FoundryClaim::Flows);
        assert_eq!(
            concat.rendered_transfers(),
            vec![
                "parameter[0]->normal_return@normal".to_owned(),
                "receiver->normal_return@normal".to_owned(),
            ]
        );
        assert!(concat.boundary.has_receiver);
        assert_eq!(concat.boundary.parameter_count, 1);
        assert_eq!(concat.target.artifact_path, "java/lang/String.class");
        assert_eq!(concat.evidence.len(), 2);
    }

    #[test]
    fn an_argument_range_expands_to_one_transfer_per_ordinal() {
        let translation = translate_codeql_models(&slice_files()).expect("slice parses");

        let join = entry(&translation, "join");
        assert_eq!(
            join.rendered_transfers(),
            vec![
                "parameter[0]->normal_return@normal".to_owned(),
                "parameter[1]->normal_return@normal".to_owned(),
            ]
        );
        assert!(
            join.notes
                .contains(&FoundryNote::ParameterCountIsLowerBound { observed: 2 })
        );
    }

    #[test]
    fn a_constructor_row_lands_on_the_class_file_member_name() {
        let translation = translate_codeql_models(&slice_files()).expect("slice parses");

        let constructor = entry(&translation, "<init>(String)");
        assert_eq!(
            constructor.rendered_transfers(),
            vec!["parameter[0]->receiver@normal".to_owned()]
        );
        assert!(
            constructor
                .notes
                .contains(&FoundryNote::SubtypeClaimNotCarried)
        );
    }

    #[test]
    fn a_neutral_summary_row_is_a_no_flow_claim_without_an_authored_spelling() {
        let translation = translate_codeql_models(&slice_files()).expect("slice parses");

        let neutral = entry(&translation, "length()");
        assert_eq!(neutral.claim, FoundryClaim::NoFlow);
        assert!(neutral.transfers.is_empty());
        assert!(neutral.authored_summary().is_none());
        assert!(neutral.notes.contains(&FoundryNote::NoFlowClaimNotCarried));
    }

    #[test]
    fn unrepresentable_rows_are_counted_by_typed_reason() {
        let translation = translate_codeql_models(&slice_files()).expect("slice parses");
        let counts = skip_counts(&translation.skips);

        assert_eq!(counts.get("access_path_unsupported"), Some(&2));
        assert_eq!(counts.get("output_port_unsupported"), Some(&1));
        assert_eq!(counts.get("row_kind_out_of_scope"), Some(&3));
        assert_eq!(translation.rows_by_kind.get("sinkModel"), Some(&1));
        assert_eq!(translation.rows_by_kind.get("sourceModel"), Some(&1));
    }

    #[test]
    fn translation_is_deterministic() {
        let first = translate_codeql_models(&slice_files()).expect("slice parses");
        let second = translate_codeql_models(&slice_files()).expect("slice parses");

        assert_eq!(first, second);
    }
}
