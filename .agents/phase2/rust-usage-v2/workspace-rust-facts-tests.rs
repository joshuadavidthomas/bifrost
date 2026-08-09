// Parked verbatim by Phase 1 of `.agents/plans/port-optimization-arc-to-upstream.md`.
//
// The workspace-level pin for the usage-v2 readiness/warmth distinction, from
// `crates/bifrost-analysis/src/analyzer/workspace.rs`. Phase 1 restores
// upstream's `warm_usage_analysis`, which has no such distinction, so the two
// predicates this test names do not exist. Phase 2 restores both.
//
// This file is not a Cargo module and is never compiled.

    /// The two Rust usage predicates a caller can ask a workspace, and the
    /// distinction ExecPlan Milestone 3 introduced between them: readiness is
    /// "would a query wait", which a healthy workspace answers `true` even
    /// before any warm because v2 has nothing to build, and warmth is "has the
    /// catch-up run for this generation", which only the warm makes true.
    /// Neither may be `false` for a workspace with no Rust.
    #[test]
    fn rust_usage_readiness_and_warmth_are_distinct_and_vacuous_without_rust() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        ProjectFile::new(root.clone(), "src/lib.rs")
            .write("pub mod worker;\npub fn root() {}\n")
            .unwrap();
        ProjectFile::new(root.clone(), "src/worker.rs")
            .write("use crate::root;\npub fn run() { root(); }\n")
            .unwrap();

        let rust: Arc<dyn Project> = Arc::new(TestProject::new(root.clone(), Language::Rust));
        let rust = WorkspaceAnalyzer::build(rust, AnalyzerConfig::default());
        assert!(rust.rust_usage_facts_ready());
        assert!(!rust.rust_usage_facts_warm());
        rust.warm_rust_usage_facts();
        assert!(rust.rust_usage_facts_ready());
        assert!(rust.rust_usage_facts_warm());

        let java: Arc<dyn Project> = Arc::new(TestProject::new(root, Language::Java));
        let java = WorkspaceAnalyzer::build(java, AnalyzerConfig::default());
        assert!(java.rust_usage_facts_ready());
        assert!(java.rust_usage_facts_warm());
    }
