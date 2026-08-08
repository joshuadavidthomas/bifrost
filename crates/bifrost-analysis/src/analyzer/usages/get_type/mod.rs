use crate::analyzer::common::language_for_file;
use crate::analyzer::languages::{BoundedReceiverQuery, language_support};
use crate::analyzer::usages::get_definition::{
    BoundedResolution, java::JavaResolutionSession, parse_tree_for_language,
};
use crate::analyzer::usages::receiver_analysis::{
    INTERACTIVE_TYPE_LOOKUP_BUDGET, ReceiverAnalysisBudget, ReceiverBudgetLimit,
};
use crate::analyzer::usages::reference_site::{
    ResolvedReferenceSite, SourceLocationRequest, resolve_reference_site,
};
use crate::analyzer::usages::target_kind::TypeLookupTargetKind;
use crate::analyzer::{AnalyzerDefinitionLookup, CodeUnit, IAnalyzer, Language, ProjectFile};
use crate::hash::{HashMap, HashSet};
use crate::path_utils::rel_path_string;
use std::sync::Arc;
use tree_sitter::Tree;

mod cpp;
mod csharp;
mod go;
pub(crate) mod java;
mod js_ts;
mod kotlin;
mod php;
mod python;
mod ruby;
mod rust;
mod scala;

pub(crate) use cpp::resolve_cpp_type_bounded;
pub(crate) use csharp::resolve_csharp_type_bounded;
pub(crate) use go::resolve_go_type_bounded;
pub(crate) use kotlin::resolve_kotlin_type_bounded;
pub(crate) use php::resolve_php_type_bounded;
pub(crate) use python::resolve_python_type_bounded;
pub(crate) use ruby::resolve_ruby_type_bounded;
pub(crate) use rust::resolve_rust_type_bounded;
pub(crate) use scala::resolve_scala_type_bounded;

