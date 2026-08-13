use super::*;
use brokk_bifrost_analysis::analyzer::semantic::{
    SemanticBudget, SemanticOutcome, SemanticRequest,
};
use brokk_bifrost_analysis::analyzer::structural::{
    CodeQueryCompletion, CodeQueryExecutionLimits, execute_workspace_request_with_cancellation,
};
use brokk_bifrost_analysis::analyzer::{
    AnalyzerConfig, FilesystemProject, OverlayProject, Project, ProjectFile, WorkspaceAnalyzer,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fmt, path::PathBuf, sync::Arc};

#[derive(Debug, Clone)]
pub struct ExtensionWorkspaceOptions {
    pub roots: Vec<PathBuf>,
    pub analyzer_config: AnalyzerConfig,
    pub limits: ExtensionLimits,
}
impl ExtensionWorkspaceOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            roots: vec![root.into()],
            analyzer_config: AnalyzerConfig::default(),
            limits: ExtensionLimits::default(),
        }
    }
}

#[derive(Debug)]
pub enum ExtensionWorkspaceError {
    InvalidRoots(Box<str>),
    Project(Box<str>),
    Analyzer(Box<str>),
}
impl fmt::Display for ExtensionWorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ExtensionWorkspaceError {}
#[derive(Debug)]
pub enum ExtensionError {
    Compatibility(ExtensionCompatibilityError),
    StaleGeneration {
        expected: WorkspaceGeneration,
        actual: WorkspaceGeneration,
    },
    InvalidRequest(Box<str>),
    Execution(Box<str>),
}
impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ExtensionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Complete,
    Partial,
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageCapabilityReport {
    pub language: Box<str>,
    pub control_flow: CapabilitySupport,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationCapability {
    pub id: ExtensionCapabilityId,
    pub stability: ApiStability,
    pub support: CapabilitySupport,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionCapabilityReport {
    pub generation: WorkspaceGeneration,
    pub languages: Box<[LanguageCapabilityReport]>,
    pub operations: Box<[OperationCapability]>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionWorkspaceDescription {
    pub api: ExtensionApiVersion,
    pub generation: WorkspaceGeneration,
    pub capabilities: ExtensionCapabilityReport,
}

pub struct ExtensionWorkspace {
    generation: WorkspaceGeneration,
    capabilities: ExtensionCapabilityReport,
    analyzer: WorkspaceAnalyzer,
}

impl ExtensionWorkspace {
    pub fn open(options: ExtensionWorkspaceOptions) -> Result<Self, ExtensionWorkspaceError> {
        if options.roots.len() != 1 {
            return Err(ExtensionWorkspaceError::InvalidRoots(
                "API version 1 requires exactly one workspace root".into(),
            ));
        }
        let filesystem = FilesystemProject::new(options.roots[0].clone()).map_err(|error| {
            ExtensionWorkspaceError::Project(error.to_string().into_boxed_str())
        })?;
        let filesystem: Arc<dyn Project> = Arc::new(filesystem);
        let files = filesystem.all_files_shared().map_err(|error| {
            ExtensionWorkspaceError::Project(error.to_string().into_boxed_str())
        })?;
        let frozen = OverlayProject::with_max_bytes(
            Arc::clone(&filesystem),
            options.limits.values().source_bytes as usize,
        );
        for file in files.iter() {
            let source = filesystem.read_source(file).map_err(|error| {
                ExtensionWorkspaceError::Project(error.to_string().into_boxed_str())
            })?;
            if !frozen.set(file.abs_path(), source) {
                return Err(ExtensionWorkspaceError::Project(
                    format!(
                        "source exceeds extension open limit: {}",
                        file.rel_path().display()
                    )
                    .into_boxed_str(),
                ));
            }
        }
        let project: Arc<dyn Project> = Arc::new(frozen.snapshot());
        let analyzer = WorkspaceAnalyzer::build_ephemeral(
            Arc::clone(&project),
            options.analyzer_config.clone(),
        )
        .map_err(|error| ExtensionWorkspaceError::Analyzer(error.to_string().into_boxed_str()))?;
        let generation = generation_for(&analyzer, &options.analyzer_config)?;
        let languages = analyzer
            .analyzer()
            .languages()
            .into_iter()
            .map(|language| LanguageCapabilityReport {
                language: format!("{language:?}")
                    .to_ascii_lowercase()
                    .into_boxed_str(),
                control_flow: CapabilitySupport::Complete,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let capabilities = ExtensionCapabilityReport {
            generation: generation.clone(),
            languages,
            operations: vec![
                OperationCapability {
                    id: capability("structural.query"),
                    stability: ApiStability::Stable,
                    support: CapabilitySupport::Complete,
                },
                OperationCapability {
                    id: capability("experimental.semantic.control_flow"),
                    stability: ApiStability::Experimental { since_minor: 0 },
                    support: CapabilitySupport::Complete,
                },
            ]
            .into_boxed_slice(),
        };
        Ok(Self {
            generation,
            capabilities,
            analyzer,
        })
    }
    pub fn generation(&self) -> &WorkspaceGeneration {
        &self.generation
    }
    pub fn capabilities(&self) -> &ExtensionCapabilityReport {
        &self.capabilities
    }
    pub fn describe(&self) -> ExtensionWorkspaceDescription {
        ExtensionWorkspaceDescription {
            api: EXTENSION_API_VERSION,
            generation: self.generation.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
    fn validate(
        &self,
        compatibility: &ExtensionCompatibility,
        expected: &WorkspaceGeneration,
    ) -> Result<(), ExtensionError> {
        negotiate_extension_api(compatibility).map_err(ExtensionError::Compatibility)?;
        if expected != &self.generation {
            return Err(ExtensionError::StaleGeneration {
                expected: expected.clone(),
                actual: self.generation.clone(),
            });
        }
        Ok(())
    }
    pub fn structural_query(
        &self,
        request: StructuralRequest,
        cancellation: &ExtensionCancellation,
    ) -> Result<ExtensionOutcome<StructuralResult>, ExtensionError> {
        self.validate(&request.compatibility, &request.expected_generation)?;
        let values = request.limits.values();
        if cancellation.is_cancelled() {
            return Ok(make_outcome(
                None,
                ExtensionCompletion::Cancelled,
                &self.generation,
                &request.limits,
                "structural.query",
                ApiStability::Stable,
                ExtensionWork::default(),
            ));
        }
        let limits = CodeQueryExecutionLimits {
            max_pipeline_rows: values.result_items as usize,
            max_scanned_source_bytes: values.source_bytes as usize,
            max_scanned_files: values.semantic_files as usize,
            ..Default::default()
        };
        let response = execute_workspace_request_with_cancellation(
            &self.analyzer,
            &request.query,
            limits,
            cancellation.token(),
        );
        let result = response.result().ok_or_else(|| {
            ExtensionError::InvalidRequest("explain requests do not produce structural rows".into())
        })?;
        let completion = match result.completion() {
            CodeQueryCompletion::Complete => ExtensionCompletion::Complete,
            CodeQueryCompletion::ProvenSubset { .. } => ExtensionCompletion::Unproven,
            CodeQueryCompletion::Incomplete { .. } => ExtensionCompletion::Truncated {
                limit: "structural_execution".into(),
            },
            CodeQueryCompletion::Cancelled => ExtensionCompletion::Cancelled,
            CodeQueryCompletion::Invalid { .. } => {
                return Err(ExtensionError::InvalidRequest(
                    "invalid structural query".into(),
                ));
            }
        };
        let items = result
            .results
            .iter()
            .map(|item| serde_json::to_value(item).expect("query result item serializes"))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let value = StructuralResult { items };
        let work = ExtensionWork {
            result_items: value.items.len() as u64,
            result_bytes: serde_json::to_vec(&value)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or(0),
            ..Default::default()
        };
        Ok(make_outcome(
            Some(value),
            completion,
            &self.generation,
            &request.limits,
            "structural.query",
            ApiStability::Stable,
            work,
        ))
    }
    pub fn semantic_relations(
        &self,
        request: SemanticRelationRequest,
        cancellation: &ExtensionCancellation,
    ) -> Result<ExtensionOutcome<SemanticRelationSnapshot>, ExtensionError> {
        self.validate(&request.compatibility, &request.expected_generation)?;
        request
            .seed
            .validate()
            .map_err(|error| ExtensionError::InvalidRequest(error.into_boxed_str()))?;
        let stability = ApiStability::Experimental { since_minor: 0 };
        let operation = "experimental.semantic.control_flow";
        if cancellation.is_cancelled() {
            return Ok(make_outcome(
                None,
                ExtensionCompletion::Cancelled,
                &self.generation,
                &request.limits,
                operation,
                stability,
                ExtensionWork::default(),
            ));
        }
        let root = self.analyzer.analyzer().project().root();
        let file = ProjectFile::new(root.to_path_buf(), request.seed.path.as_str());
        if self
            .analyzer
            .program_semantics_provider_for_file(&file)
            .is_none()
        {
            return Ok(make_outcome(
                None,
                ExtensionCompletion::Unsupported {
                    capability: capability(operation),
                },
                &self.generation,
                &request.limits,
                operation,
                stability,
                ExtensionWork::default(),
            ));
        }
        let values = request.limits.values();
        let mut budget = SemanticBudget::default();
        let mut semantic_request = SemanticRequest::new(&mut budget, cancellation.token());
        let materialized = self
            .analyzer
            .materialize_program_semantics(&file, &mut semantic_request)
            .map_err(|error| ExtensionError::Execution(error.to_string().into_boxed_str()))?;
        let completion = semantic_completion(&materialized);
        let Some(artifact) = materialized.available_value() else {
            return Ok(make_outcome(
                None,
                completion,
                &self.generation,
                &request.limits,
                operation,
                stability,
                ExtensionWork::default(),
            ));
        };
        let seed_start = u32::try_from(request.seed.start_utf8_byte)
            .map_err(|_| ExtensionError::InvalidRequest("seed byte offset exceeds u32".into()))?;
        let procedure = artifact
            .procedures()
            .iter()
            .filter(|procedure| {
                let span = procedure.locator().anchor().span();
                span.start_byte() <= seed_start && seed_start <= span.end_byte()
            })
            .min_by_key(|procedure| {
                procedure.locator().anchor().span().end_byte()
                    - procedure.locator().anchor().span().start_byte()
            });
        let Some(procedure) = procedure else {
            return Ok(make_outcome(
                None,
                ExtensionCompletion::Unknown,
                &self.generation,
                &request.limits,
                operation,
                stability,
                ExtensionWork::default(),
            ));
        };
        let truncated = procedure.points().len() > values.semantic_nodes as usize
            || procedure.control_edges().len() > values.semantic_edges as usize;
        let nodes = procedure
            .points()
            .iter()
            .take(values.semantic_nodes as usize)
            .map(|point| {
                let mapping = procedure
                    .source_mapping(point.source)
                    .expect("validated semantic source mapping");
                let span = mapping.locator.anchor().span();
                SemanticNodeOccurrence {
                    id: stable_semantic_id(
                        &self.generation,
                        request.seed.path.as_str(),
                        span.start_byte(),
                        span.end_byte(),
                        point.id.index(),
                    ),
                    span: SourceSpan {
                        path: request.seed.path.clone(),
                        start_utf8_byte: span.start_byte() as u64,
                        end_utf8_byte: span.end_byte() as u64,
                    },
                    role: "program_point".into(),
                }
            })
            .collect::<Vec<_>>();
        let edges = procedure
            .control_edges()
            .iter()
            .take(values.semantic_edges as usize)
            .map(|edge| SemanticRelationEdge {
                source: edge.source_point.index() as u32,
                target: edge.target_point.index() as u32,
                kind: edge.kind.label().into(),
            })
            .collect::<Vec<_>>();
        let work = ExtensionWork {
            semantic_nodes: nodes.len() as u64,
            semantic_edges: edges.len() as u64,
            ..Default::default()
        };
        let snapshot = SemanticRelationSnapshot {
            nodes: nodes.into_boxed_slice(),
            edges: edges.into_boxed_slice(),
            boundaries: if truncated {
                vec!["limit".into()].into_boxed_slice()
            } else {
                Box::new([])
            },
        };
        Ok(make_outcome(
            Some(snapshot),
            if truncated {
                ExtensionCompletion::Truncated {
                    limit: "semantic_nodes_or_edges".into(),
                }
            } else {
                completion
            },
            &self.generation,
            &request.limits,
            operation,
            stability,
            work,
        ))
    }
}

fn generation_for(
    analyzer: &WorkspaceAnalyzer,
    config: &AnalyzerConfig,
) -> Result<WorkspaceGeneration, ExtensionWorkspaceError> {
    let mut hasher = Sha256::new();
    hasher.update(b"brokk-bifrost-extension-workspace-generation-v1\0");
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update(EXTENSION_API_VERSION.major.to_le_bytes());
    hasher.update(format!("{config:?}").as_bytes());
    let project = analyzer.analyzer().project();
    hasher.update(project.root().to_string_lossy().as_bytes());
    for file in project
        .all_files_shared()
        .map_err(|error| ExtensionWorkspaceError::Project(error.to_string().into_boxed_str()))?
        .iter()
    {
        hasher.update([0]);
        hasher.update(file.rel_path().to_string_lossy().as_bytes());
        let source = project.read_source_snapshot(file).map_err(|error| {
            ExtensionWorkspaceError::Project(error.to_string().into_boxed_str())
        })?;
        hasher.update([0]);
        hasher.update(source.source().as_bytes());
    }
    Ok(WorkspaceGeneration::new(
        StableDigest::parse(format!("{:x}", hasher.finalize())).expect("SHA-256 is canonical"),
    ))
}
fn capability(value: &str) -> ExtensionCapabilityId {
    ExtensionCapabilityId::new(value).expect("static capability is valid")
}
fn stable_semantic_id(
    generation: &WorkspaceGeneration,
    path: &str,
    start: u32,
    end: u32,
    local: usize,
) -> StableDigest {
    StableDigest::from_hash(format!(
        "semantic-node-v1\0{generation}\0{path}\0{start}\0{end}\0{local}"
    ))
}
fn semantic_completion<T>(outcome: &SemanticOutcome<T>) -> ExtensionCompletion {
    match outcome {
        SemanticOutcome::Complete { .. } => ExtensionCompletion::Complete,
        SemanticOutcome::Ambiguous { .. } => ExtensionCompletion::Ambiguous,
        SemanticOutcome::Unknown { .. } => ExtensionCompletion::Unknown,
        SemanticOutcome::Unsupported { capability, .. } => ExtensionCompletion::Unsupported {
            capability: ExtensionCapabilityId::new(format!("semantic.{}", capability.label()))
                .unwrap(),
        },
        SemanticOutcome::Unproven { .. } => ExtensionCompletion::Unproven,
        SemanticOutcome::ExceededBudget { exceeded, .. } => ExtensionCompletion::ExceededBudget {
            dimension: format!("{exceeded:?}").into_boxed_str(),
        },
        SemanticOutcome::Cancelled { .. } => ExtensionCompletion::Cancelled,
    }
}
fn make_outcome<T>(
    value: Option<T>,
    completion: ExtensionCompletion,
    generation: &WorkspaceGeneration,
    limits: &ExtensionLimits,
    operation: &str,
    stability: ApiStability,
    work: ExtensionWork,
) -> ExtensionOutcome<T> {
    ExtensionOutcome {
        completion,
        value,
        metadata: ExtensionResultMetadata {
            api: EXTENSION_API_VERSION,
            operation: capability(operation),
            stability,
            generation: generation.clone(),
            diagnostics: Box::new([]),
            work,
            limits: limits.values(),
            provenance: vec![
                format!("brokk-bifrost-runtime:{}", env!("CARGO_PKG_VERSION")).into_boxed_str(),
            ]
            .into_boxed_slice(),
        },
    }
}

#[derive(Debug, Clone)]
pub struct StructuralRequest {
    pub compatibility: ExtensionCompatibility,
    pub expected_generation: WorkspaceGeneration,
    pub query: CodeQuery,
    pub limits: ExtensionLimits,
}
impl Serialize for StructuralRequest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (
            &self.compatibility,
            &self.expected_generation,
            self.query.to_canonical_json(),
            &self.limits,
        )
            .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for StructuralRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (compatibility, expected_generation, query, limits): (
            ExtensionCompatibility,
            WorkspaceGeneration,
            Value,
            ExtensionLimits,
        ) = Deserialize::deserialize(deserializer)?;
        let query = CodeQuery::from_json(&query).map_err(serde::de::Error::custom)?;
        Ok(Self {
            compatibility,
            expected_generation,
            query,
            limits,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuralResult {
    pub items: Box<[Value]>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticRelationRequest {
    pub compatibility: ExtensionCompatibility,
    pub expected_generation: WorkspaceGeneration,
    pub seed: SourceSpan,
    pub limits: ExtensionLimits,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticNodeOccurrence {
    pub id: StableDigest,
    pub span: SourceSpan,
    pub role: Box<str>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticRelationEdge {
    pub source: u32,
    pub target: u32,
    pub kind: Box<str>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticRelationSnapshot {
    pub nodes: Box<[SemanticNodeOccurrence]>,
    pub edges: Box<[SemanticRelationEdge]>,
    pub boundaries: Box<[Box<str>]>,
}
