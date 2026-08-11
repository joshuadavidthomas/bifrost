//! Consolidated `suite_bench_policy` test harness.
//!
//! Each module below was previously its own `tests/*.rs` integration binary.
//! They were merged so the suite links the library once instead of once per
//! file; module scoping keeps every test path and helper name isolated.
//! Run a single former file with:
//!     cargo test --test suite_bench_policy -- <module>::

#[path = "../common/mod.rs"]
mod common;

mod benchmark_compare;
mod benchmark_manifest;
mod benchmark_repo_cache;
mod benchmark_workflow_policy;
mod bifrost_policy_cli;
mod builtin_policy_pack;
mod cvss_classification;
mod measure_semantic_summary_taint_lifecycle;
mod no_stringly_name_parsing;
mod policy_assertion_conformance;
mod policy_assertion_evaluation;
mod policy_assertion_per_file_completion;
mod policy_baseline_evaluation;
mod policy_docs;
mod policy_edge_parity_assertions;
mod policy_identity_assertions;
mod policy_identity_conformance;
mod policy_loading;
mod policy_loading_workspace;
mod policy_loop_invariant_sort;
mod policy_match_evaluation;
mod policy_materialization_assertions;
mod policy_overload_selection_trio;
mod policy_relational_assertions;
mod policy_rendering;
mod policy_resolution_assertions;
mod policy_resolution_conformance;
mod policy_sarif_rendering;
mod policy_scope_evaluation;
mod policy_source;
mod policy_suppression_evaluation;
mod policy_suppression_loading;
mod sanitizer_summary_pack;
// Stage 4 of the procedure-summary foundry (#1871, #1923): the shipped k3
// sanitizer packs. The converter that produces them is generation-time tooling
// behind `release-tooling`, so this acceptance runs with that feature enabled.
#[cfg(feature = "release-tooling")]
mod sanitizer_pack_shipping;
// The golden-core JDK flow-through summary pack for OWASP Benchmark blocker 4
// (#1935). Its converter is generation-time tooling behind `release-tooling`,
// so this acceptance runs with that feature enabled.
#[cfg(feature = "release-tooling")]
mod golden_summary_pack_shipping;
mod issue_1953_ruby_call_binding;
mod scan_usages_same_owner_policy;
// Milestone 3 of the procedure-summary foundry (#1871). The fixture engine and
// its runner are generation-time tooling behind `release-tooling`, so this
// acceptance runs with that feature enabled and is absent from a featureless
// build, exactly like the rest of the foundry.
#[cfg(feature = "release-tooling")]
mod summary_foundry_fixtures;
// Milestone 4.5 of the procedure-summary foundry (#1871): the demand sweep. Its
// extraction proof and its env-gated corpus driver are generation-time tooling
// behind `release-tooling`, like the rest of the foundry.
#[cfg(feature = "release-tooling")]
mod summary_foundry_demand;
mod taint_policy_adapter;
#[cfg(unix)]
mod temp_storage_scripts;