#[derive(Debug, Clone)]
pub struct TypeLookupRequest {
    pub file: ProjectFile,
    pub source: Option<Arc<String>>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TypeLookupOutcome {
    pub(crate) status: TypeLookupStatus,
    pub(crate) reference: Option<ResolvedReferenceSite>,
    pub types: Vec<TypeLookupType>,
    pub(crate) diagnostics: Vec<TypeLookupDiagnostic>,
    pub target_kind: TypeLookupTargetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeLookupStatus {
    Resolved,
    NoType,
    Ambiguous,
    UnsupportedLanguage,
    InvalidLocation,
    NotFound,
    /// Bounded resolution stopped on the named budget axis before finishing.
    /// An incomplete answer, not a proven "no type": the workspace may still
    /// hold a type for the reference.
    ExceededBudget(ReceiverBudgetLimit),
}

impl TypeLookupStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::NoType => "no_type",
            Self::Ambiguous => "ambiguous",
            Self::UnsupportedLanguage => "unsupported_language",
            Self::InvalidLocation => "invalid_location",
            Self::NotFound => "not_found",
            Self::ExceededBudget(_) => "exceeded_budget",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TypeLookupDiagnostic {
    pub(crate) kind: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub struct TypeLookupType {
    pub fqn: String,
    pub definitions: Vec<CodeUnit>,
}

pub fn resolve_type_batch(
    analyzer: &dyn IAnalyzer,
    requests: Vec<TypeLookupRequest>,
) -> Vec<TypeLookupOutcome> {
    resolve_type_batch_with_budget(analyzer, requests, INTERACTIVE_TYPE_LOOKUP_BUDGET)
}

fn resolve_type_batch_with_budget(
    analyzer: &dyn IAnalyzer,
    requests: Vec<TypeLookupRequest>,
    budget: ReceiverAnalysisBudget,
) -> Vec<TypeLookupOutcome> {
    let mut context = TypeBatchContext::new(analyzer);
    requests
        .into_iter()
        .map(|request| resolve_one(analyzer, &mut context, request, budget))
        .collect()
}

struct TypeBatchContext<'a> {
    sources: HashMap<ProjectFile, Result<Arc<String>, String>>,
    trees: HashMap<(ProjectFile, Language), Option<Tree>>,
    support: AnalyzerDefinitionLookup<'a>,
}

impl<'a> TypeBatchContext<'a> {
    fn new(analyzer: &'a dyn IAnalyzer) -> Self {
        Self {
            sources: HashMap::default(),
            trees: HashMap::default(),
            support: AnalyzerDefinitionLookup::new(analyzer, Language::None),
        }
    }

    fn source(&mut self, file: &ProjectFile) -> Result<Arc<String>, String> {
        self.sources
            .entry(file.clone())
            .or_insert_with(|| {
                file.read_to_string()
                    .map(Arc::new)
                    .map_err(|err| format!("failed to read `{}`: {err}", rel_path_string(file)))
            })
            .clone()
    }

    fn tree(&mut self, file: &ProjectFile, language: Language, source: &str) -> Option<Tree> {
        self.trees
            .entry((file.clone(), language))
            .or_insert_with(|| parse_tree_for_type_lookup(file, language, source))
            .clone()
    }
}

fn resolve_one<'a>(
    analyzer: &'a dyn IAnalyzer,
    context: &mut TypeBatchContext<'a>,
    request: TypeLookupRequest,
    budget: ReceiverAnalysisBudget,
) -> TypeLookupOutcome {
    let file = request.file.clone();
    let language = language_for_file(&file);
    let source = match request.source.clone() {
        Some(source) => source,
        None => match context.source(&file) {
            Ok(source) => source,
            Err(message) => {
                return diagnostic_outcome(TypeLookupStatus::NotFound, "file_read_failed", message);
            }
        },
    };
    let tree = if request.source.is_some() {
        parse_tree_for_type_lookup(&file, language, &source)
    } else {
        context.tree(&file, language, &source)
    };
    let site = match resolve_reference_site(
        &request.as_source_location(),
        &source,
        tree.as_ref().map(Tree::root_node),
    ) {
        Ok(site) => site,
        Err(message) => {
            return diagnostic_outcome(
                TypeLookupStatus::InvalidLocation,
                "invalid_location",
                message,
            );
        }
    };

    let Some(resolution) = bounded_type_resolution(
        analyzer,
        &context.support,
        &file,
        language,
        &source,
        tree.as_ref(),
        &site,
        budget,
    ) else {
        return finish_lookup_outcome(
            diagnostic_outcome(
                TypeLookupStatus::UnsupportedLanguage,
                "unsupported_language",
                format!("{language:?} type lookup is not implemented yet"),
            ),
            site,
        );
    };
    let outcome = match resolution {
        BoundedResolution::Complete { value, .. } => value,
        BoundedResolution::Exceeded { limit, .. } => diagnostic_outcome(
            TypeLookupStatus::ExceededBudget(limit),
            "resolution_budget_exhausted",
            format!(
                "bounded type resolution stopped after exhausting its {} budget; \
                 the result is incomplete, not a proven absence of a type",
                limit.as_str()
            ),
        ),
        BoundedResolution::Cancelled { .. } => {
            unreachable!("type lookup runs without a cancellation token")
        }
    };
    finish_lookup_outcome(outcome, site)
}

/// One location's type, resolved through the bounded receiver contract, or
/// `None` when no route serves the language at all.
///
/// The structural-receiver resolver is the primary route: the same bounded core
/// the receiver query dispatches through answers here under the caller's
/// budget. Java and JS/TS run their receiver analysis elsewhere (a Java
/// resolution session, the JS/TS syntax index), so they keep their own arms --
/// but those arms take the same budget, and no arm runs unbounded.
#[allow(clippy::too_many_arguments)]
fn bounded_type_resolution(
    analyzer: &dyn IAnalyzer,
    support: &AnalyzerDefinitionLookup<'_>,
    file: &ProjectFile,
    language: Language,
    source: &str,
    tree: Option<&Tree>,
    site: &ResolvedReferenceSite,
    budget: ReceiverAnalysisBudget,
) -> Option<BoundedResolution<TypeLookupOutcome>> {
    let language_support = language_support(language)?;
    if let Some(resolver) = language_support.structural_receiver() {
        return Some(resolver.resolve_type_bounded(BoundedReceiverQuery {
            analyzer,
            file,
            source,
            tree,
            site,
            budget,
            cancellation: None,
        }));
    }
    match language {
        Language::Java => {
            support.set_language(language);
            let session = JavaResolutionSession::bounded(support, budget, None);
            Some(java::resolve_java_type_bounded(
                analyzer, &session, file, source, tree, site,
            ))
        }
        Language::JavaScript | Language::TypeScript => {
            support.set_language(language);
            Some(js_ts::resolve_js_ts_type_bounded(
                analyzer, support, file, language, source, tree, site, budget, None,
            ))
        }
        _ => None,
    }
}

fn finish_lookup_outcome(
    mut outcome: TypeLookupOutcome,
    site: ResolvedReferenceSite,
) -> TypeLookupOutcome {
    outcome.reference = Some(site);
    outcome
}

impl TypeLookupRequest {
    fn as_source_location(&self) -> SourceLocationRequest {
        SourceLocationRequest {
            file: self.file.clone(),
            line: self.line,
            column: self.column,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
        }
    }
}

fn parse_tree_for_type_lookup(
    file: &ProjectFile,
    language: Language,
    source: &str,
) -> Option<Tree> {
    match language {
        Language::Go => {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_go::LANGUAGE.into()).ok()?;
            parser.parse(source, None)
        }
        _ => parse_tree_for_language(file, language, source),
    }
}

pub(super) fn candidates_outcome(
    fqn: impl Into<String>,
    candidates: Vec<CodeUnit>,
) -> TypeLookupOutcome {
    candidates_outcome_with_target_kind(fqn, candidates, TypeLookupTargetKind::ValueExpression)
}

pub(super) fn type_reference_outcome(
    fqn: impl Into<String>,
    candidates: Vec<CodeUnit>,
) -> TypeLookupOutcome {
    candidates_outcome_with_target_kind(fqn, candidates, TypeLookupTargetKind::TypeReference)
}

pub(super) fn candidates_outcome_with_target_kind(
    fqn: impl Into<String>,
    mut candidates: Vec<CodeUnit>,
    target_kind: TypeLookupTargetKind,
) -> TypeLookupOutcome {
    sort_units(&mut candidates);
    candidates.dedup();
    let mut semantic_keys = HashSet::default();
    for candidate in &candidates {
        semantic_keys.insert((candidate.fq_name(), candidate.source().clone()));
    }
    let status = if semantic_keys.len() <= 1 {
        TypeLookupStatus::Resolved
    } else {
        TypeLookupStatus::Ambiguous
    };
    TypeLookupOutcome {
        status,
        reference: None,
        types: vec![TypeLookupType {
            fqn: fqn.into(),
            definitions: candidates,
        }],
        diagnostics: if status == TypeLookupStatus::Ambiguous {
            vec![TypeLookupDiagnostic {
                kind: "ambiguous_type".to_string(),
                message: "reference resolved to multiple possible types".to_string(),
            }]
        } else {
            Vec::new()
        },
        target_kind,
    }
}

pub(super) fn no_type(kind: impl Into<String>, message: impl Into<String>) -> TypeLookupOutcome {
    diagnostic_outcome(TypeLookupStatus::NoType, kind, message)
}

fn diagnostic_outcome(
    status: TypeLookupStatus,
    kind: impl Into<String>,
    message: impl Into<String>,
) -> TypeLookupOutcome {
    TypeLookupOutcome {
        status,
        reference: None,
        types: Vec::new(),
        diagnostics: vec![TypeLookupDiagnostic {
            kind: kind.into(),
            message: message.into(),
        }],
        target_kind: TypeLookupTargetKind::ValueExpression,
    }
}

pub(super) fn sort_units(units: &mut [CodeUnit]) {
    units.sort_by(|left, right| {
        left.fq_name()
            .cmp(&right.fq_name())
            .then_with(|| left.source().cmp(right.source()))
    });
}
