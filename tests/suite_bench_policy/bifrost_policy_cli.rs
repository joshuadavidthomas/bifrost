use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

use serde_json::Value;

use crate::common::{BuiltInlineTestProject, InlineTestProject};

const APP: &str = include_str!("../fixtures/policy-cli/project/src/app.py");
const RESOURCE_SOURCE: &str = include_str!("../fixtures/policy-cli/project/src/resource.ts");
const CROSS_LANGUAGE_TYPESCRIPT_RESOURCE_SOURCE: &str = r#"interface Resource {}

function openResource(): Resource {
  return {};
}

function closeResource(resource: Resource): void {}

export function leaksResource(): void {
  const resource = openResource();
}
"#;
const JAVA_RESOURCE_SOURCE: &str = r#"class Resource {}

class ResourceLifecycle {
  static Resource openResource() {
    return null;
  }

  static void closeResource(Resource resource) {}

  static void leaksResource() {
    Resource resource = ResourceLifecycle.openResource();
  }
}
"#;
const DYNAMIC: &str = include_str!("../fixtures/policy-cli/project/policies/dynamic-eval.rqlp");
const INFERRED: &str =
    include_str!("../fixtures/policy-cli/project/policies/inferred-dynamic-eval.rqlp");
const NO_EXEC: &str = include_str!("../fixtures/policy-cli/project/policies/no-exec.rqlp");
const RESOURCE: &str =
    include_str!("../fixtures/policy-cli/project/policies/resource-lifecycle.rqlp");
const UNRATED: &str = include_str!("../fixtures/policy-cli/project/policies/unrated-eval.rqlp");
const NOTE: &str = include_str!("../fixtures/policy-cli/project/policies/note-eval.rqlp");
const HTTP_ENDPOINT: &str =
    include_str!("../fixtures/policy-cli/project/policies/endpoints/http-request-parameter.rqlp");
const ACQUIRE_ENDPOINT: &str =
    include_str!("../fixtures/policy-cli/project/policies/endpoints/resource-acquire.rqlp");
const INFERRED_ACQUIRE_ENDPOINT: &str =
    include_str!("../fixtures/policy-cli/overrides/resource-acquire-inferred.rqlp");
const CLOSE_ENDPOINT: &str =
    include_str!("../fixtures/policy-cli/project/policies/endpoints/resource-close.rqlp");
const CROSS_LANGUAGE_ACQUIRE_ENDPOINT: &str = r#"(endpoint
  :schema-version 1
  :id "bifrost.resources.acquire"
  :name "Resource acquisition"
  :display-name "acquired resource"
  :role source
  :categories [resource.lifecycle resource.acquire]
  :selector
    (rql
      :schema-version 1
      (union
        (language typescript (call :callee (name "openResource")))
        (language java (call :callee (name "openResource")))))
  :binding return-value
  :supersedes [])"#;
const CROSS_LANGUAGE_CLOSE_ENDPOINT: &str = r#"(endpoint
  :schema-version 1
  :id "bifrost.resources.close"
  :name "Resource close"
  :display-name "resource close"
  :role sink
  :categories [resource.lifecycle resource.close]
  :selector
    (rql
      :schema-version 1
      (union
        (language typescript (call :callee (name "closeResource")))
        (language java (call :callee (name "closeResource")))))
  :binding (argument :index 0)
  :supersedes [])"#;
const DOMINANT_CLOSE_ENDPOINT: &str = r#"(endpoint
  :schema-version 1
  :id "bifrost.resources.close-dominant"
  :name "Dominant resource close"
  :display-name "dominant resource close"
  :role sink
  :categories [resource.lifecycle resource.close]
  :selector
    (rql
      :schema-version 1
      (language typescript
        (call :callee (name "closeResource"))))
  :binding (argument :index 0)
  :supersedes [bifrost.resources.close])"#;
const DOMINANT_ACQUIRE_ENDPOINT: &str = r#"(endpoint
  :schema-version 1
  :id "bifrost.resources.acquire-dominant"
  :name "Dominant resource acquisition"
  :display-name "dominant acquired resource"
  :role source
  :categories [resource.lifecycle resource.acquire]
  :selector
    (rql
      :schema-version 1
      (language typescript
        (call :callee (name "openResource"))))
  :binding return-value
  :supersedes [bifrost.resources.acquire])"#;
const DOMINANCE_SOURCE: &str = r#"class Resource {}

function openResource(): Resource {
  return new Resource();
}

function closeResource(resource: Resource): void {}

export function closesResource(): void {
  const resource = openResource();
  closeResource(resource);
}
"#;
const EXIT_EVENT_POLICY: &str = r#"(policy
  :schema-version 1
  :id "bifrost.test.resource-exit-event"
  :name "Resource exit event"
  :message "Resource reached the analysis-root exit"
  :severity error
  :analysis
    (analysis
      :type typestate
      :mode may
      :subjects
        (subject-set
          :include-matches [
            (match-directory
              :path "policies/endpoints"
              :scope recursive
              :categories (all [resource.acquire]))]
          :entries [])
      :uncertainty
        (uncertainty
          :escape inconclusive)
      :automaton
        (automaton
          :states [open violated]
          :initial open
          :accepting-states [open]
          :error-states [violated]
          :events [
            (event
              :id finish
              :on (normal-procedure-exit :scope analysis-root)
              :supersedes [])]
          :transitions [
            (transition :from open :on finish :to violated)]
          :terminal-expectations [])))"#;

fn policy_project(extra: &[(&str, String)]) -> BuiltInlineTestProject {
    let mut project = InlineTestProject::new()
        .file("src/app.py", APP)
        .file("src/resource.ts", RESOURCE_SOURCE)
        .file("policies/dynamic-eval.rqlp", DYNAMIC)
        .file("policies/inferred-dynamic-eval.rqlp", INFERRED)
        .file("policies/no-exec.rqlp", NO_EXEC)
        .file("policies/resource-lifecycle.rqlp", RESOURCE)
        .file("policies/unrated-eval.rqlp", UNRATED)
        .file("policies/note-eval.rqlp", NOTE)
        .file(
            "policies/endpoints/http-request-parameter.rqlp",
            HTTP_ENDPOINT,
        )
        .file("policies/endpoints/resource-acquire.rqlp", ACQUIRE_ENDPOINT)
        .file("policies/endpoints/resource-close.rqlp", CLOSE_ENDPOINT);
    for (path, source) in extra {
        project = project.file(*path, source.clone());
    }
    project.build()
}

fn resource_policy_project(path: &str, source: &str) -> BuiltInlineTestProject {
    InlineTestProject::new()
        .file(path, source)
        .file("policies/resource-lifecycle.rqlp", RESOURCE)
        .file(
            "policies/endpoints/resource-acquire.rqlp",
            CROSS_LANGUAGE_ACQUIRE_ENDPOINT,
        )
        .file(
            "policies/endpoints/resource-close.rqlp",
            CROSS_LANGUAGE_CLOSE_ENDPOINT,
        )
        .build()
}

fn repeated_java_resource_policy_project(copy: String) -> BuiltInlineTestProject {
    InlineTestProject::new()
        .file("src/ResourceLifecycle.java", JAVA_RESOURCE_SOURCE)
        .file("policies/resource-lifecycle.rqlp", RESOURCE)
        .file("policies/resource-lifecycle-copy.rqlp", copy)
        .file(
            "policies/endpoints/resource-acquire.rqlp",
            CROSS_LANGUAGE_ACQUIRE_ENDPOINT,
        )
        .file(
            "policies/endpoints/resource-close.rqlp",
            CROSS_LANGUAGE_CLOSE_ENDPOINT,
        )
        .build()
}

fn bifrost(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bifrost"));
    command
        .arg("--root")
        .arg(root)
        .env("BIFROST_PARALLELISM", "1");
    command
}

fn run(root: &Path, args: &[&str]) -> Output {
    bifrost(root)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run bifrost {args:?}: {error}"))
}

fn assert_status(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON stdout: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    })
}

fn assert_single_terminal_safe_line(output: &Output) {
    let stderr = String::from_utf8(output.stderr.clone()).expect("UTF-8 stderr");
    assert_eq!(stderr.matches('\n').count(), 1, "{stderr:?}");
    for character in ['\u{001B}', '\u{202E}', '\u{2066}'] {
        assert!(
            !stderr.contains(character),
            "raw {character:?} in {stderr:?}"
        );
    }
    for escaped in ["\\u{A}", "\\u{1B}", "\\u{202E}", "\\u{2066}"] {
        assert!(stderr.contains(escaped), "missing {escaped} in {stderr:?}");
    }
}

#[test]
fn built_in_policy_catalog_lists_without_constructing_a_workspace() {
    let output = Command::new(env!("CARGO_BIN_EXE_bifrost"))
        .arg("--list-policies")
        .output()
        .expect("run policy listing");
    assert_status(&output, 0);
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    let manifest = json_stdout(&output);
    assert_eq!(manifest["id"], "bifrost.code-smells");
    assert_eq!(manifest["policies"].as_array().map(Vec::len), Some(13));
    assert_eq!(
        manifest["policies"][0]["id"],
        "bifrost.correctness.dynamic-evaluation"
    );
}

#[test]
fn built_in_and_workspace_policies_run_in_one_batch() {
    let project = policy_project(&[]);
    let output = run(
        project.root(),
        &[
            "--policy-id",
            "bifrost.correctness.dynamic-evaluation",
            "--policy-file",
            "policies/no-exec.rqlp",
            "--evaluation-date",
            "2026-07-28",
            "--fail-on",
            "never",
            "--format",
            "json",
        ],
    );
    assert_status(&output, 0);
    let report = json_stdout(&output);
    let policy_ids = report["runs"]
        .as_array()
        .expect("policy runs")
        .iter()
        .map(|run| run["policy_id"].as_str().expect("policy id"))
        .collect::<Vec<_>>();
    assert_eq!(
        policy_ids,
        vec![
            "bifrost.correctness.dynamic-evaluation",
            "bifrost.security.no-exec"
        ]
    );
}

#[test]
fn built_in_pack_and_category_selectors_run_valid_batches() {
    let project = policy_project(&[]);
    let category = run(
        project.root(),
        &[
            "--policy-category",
            "correctness",
            "--evaluation-date",
            "2026-07-28",
            "--fail-on",
            "never",
            "--format",
            "json",
        ],
    );
    assert_status(&category, 0);
    let category_report = json_stdout(&category);
    let category_ids = category_report["runs"]
        .as_array()
        .expect("category runs")
        .iter()
        .map(|run| run["policy_id"].as_str().expect("category policy id"))
        .collect::<Vec<_>>();
    assert_eq!(
        category_ids,
        vec![
            "bifrost.correctness.dynamic-evaluation",
            "bifrost.correctness.rayon-in-blocking-lazy-init",
            "bifrost.correctness.unsafe-deserialization"
        ]
    );

    let pack = run(
        project.root(),
        &[
            "--policy-pack",
            "bifrost.code-smells",
            "--evaluation-date",
            "2026-07-28",
            "--fail-on",
            "never",
            "--format",
            "json",
        ],
    );
    assert_status(&pack, 0);
    let pack_report = json_stdout(&pack);
    let pack_ids = pack_report["runs"]
        .as_array()
        .expect("pack runs")
        .iter()
        .map(|run| run["policy_id"].as_str().expect("pack policy id"))
        .collect::<Vec<_>>();
    assert_eq!(
        pack_ids,
        vec![
            "bifrost.correctness.dynamic-evaluation",
            "bifrost.correctness.rayon-in-blocking-lazy-init",
            "bifrost.correctness.unsafe-deserialization",
            "bifrost.performance.database-call-in-loop",
            "bifrost.performance.expensive-operation-in-nested-loop",
            "bifrost.performance.file-read-in-loop",
            "bifrost.performance.network-call-in-loop",
            "bifrost.performance.parsing-in-loop",
            "bifrost.performance.regex-compile-in-loop",
            "bifrost.performance.serialization-in-loop",
            "bifrost.performance.sleep-in-loop",
            "bifrost.performance.sort-in-loop",
            "bifrost.performance.subprocess-in-loop",
        ]
    );
}

#[test]
fn unknown_built_in_selector_is_a_policy_invocation_error() {
    let project = policy_project(&[]);
    let output = run(
        project.root(),
        &["--policy-pack", "bifrost.missing", "--format", "json"],
    );
    assert_status(&output, 2);
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("unknown built-in policy pack `bifrost.missing`")
    );
}

#[test]
fn thresholds_cover_clean_rated_and_unrated_findings() {
    let project = policy_project(&[]);
    let cases: &[(&[&str], i32)] = &[
        (&["--policy-file", "policies/no-exec.rqlp"], 0),
        (
            &[
                "--policy-file",
                "policies/dynamic-eval.rqlp",
                "--fail-on",
                "never",
            ],
            0,
        ),
        (
            &[
                "--policy-file",
                "policies/dynamic-eval.rqlp",
                "--fail-on",
                "warning",
            ],
            1,
        ),
        (
            &[
                "--policy-file",
                "policies/dynamic-eval.rqlp",
                "--fail-on",
                "error",
            ],
            0,
        ),
        (
            &[
                "--policy-file",
                "policies/note-eval.rqlp",
                "--fail-on",
                "note",
            ],
            1,
        ),
        (
            &[
                "--policy-file",
                "policies/note-eval.rqlp",
                "--fail-on",
                "warning",
            ],
            0,
        ),
        (
            &[
                "--policy-file",
                "policies/unrated-eval.rqlp",
                "--fail-on",
                "finding",
            ],
            1,
        ),
        (
            &[
                "--policy-file",
                "policies/unrated-eval.rqlp",
                "--fail-on",
                "note",
            ],
            0,
        ),
    ];
    for (args, expected) in cases {
        let output = run(project.root(), args);
        assert_status(&output, *expected);
    }

    let default = run(
        project.root(),
        &["--policy-file", "policies/dynamic-eval.rqlp"],
    );
    assert_status(&default, 1);
    let stdout = String::from_utf8(default.stdout).expect("UTF-8 human report");
    assert!(!stdout.contains('\u{001B}'));
    assert!(stdout.contains("[warning]  src/app.py:2:12\n"), "{stdout}");
    assert!(
        stdout.contains("    Dynamic evaluation is forbidden\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("  evidence:"), "{stdout}");
    assert!(
        stdout.contains("summary: 1 active finding; 0 suppressed findings; 1 complete policy run")
    );

    let verbose = run(
        project.root(),
        &["--policy-file", "policies/dynamic-eval.rqlp", "--verbose"],
    );
    assert_status(&verbose, 1);
    let verbose = String::from_utf8(verbose.stdout).expect("UTF-8 verbose human report");
    assert!(verbose.contains("src/app.py:2:12: [warning] bifrost.security.dynamic-eval"));
    assert!(verbose.contains("  evidence: structural_match call\n"));

    let colored = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--color",
            "always",
        ],
    );
    assert_status(&colored, 1);
    assert!(
        colored
            .stdout
            .windows(5)
            .any(|window| window == b"\x1b[33m")
    );

    let no_color = bifrost(project.root())
        .args([
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--color",
            "auto",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run policy with NO_COLOR");
    assert_status(&no_color, 1);
    assert!(!no_color.stdout.contains(&0x1b));

    let never = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--color",
            "never",
        ],
    );
    assert_status(&never, 1);
    assert!(!never.stdout.contains(&0x1b));

    let verbose_colored = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--verbose",
            "--color",
            "always",
        ],
    );
    assert_status(&verbose_colored, 1);
    assert!(
        verbose_colored
            .stdout
            .windows(5)
            .any(|window| window == b"\x1b[33m")
    );
}

#[test]
fn policy_suppressions_are_deterministic_auditable_and_threshold_aware_across_formats() {
    let project = policy_project(&[]);
    let baseline = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-07-27",
            "--format",
            "json",
            "--fail-on",
            "warning",
        ],
    );
    assert_status(&baseline, 1);
    let baseline = json_stdout(&baseline);
    let policy_id = baseline["rules"][0]["policy_id"].as_str().unwrap();
    let policy_hash = baseline["rules"][0]["policy_hash"].as_str().unwrap();
    let finding_id = baseline["runs"][0]["findings"][0]["id"].as_str().unwrap();
    let suppression = serde_json::json!({
        "schema_version": 1,
        "suppressions": [{
            "policy_id": policy_id,
            "finding_id": finding_id,
            "identity_stability": "strong",
            "status": "accepted",
            "reason": "Reviewed compatibility boundary",
            "policy_hash_at_acceptance": policy_hash,
            "accepted_by": "security-review",
            "accepted_at": "2026-07-01",
            "expires_at": "2026-07-27"
        }]
    });
    let default_path = project.root().join(".bifrost/suppressions.json");
    fs::create_dir_all(default_path.parent().unwrap()).unwrap();
    fs::write(
        &default_path,
        serde_json::to_vec_pretty(&suppression).unwrap(),
    )
    .unwrap();
    let custom_path = project.root().join("reviews/accepted.json");
    fs::create_dir_all(custom_path.parent().unwrap()).unwrap();
    fs::write(
        &custom_path,
        serde_json::to_vec_pretty(&suppression).unwrap(),
    )
    .unwrap();

    let concise = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-07-27",
            "--fail-on",
            "warning",
        ],
    );
    assert_status(&concise, 0);
    let concise = String::from_utf8(concise.stdout).unwrap();
    assert!(!concise.contains("Dynamic evaluation is forbidden"));
    assert_eq!(
        concise,
        "summary: 0 active findings; 1 suppressed finding; 1 complete policy run; clean\n"
    );

    let json_args = [
        "--policy-file",
        "policies/dynamic-eval.rqlp",
        "--suppressions-file",
        "reviews/accepted.json",
        "--evaluation-date",
        "2026-07-27",
        "--format",
        "json",
        "--fail-on",
        "warning",
    ];
    let first = run(project.root(), &json_args);
    let second = run(project.root(), &json_args);
    assert_status(&first, 0);
    assert_status(&second, 0);
    assert_eq!(first.stdout, second.stdout);
    let json = json_stdout(&first);
    assert_eq!(json["schema_version"], 3);
    assert_eq!(json["evaluation"]["evaluation_date"], "2026-07-27");
    assert_eq!(
        json["evaluation"]["suppression_path"],
        "reviews/accepted.json"
    );
    assert_eq!(json["runs"][0]["findings"][0]["id"], finding_id);
    assert_eq!(
        json["runs"][0]["findings"][0]["suppression"]["status"],
        "accepted"
    );
    assert_eq!(json["suppressions"][0]["applied"], true);

    let verbose = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-07-27",
            "--verbose",
            "--fail-on",
            "warning",
        ],
    );
    assert_status(&verbose, 0);
    let verbose = String::from_utf8(verbose.stdout).unwrap();
    assert!(verbose.contains("Dynamic evaluation is forbidden"));
    assert!(verbose.contains("  suppression: accepted (policy hash matching)\n"));
    assert!(verbose.contains("  accepted: 2026-07-01 by security-review\n"));

    let sarif = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-07-27",
            "--format",
            "sarif",
            "--fail-on",
            "warning",
        ],
    );
    assert_status(&sarif, 0);
    let sarif = json_stdout(&sarif);
    assert_eq!(
        sarif["runs"][0]["results"][0]["suppressions"][0]["kind"],
        "external"
    );
    assert_eq!(
        sarif["runs"][0]["results"][0]["partialFingerprints"]["bifrostFinding/v1"],
        finding_id
    );

    let expired = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-07-28",
            "--fail-on",
            "warning",
        ],
    );
    assert_status(&expired, 1);
    let expired = String::from_utf8(expired.stdout).unwrap();
    assert!(expired.contains("Dynamic evaluation is forbidden"));
    assert!(expired.contains("1 expired suppression review"));

    fs::write(project.root().join("reviews/invalid.json"), b"not JSON\n").unwrap();
    let invalid = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--suppressions-file",
            "reviews/invalid.json",
            "--evaluation-date",
            "2026-07-27",
            "--format",
            "json",
            "--fail-on",
            "warning",
        ],
    );
    assert_status(&invalid, 2);
    let invalid = json_stdout(&invalid);
    assert_eq!(invalid["runs"][0]["findings"].as_array().unwrap().len(), 1);
    assert_eq!(invalid["diagnostics"][0]["code"], "suppression-load-failed");
}

#[test]
fn proven_subset_callers_are_visible_and_reliable_without_claiming_exhaustiveness() {
    let source = r#"class Smells {
    void terminate() { System.exit(1); }
    void directTerminate() { this.terminate(); }
    void secondOrderTerminate() { this.directTerminate(); }
}"#;
    let policy = |id: &str, completeness: &str| {
        format!(
            r#"(policy
  :id "{id}"
  :name "Proven callers"
  :message "Calls System.exit"
  :severity warning
  :analysis (analysis :type match :selector
    (rql (language java
      (callers :depth 2 :proof proven {completeness}
        (enclosing-decl
          (call :receiver (name "System") :callee (name "exit"))))))))"#
        )
    };
    let project = InlineTestProject::new()
        .file("src/Smells.java", source)
        .file(
            "policies/exhaustive.rqlp",
            policy("test.exhaustive-callers", ""),
        )
        .file(
            "policies/proven-subset.rqlp",
            policy("test.proven-subset-callers", ":completeness proven-subset"),
        )
        .build();

    let exhaustive = run(
        project.root(),
        &[
            "--policy-file",
            "policies/exhaustive.rqlp",
            "--format",
            "json",
        ],
    );
    assert_status(&exhaustive, 2);
    let exhaustive = json_stdout(&exhaustive);
    assert_eq!(exhaustive["runs"][0]["completion"]["type"], "inconclusive");

    let subset = run(
        project.root(),
        &[
            "--policy-file",
            "policies/proven-subset.rqlp",
            "--format",
            "json",
        ],
    );
    assert_status(&subset, 1);
    let subset = json_stdout(&subset);
    assert_eq!(subset["runs"][0]["completion"]["type"], "proven_subset");
    assert_eq!(subset["runs"][0]["findings"].as_array().unwrap().len(), 2);
    assert!(
        subset["runs"][0]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["impact"] == "declared_non_exhaustive")
    );

    let human = run(
        project.root(),
        &["--policy-file", "policies/proven-subset.rqlp"],
    );
    assert_status(&human, 1);
    let human = String::from_utf8(human.stdout).expect("UTF-8 human report");
    assert!(
        human.contains("proven subset (not exhaustive; call_relation_candidates_omitted)"),
        "{human}"
    );
    assert!(human.contains("non-exhaustive"), "{human}");

    let sarif = run(
        project.root(),
        &[
            "--policy-file",
            "policies/proven-subset.rqlp",
            "--format",
            "sarif",
        ],
    );
    assert_status(&sarif, 1);
    let sarif = json_stdout(&sarif);
    assert_eq!(
        sarif["runs"][0]["invocations"][0]["executionSuccessful"],
        true
    );
    assert!(
        sarif["runs"][0]["invocations"][0]["toolExecutionNotifications"]
            .as_array()
            .unwrap()
            .iter()
            .any(|notification| notification["descriptor"]["id"] == "BIFROST_POLICY_PROVEN_SUBSET"),
        "{sarif:#}"
    );

    let below_threshold = run(
        project.root(),
        &[
            "--policy-file",
            "policies/proven-subset.rqlp",
            "--fail-on",
            "error",
        ],
    );
    assert_status(&below_threshold, 2);
}

#[test]
fn strict_versions_endpoint_roots_and_typestate_execution_have_typed_statuses() {
    let project = policy_project(&[]);

    let inferred = run(
        project.root(),
        &[
            "--policy-file",
            "policies/inferred-dynamic-eval.rqlp",
            "--format",
            "json",
            "--require-explicit-schema-versions",
        ],
    );
    assert_status(&inferred, 2);
    let report = json_stdout(&inferred);
    assert!(report["rules"].as_array().unwrap().is_empty());
    assert!(report["runs"].as_array().unwrap().is_empty());
    let codes = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"explicit-policy-schema-version-required"));
    assert!(codes.contains(&"explicit-rql-schema-version-required"));

    let accepted_inference = run(
        project.root(),
        &["--policy-file", "policies/inferred-dynamic-eval.rqlp"],
    );
    assert_status(&accepted_inference, 1);
    assert!(
        String::from_utf8_lossy(&accepted_inference.stdout)
            .contains(
                "policy bifrost.security.inferred-dynamic-eval inferred policy schema 1 and RQL schema 1"
            )
    );

    let endpoint = run(
        project.root(),
        &[
            "--policy-file",
            "policies/endpoints/http-request-parameter.rqlp",
        ],
    );
    assert_status(&endpoint, 2);
    assert!(String::from_utf8_lossy(&endpoint.stdout).contains("not-executable-endpoint"));

    let resource = run(
        project.root(),
        &[
            "--policy-file",
            "policies/resource-lifecycle.rqlp",
            "--format",
            "json",
        ],
    );
    assert_status(&resource, 1);
    let report = json_stdout(&resource);
    assert_eq!(report["rules"].as_array().unwrap().len(), 1);
    assert_eq!(report["runs"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["rules"][0]["endpoint_dependencies"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(report["runs"][0]["completion"]["type"], "complete");
    assert_eq!(report["runs"][0]["findings"].as_array().unwrap().len(), 1);
    for finding in report["runs"][0]["findings"].as_array().unwrap() {
        assert_eq!(finding["analysis_type"], "typestate");
        assert_eq!(finding["identity_stability"], "strong");
        assert!(!finding["related"].as_array().unwrap().is_empty());
        assert!(!finding["witnesses"].as_array().unwrap().is_empty());
    }

    let dependency_project = policy_project(&[(
        "policies/endpoints/resource-acquire.rqlp",
        INFERRED_ACQUIRE_ENDPOINT.to_string(),
    )]);
    let strict_dependency = run(
        dependency_project.root(),
        &[
            "--policy-file",
            "policies/resource-lifecycle.rqlp",
            "--format",
            "json",
            "--require-explicit-schema-versions",
        ],
    );
    assert_status(&strict_dependency, 2);
    let report = json_stdout(&strict_dependency);
    assert!(report["rules"].as_array().unwrap().is_empty());
    assert!(report["runs"].as_array().unwrap().is_empty());
    let dependency_diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(dependency_diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "explicit-policy-schema-version-required"
            && diagnostic["source"] == "policies/endpoints/resource-acquire.rqlp"
    }));
    assert!(dependency_diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "explicit-rql-schema-version-required"
            && diagnostic["source"] == "policies/endpoints/resource-acquire.rqlp"
    }));
}

#[test]
fn json_sarif_stdout_and_atomic_file_output_are_deterministic() {
    let project = policy_project(&[]);
    let json_args = [
        "--policy-file",
        "policies/dynamic-eval.rqlp",
        "--format",
        "json",
        "--fail-on",
        "never",
    ];
    let first = run(project.root(), &json_args);
    let second = run(project.root(), &json_args);
    assert_status(&first, 0);
    assert_status(&second, 0);
    assert_eq!(first.stdout, second.stdout);
    let report = json_stdout(&first);
    assert_eq!(report["rules"].as_array().unwrap().len(), 1);
    assert_eq!(report["runs"][0]["findings"].as_array().unwrap().len(), 1);
    assert_eq!(report["runs"][0]["completion"]["type"], "complete");

    let sarif = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--format",
            "sarif",
            "--fail-on",
            "never",
        ],
    );
    assert_status(&sarif, 0);
    let sarif = json_stdout(&sarif);
    assert_eq!(sarif["version"], "2.1.0");
    assert_eq!(sarif["runs"][0]["columnKind"], "unicodeCodePoints");
    assert_eq!(
        sarif["runs"][0]["invocations"][0]["executionSuccessful"],
        true
    );
    assert_eq!(
        sarif["runs"][0]["results"][0]["ruleId"],
        "bifrost.security.dynamic-eval"
    );

    let destination = tempfile::tempdir().expect("output directory");
    let output_path = destination.path().join("report.json");
    let written = bifrost(project.root())
        .args(json_args)
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("write policy JSON");
    assert_status(&written, 0);
    assert!(written.stdout.is_empty());
    assert_eq!(fs::read(&output_path).unwrap(), first.stdout);
}

#[test]
fn policy_id_order_is_stable_and_duplicate_roots_are_all_excluded() {
    let duplicate = DYNAMIC.replace(
        ":name \"No dynamic evaluation\"",
        ":name \"Duplicate dynamic evaluation\"",
    );
    let project = policy_project(&[("policies/duplicate.rqlp", duplicate)]);

    let first = run(
        project.root(),
        &[
            "--policy-file",
            "policies/note-eval.rqlp",
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--format",
            "json",
            "--fail-on",
            "never",
        ],
    );
    let reversed = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--policy-file",
            "policies/note-eval.rqlp",
            "--format",
            "json",
            "--fail-on",
            "never",
        ],
    );
    assert_status(&first, 0);
    assert_status(&reversed, 0);
    assert_eq!(first.stdout, reversed.stdout);

    let duplicates = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--policy-file",
            "policies/duplicate.rqlp",
            "--format",
            "json",
        ],
    );
    let duplicates_reversed = run(
        project.root(),
        &[
            "--policy-file",
            "policies/duplicate.rqlp",
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--format",
            "json",
        ],
    );
    assert_status(&duplicates, 2);
    assert_status(&duplicates_reversed, 2);
    assert_eq!(duplicates.stdout, duplicates_reversed.stdout);
    let report = json_stdout(&duplicates);
    assert!(report["rules"].as_array().unwrap().is_empty());
    assert!(report["runs"].as_array().unwrap().is_empty());
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    let mut sources = Vec::new();
    for diagnostic in diagnostics {
        assert_eq!(diagnostic["code"], "duplicate-policy-id");
        assert_eq!(
            diagnostic["message"],
            "policy ID `bifrost.security.dynamic-eval` has 2 requested definitions across 2 source identities; every definition was excluded"
        );
        sources.push(diagnostic["source"].as_str().unwrap());
        assert_eq!(diagnostic["related"].as_array().unwrap().len(), 1);
    }
    sources.sort_unstable();
    assert_eq!(
        sources,
        ["policies/duplicate.rqlp", "policies/dynamic-eval.rqlp"]
    );
}

#[test]
fn mixed_invalid_and_typestate_batches_retain_valid_findings_with_typed_statuses() {
    let project = policy_project(&[(
        "policies/invalid.rqlp",
        "(policy :id \"broken\"".to_string(),
    )]);
    let destination = tempfile::tempdir().expect("output directory");
    let output_path = destination.path().join("mixed.json");
    let mixed = bifrost(project.root())
        .args([
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--policy-file",
            "policies/invalid.rqlp",
            "--format",
            "json",
            "--output",
        ])
        .arg(&output_path)
        .output()
        .expect("run mixed valid/invalid policy batch");
    assert_status(&mixed, 2);
    assert!(mixed.stdout.is_empty());
    let report: Value = serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(report["rules"].as_array().unwrap().len(), 1);
    assert_eq!(report["runs"][0]["findings"].as_array().unwrap().len(), 1);
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);

    let typestate = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--policy-file",
            "policies/resource-lifecycle.rqlp",
            "--format",
            "sarif",
        ],
    );
    assert_status(&typestate, 1);
    let sarif = json_stdout(&typestate);
    assert_eq!(sarif["runs"][0]["results"].as_array().unwrap().len(), 2);
    assert_eq!(
        sarif["runs"][0]["invocations"][0]["executionSuccessful"],
        true
    );
    assert!(
        !sarif["runs"][0]["invocations"][0]["toolExecutionNotifications"]
            .as_array()
            .unwrap()
            .iter()
            .any(|notification| {
                notification["descriptor"]["id"] == "BIFROST_POLICY_INCONCLUSIVE"
            })
    );
}

#[test]
fn typestate_finding_identity_locations_and_witnesses_match_human_and_sarif() {
    let project = policy_project(&[]);
    let arguments = [
        "--policy-file",
        "policies/resource-lifecycle.rqlp",
        "--fail-on",
        "never",
    ];
    let json = run(
        project.root(),
        &[&arguments[..], &["--format", "json"]].concat(),
    );
    assert_status(&json, 0);
    let json = json_stdout(&json);
    let findings = json["runs"][0]["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    let finding_ids = findings
        .iter()
        .map(|finding| finding["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    for finding in findings {
        assert!(!finding["related"].as_array().unwrap().is_empty());
        assert!(!finding["witnesses"].as_array().unwrap().is_empty());
        assert_eq!(finding["witnesses_truncated"], false);
    }

    let human = run(project.root(), &[&arguments[..], &["--verbose"]].concat());
    assert_status(&human, 0);
    let human = String::from_utf8(human.stdout).expect("UTF-8 typestate report");
    for finding_id in &finding_ids {
        assert!(
            human.contains(finding_id),
            "missing {finding_id} in:\n{human}"
        );
    }
    assert!(human.contains("  related source: "), "{human}");
    assert!(human.contains("  witness "), "{human}");
    let finding = &findings[0];
    assert!(
        human.contains(finding["policy_id"].as_str().unwrap()),
        "{human}"
    );
    assert!(
        human.contains(&format!(
            "analysis: typestate ({}, {})",
            finding["certainty"]["type"].as_str().unwrap(),
            finding["completeness"]["type"].as_str().unwrap(),
        )),
        "{human}"
    );
    for witness in finding["witnesses"].as_array().unwrap() {
        assert!(human.contains(witness["id"].as_str().unwrap()), "{human}");
        for step in witness["steps"].as_array().unwrap() {
            assert!(human.contains(step["label"].as_str().unwrap()), "{human}");
        }
    }

    let sarif = run(
        project.root(),
        &[&arguments[..], &["--format", "sarif"]].concat(),
    );
    assert_status(&sarif, 0);
    let sarif = json_stdout(&sarif);
    let results = sarif["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let sarif_ids = results
        .iter()
        .map(|result| result["properties"]["bifrost.findingId"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(sarif_ids, finding_ids);
    for (finding, result) in findings.iter().zip(results) {
        assert_eq!(result["ruleId"], finding["policy_id"]);
        assert_eq!(
            result["properties"]["bifrost.certainty"],
            finding["certainty"]
        );
        assert_eq!(
            result["properties"]["bifrost.findingCompleteness"],
            finding["completeness"]
        );
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            finding["primary"]["path"]
        );
        assert_eq!(
            result["locations"][0]["physicalLocation"]["region"]["startLine"],
            finding["primary"]["region"]["start_line"]
        );
        assert!(!result["relatedLocations"].as_array().unwrap().is_empty());
        assert!(!result["codeFlows"].as_array().unwrap().is_empty());
        let related = finding["related"].as_array().unwrap();
        let sarif_related = result["relatedLocations"].as_array().unwrap();
        assert_eq!(sarif_related.len(), related.len());
        for (expected, actual) in related.iter().zip(sarif_related) {
            assert_eq!(
                actual["physicalLocation"]["artifactLocation"]["uri"],
                expected["location"]["path"]
            );
            assert_eq!(
                actual["properties"]["bifrost.relationship"],
                expected["relationship"]
            );
        }
        let witnesses = finding["witnesses"].as_array().unwrap();
        let code_flows = result["codeFlows"].as_array().unwrap();
        assert_eq!(code_flows.len(), witnesses.len());
        for (expected, actual) in witnesses.iter().zip(code_flows) {
            assert_eq!(actual["properties"]["bifrost.witnessId"], expected["id"]);
            assert_eq!(
                actual["properties"]["bifrost.truncated"],
                expected["truncated"]
            );
            let actual_steps = actual["threadFlows"][0]["locations"].as_array().unwrap();
            let expected_steps = expected["steps"].as_array().unwrap();
            assert_eq!(actual_steps.len(), expected_steps.len());
            for (expected_step, actual_step) in expected_steps.iter().zip(actual_steps) {
                assert_eq!(
                    actual_step["location"]["message"]["text"],
                    expected_step["label"]
                );
            }
        }
    }
}

#[test]
fn java_and_typescript_resource_rqlp_retain_identity_through_all_renderers() {
    // Both typestate runs complete: Java since #1952, TypeScript once
    // receiverless local calls bind completely (#1951).
    for (language, expected_status, project) in [
        (
            "typescript",
            0,
            resource_policy_project("src/resource.ts", CROSS_LANGUAGE_TYPESCRIPT_RESOURCE_SOURCE),
        ),
        (
            "java",
            0,
            resource_policy_project("src/ResourceLifecycle.java", JAVA_RESOURCE_SOURCE),
        ),
    ] {
        let arguments = [
            "--policy-file",
            "policies/resource-lifecycle.rqlp",
            "--fail-on",
            "never",
        ];
        let json = run(
            project.root(),
            &[&arguments[..], &["--format", "json"]].concat(),
        );
        assert_status(&json, expected_status);
        let json = json_stdout(&json);
        let findings = json["runs"][0]["findings"]
            .as_array()
            .expect("findings array");
        assert_eq!(findings.len(), 1, "{language} report: {json:#}");
        let finding = &findings[0];
        let finding_id = finding["id"].as_str().expect("finding id");
        let policy_id = finding["policy_id"].as_str().expect("policy id");
        assert_eq!(policy_id, "bifrost.test.resource-lifecycle");
        assert!(!finding["witnesses"].as_array().unwrap().is_empty());

        let human = run(project.root(), &[&arguments[..], &["--verbose"]].concat());
        assert_status(&human, expected_status);
        let human = String::from_utf8(human.stdout).expect("UTF-8 typestate report");
        assert!(human.contains(finding_id), "{human}");
        assert!(human.contains(policy_id), "{human}");

        let sarif = run(
            project.root(),
            &[&arguments[..], &["--format", "sarif"]].concat(),
        );
        assert_status(&sarif, expected_status);
        let sarif = json_stdout(&sarif);
        let result = &sarif["runs"][0]["results"][0];
        assert_eq!(result["ruleId"], policy_id);
        assert_eq!(result["properties"]["bifrost.findingId"], finding_id);
        assert_eq!(
            result["properties"]["bifrost.certainty"],
            finding["certainty"]
        );
        assert_eq!(
            result["properties"]["bifrost.findingCompleteness"],
            finding["completeness"]
        );
        assert!(!result["codeFlows"].as_array().unwrap().is_empty());
    }
}

#[test]
fn repeated_typestate_policies_share_production_summaries_with_explicit_counters() {
    let copy = RESOURCE
        .replace(
            "bifrost.test.resource-lifecycle",
            "bifrost.test.resource-lifecycle-copy",
        )
        .replace("Resource lifecycle", "Resource lifecycle copy");
    let project = repeated_java_resource_policy_project(copy);
    let output = run(
        project.root(),
        &[
            "--policy-file",
            "policies/resource-lifecycle.rqlp",
            "--policy-file",
            "policies/resource-lifecycle-copy.rqlp",
            "--format",
            "json",
            "--fail-on",
            "never",
        ],
    );
    assert_status(&output, 0);
    let report = json_stdout(&output);
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    let metric = |run: &Value, name: &str| {
        run["work"]["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|metric| metric["name"] == name)
            .unwrap_or_else(|| panic!("missing {name} in {}", run["policy_id"]))["value"]
            .as_u64()
            .unwrap()
    };
    let first = runs
        .iter()
        .find(|run| run["policy_id"] == "bifrost.test.resource-lifecycle")
        .unwrap();
    let repeated = runs
        .iter()
        .find(|run| run["policy_id"] == "bifrost.test.resource-lifecycle-copy")
        .unwrap();

    assert!(metric(first, "typestate.summary_misses") > 0);
    assert_eq!(
        metric(first, "typestate.summary_recomputations"),
        metric(first, "typestate.summary_misses")
    );
    // Policy-batch exact results are presentation-neutral and replay their
    // original semantic, solver, and provider-execution charges. The second
    // policy therefore reuses the first result even though policy/finding IDs
    // remain policy-specific.
    assert!(metric(repeated, "typestate.summary_hits") > 0);
    assert_eq!(metric(repeated, "typestate.summary_misses"), 0);
    assert_eq!(metric(repeated, "typestate.summary_recomputations"), 0);
    let first_finding = &first["findings"][0];
    let repeated_finding = &repeated["findings"][0];
    for field in [
        "message",
        "severity",
        "certainty",
        "completeness",
        "primary",
        "related",
        "witnesses_truncated",
    ] {
        assert_eq!(first_finding[field], repeated_finding[field], "{field}");
    }
    let first_witnesses = first_finding["witnesses"].as_array().unwrap();
    let repeated_witnesses = repeated_finding["witnesses"].as_array().unwrap();
    assert_eq!(first_witnesses.len(), repeated_witnesses.len());
    for (first_witness, repeated_witness) in first_witnesses.iter().zip(repeated_witnesses) {
        assert_eq!(first_witness["steps"], repeated_witness["steps"]);
        assert_eq!(first_witness["truncated"], repeated_witness["truncated"]);
    }
    assert_eq!(first["diagnostics"], repeated["diagnostics"]);
    for run in runs {
        assert_eq!(metric(run, "typestate.summary_evictions"), 0);
        let _ = metric(run, "typestate.summary_rejections");
    }
}

#[test]
#[ignore = "emits timing evidence; run explicitly with --ignored --nocapture"]
fn production_summary_repeated_policy_measurement() {
    let copy = RESOURCE
        .replace(
            "bifrost.test.resource-lifecycle",
            "bifrost.test.resource-lifecycle-copy",
        )
        .replace("Resource lifecycle", "Resource lifecycle copy");
    let project = repeated_java_resource_policy_project(copy);
    let started = Instant::now();
    let output = run(
        project.root(),
        &[
            "--policy-file",
            "policies/resource-lifecycle.rqlp",
            "--policy-file",
            "policies/resource-lifecycle-copy.rqlp",
            "--format",
            "json",
            "--fail-on",
            "never",
        ],
    );
    let batch_micros = started.elapsed().as_micros();
    assert_status(&output, 2);
    let report = json_stdout(&output);
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    let metric = |run: &Value, name: &str| {
        run["work"]["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|metric| metric["name"] == name)
            .unwrap_or_else(|| panic!("missing {name} in {}", run["policy_id"]))["value"]
            .as_u64()
            .unwrap()
    };
    let measurements = runs
        .iter()
        .map(|run| {
            serde_json::json!({
                "policy_id": run["policy_id"],
                "hits": metric(run, "typestate.summary_hits"),
                "misses": metric(run, "typestate.summary_misses"),
                "rejections": metric(run, "typestate.summary_rejections"),
                "evictions": metric(run, "typestate.summary_evictions"),
                "recomputations": metric(run, "typestate.summary_recomputations"),
                "finding_ids": run["findings"].as_array().unwrap().iter()
                    .map(|finding| finding["id"].clone())
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    assert!(measurements[1]["hits"].as_u64().unwrap() > 0);
    assert_eq!(measurements[1]["recomputations"], 0);
    println!(
        "BIFROST_PRODUCTION_SUMMARY_POLICY_MEASUREMENT={}",
        serde_json::json!({
            "format": "bifrost_production_summary_policy/v1",
            "fixture": "inline:src/ResourceLifecycle.java",
            "language": "java",
            "repeated_policies": 2,
            "batch_micros": batch_micros,
            "runs": measurements,
        })
    );
}

#[test]
fn typestate_same_site_endpoint_precedence_retains_only_the_dominant_binding() {
    let project = policy_project(&[
        (
            "policies/endpoints/resource-acquire-dominant.rqlp",
            DOMINANT_ACQUIRE_ENDPOINT.to_owned(),
        ),
        (
            "policies/endpoints/resource-close-dominant.rqlp",
            DOMINANT_CLOSE_ENDPOINT.to_owned(),
        ),
        ("src/dominance.ts", DOMINANCE_SOURCE.to_owned()),
    ]);
    let output = run(
        project.root(),
        &[
            "--policy-file",
            "policies/resource-lifecycle.rqlp",
            "--format",
            "json",
            "--fail-on",
            "never",
        ],
    );
    assert_status(&output, 2);
    let report = json_stdout(&output);
    assert_eq!(
        report["rules"][0]["precedence_manifest"]["edges"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let metrics = report["runs"][0]["work"]["metrics"].as_array().unwrap();
    let subjects = metrics
        .iter()
        .find(|metric| metric["name"] == "typestate.subjects")
        .unwrap_or_else(|| panic!("missing typestate subject metric: {report:#}"));
    assert_eq!(subjects["value"], 2);
    let initial_seeds = metrics
        .iter()
        .find(|metric| metric["name"] == "typestate.initial_seeds")
        .expect("typestate initial-seed metric");
    assert_eq!(initial_seeds["value"], 2);
    let event_bindings = metrics
        .iter()
        .find(|metric| metric["name"] == "typestate.event_bindings")
        .expect("typestate event binding metric");
    assert_eq!(event_bindings["value"], 1);
}

#[test]
fn typestate_analysis_root_exit_event_executes_its_transition() {
    let project = policy_project(&[(
        "policies/resource-exit-event.rqlp",
        EXIT_EVENT_POLICY.to_owned(),
    )]);
    let output = run(
        project.root(),
        &[
            "--policy-file",
            "policies/resource-exit-event.rqlp",
            "--format",
            "json",
            "--fail-on",
            "never",
        ],
    );
    assert_status(&output, 0);
    let report = json_stdout(&output);
    assert_eq!(
        report["runs"][0]["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("semantic exit policy report: {report:#}"))
            .len(),
        1,
        "semantic exit policy report: {report:#}"
    );
    let event_bindings = report["runs"][0]["work"]["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|metric| metric["name"] == "typestate.event_bindings")
        .expect("semantic exit event binding metric");
    assert_eq!(event_bindings["value"], 1);
}

#[test]
fn typestate_projection_honors_authored_report_caps_before_authority_validation() {
    let capped = RESOURCE.replace(
        ":severity error",
        ":severity error\n  :report (report\n    :witness (witness :max-steps 1 :max-bytes 256)\n    :witnesses-per-finding 0\n    :origins-per-finding 0)",
    );
    let project = policy_project(&[("policies/resource-capped.rqlp", capped)]);
    let output = run(
        project.root(),
        &[
            "--policy-file",
            "policies/resource-capped.rqlp",
            "--format",
            "json",
            "--fail-on",
            "never",
        ],
    );
    assert_status(&output, 0);
    let report = json_stdout(&output);
    let finding = &report["runs"][0]["findings"][0];
    assert!(finding["witnesses"].as_array().unwrap().is_empty());
    assert_eq!(finding["witnesses_truncated"], true);
    assert!(finding["omitted_witnesses_lower_bound"].as_u64().unwrap() >= 1);
    assert!(finding["related"].as_array().unwrap().is_empty());
    assert_eq!(finding["related_truncated"], true);
    assert!(
        finding["omitted_related_locations_lower_bound"]
            .as_u64()
            .unwrap()
            >= 1
    );
}

#[test]
fn policy_mode_is_exclusive_and_output_failures_use_status_two_without_clobbering() {
    let project = policy_project(&[]);
    let missing = run(
        project.root(),
        &["--policy-file", "policies/missing.rqlp", "--format", "json"],
    );
    assert_status(&missing, 2);
    let missing_report = json_stdout(&missing);
    assert_eq!(
        missing_report["diagnostics"][0]["code"],
        "policy-load-failed"
    );

    let conflict = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--sources",
            "src",
        ],
    );
    assert_status(&conflict, 2);
    assert!(conflict.stdout.is_empty());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot be combined"));

    for arguments in [
        vec!["--format", "xml"],
        vec!["--policy-file"],
        vec!["--policy-file", "policies/dynamic-eval.rqlp", "--unknown"],
        vec!["--unknown", "--policy-file", "policies/dynamic-eval.rqlp"],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-02-30",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--suppressions-file",
            "/outside/reviews.json",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-07-27",
            "--evaluation-date",
            "2026-07-28",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--suppressions-file",
            "reviews/one.json",
            "--suppressions-file",
            "reviews/two.json",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--color",
            "sometimes",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--diff-base",
            "HEAD",
            "--diff-base",
            "HEAD~1",
        ],
        vec!["--policy-file", "policies/dynamic-eval.rqlp", "--diff-base"],
        vec!["--list-policies", "--diff-base", "HEAD"],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--accept-current",
            "--fail-on",
            "warning",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--accept-current",
            "--diff-base",
            "HEAD",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--accept-current",
            "--accept-current",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--baseline-file",
            "reviews/one.json",
            "--baseline-file",
            "reviews/two.json",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--baseline-file",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--baseline-file",
            "/outside/baseline.json",
        ],
        vec!["--list-policies", "--accept-current"],
        vec!["--list-policies", "--baseline-file", "reviews/one.json"],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--format",
            "json",
            "--verbose",
        ],
        vec![
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--format",
            "sarif",
            "--color",
            "never",
        ],
        vec![
            "--args",
            "not-json",
            "--policy-file",
            "policies/dynamic-eval.rqlp",
        ],
    ] {
        let invalid_invocation = run(project.root(), &arguments);
        assert_status(&invalid_invocation, 2);
        assert!(invalid_invocation.stdout.is_empty());
    }

    let legacy_value = run(
        project.root(),
        &["--tool", "--format", "--args", "not-json"],
    );
    assert_status(&legacy_value, 1);
    assert!(String::from_utf8_lossy(&legacy_value.stderr).contains("--args must be valid JSON"));

    let destination = tempfile::tempdir().expect("destination parent");
    let output_path = destination.path().join("report.json");
    fs::write(&output_path, "previous report\n").expect("existing destination");

    let directory_destination = destination.path().join("nonempty-destination");
    fs::create_dir(&directory_destination).expect("nonempty destination directory");
    let sentinel = directory_destination.join("sentinel.txt");
    fs::write(&sentinel, "keep me\n").expect("destination sentinel");
    let directory_failure = bifrost(project.root())
        .args([
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--format",
            "json",
            "--output",
        ])
        .arg(&directory_destination)
        .output()
        .expect("run platform-neutral persist failure");
    assert_status(&directory_failure, 2);
    assert!(directory_failure.stdout.is_empty());
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "keep me\n");
    assert!(
        String::from_utf8_lossy(&directory_failure.stderr).contains("failed to atomically replace")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let original = fs::metadata(destination.path()).unwrap().permissions();
        fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let failed = bifrost(project.root())
            .args([
                "--policy-file",
                "policies/dynamic-eval.rqlp",
                "--format",
                "json",
                "--output",
            ])
            .arg(&output_path)
            .output()
            .expect("run policy output failure");
        fs::set_permissions(destination.path(), original).unwrap();

        assert_status(&failed, 2);
        assert!(failed.stdout.is_empty());
        assert_eq!(
            fs::read_to_string(&output_path).unwrap(),
            "previous report\n"
        );
        assert!(String::from_utf8_lossy(&failed.stderr).contains("policy report output failed"));
    }
}

#[test]
fn policy_stderr_escapes_control_and_bidirectional_text() {
    let project = policy_project(&[]);
    let unsafe_text = "line\n\u{001B}[31m\u{202E}\u{2066}";

    let invalid_format = run(project.root(), &["--format", unsafe_text]);
    assert_status(&invalid_format, 2);
    assert!(invalid_format.stdout.is_empty());
    assert_single_terminal_safe_line(&invalid_format);

    let unsafe_destination = project.root().join(unsafe_text).join("report.json");
    let failed_output = bifrost(project.root())
        .args(["--policy-file", "policies/dynamic-eval.rqlp", "--output"])
        .arg(unsafe_destination)
        .output()
        .expect("run policy output failure with unsafe path");
    assert_status(&failed_output, 2);
    assert!(failed_output.stdout.is_empty());
    assert_single_terminal_safe_line(&failed_output);
}

#[test]
fn broken_stdout_pipe_is_an_operational_status_two_failure() {
    let project = policy_project(&[]);
    let mut child = bifrost(project.root())
        .args([
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--format",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn policy CLI");
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("wait for policy CLI");
    assert_status(&output, 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("policy report output failed"));
}

#[test]
fn output_path_may_be_outside_the_analyzed_workspace() {
    let project = policy_project(&[]);
    let outside = tempfile::tempdir().expect("outside output root");
    let output_path: PathBuf = outside.path().join("report.txt");
    let output = bifrost(project.root())
        .args(["--policy-file", "policies/dynamic-eval.rqlp", "--output"])
        .arg(&output_path)
        .output()
        .expect("write outside workspace");
    assert_status(&output, 1);
    assert!(output.stdout.is_empty());
    let file_output = fs::read(output_path).unwrap();
    assert!(!file_output.contains(&0x1b));
    assert!(
        String::from_utf8(file_output)
            .unwrap()
            .contains("Dynamic evaluation is forbidden")
    );
}

const DIFF_GATE_POLICY: &str = r#"(policy
  :schema-version 1
  :id "test.diff-gate"
  :name "Diff gate"
  :message "Avoid target"
  :severity warning
  :analysis
    (analysis
      :type match
      :selector
        (rql :schema-version 1
          (language typescript (function :name "target")))))"#;

const DIFF_GATE_ARGS: &[&str] = &[
    "--policy-file",
    "policies/diff-gate.rqlp",
    "--evaluation-date",
    "2026-07-27",
    "--fail-on",
    "warning",
];

/// One committed finding in `app.ts`; the policy file itself is committed too.
fn committed_diff_project() -> BuiltInlineTestProject {
    let project = InlineTestProject::new()
        .file("app.ts", "export function target() { return 1; }\n")
        .file("policies/diff-gate.rqlp", DIFF_GATE_POLICY)
        .build();
    crate::common::init_git_repo_with_identity(project.root());
    crate::common::run_git(project.root(), &["add", "."]);
    crate::common::run_git(project.root(), &["commit", "-m", "base"]);
    project
}

fn diff_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = DIFF_GATE_ARGS.to_vec();
    args.extend_from_slice(extra);
    args
}

#[test]
fn diff_base_narrows_the_gate_to_new_findings_across_formats() {
    let project = committed_diff_project();

    // Committed-only worktree: the persisting finding gates without the flag
    // and stops gating with it.
    let full = run(project.root(), &diff_args(&["--format", "json"]));
    assert_status(&full, 1);
    let full = json_stdout(&full);
    assert!(full.get("diff").is_none());
    assert!(full["runs"][0]["findings"][0].get("diff").is_none());
    let clean = run(project.root(), &diff_args(&["--diff-base", "HEAD"]));
    assert_status(&clean, 0);
    assert!(
        String::from_utf8_lossy(&clean.stdout)
            .contains("; diff: 0 new, 1 persisting, 0 fixed against HEAD")
    );

    // One new uncommitted finding gates, and all three formats agree on the
    // classification.
    fs::write(
        project.root().join("extra.ts"),
        "export function target() { return 2; }\n",
    )
    .expect("new offending source");
    let json = run(
        project.root(),
        &diff_args(&["--diff-base", "HEAD", "--format", "json"]),
    );
    assert_status(&json, 1);
    let report = json_stdout(&json);
    assert_eq!(report["diff"]["new_count"], 1);
    assert_eq!(report["diff"]["persisting_count"], 1);
    assert_eq!(report["diff"]["fixed_count"], 0);
    assert_eq!(report["diff"]["degraded"], false);
    for finding in report["runs"][0]["findings"].as_array().expect("findings") {
        let expected = match finding["primary"]["path"].as_str().expect("path") {
            "app.ts" => "persisting",
            "extra.ts" => "new",
            other => panic!("unexpected finding path {other}"),
        };
        assert_eq!(finding["diff"]["disposition"], expected);
    }

    let sarif = run(
        project.root(),
        &diff_args(&["--diff-base", "HEAD", "--format", "sarif"]),
    );
    assert_status(&sarif, 1);
    let sarif = json_stdout(&sarif);
    for result in sarif["runs"][0]["results"].as_array().expect("results") {
        let uri = result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .expect("uri");
        let expected = match uri {
            "app.ts" => "unchanged",
            "extra.ts" => "new",
            other => panic!("unexpected result uri {other}"),
        };
        assert_eq!(result["baselineState"], expected, "{result:#}");
    }
    assert_eq!(
        sarif["runs"][0]["properties"]["bifrost.diffBaseline"]["new_count"],
        1
    );

    let human = run(project.root(), &diff_args(&["--diff-base", "HEAD"]));
    assert_status(&human, 1);
    let human = String::from_utf8(human.stdout).expect("UTF-8 human output");
    assert!(human.contains("extra.ts"), "{human}");
    assert!(!human.contains("app.ts"), "{human}");
    assert!(
        human.contains("; diff: 1 new, 1 persisting, 0 fixed against HEAD"),
        "{human}"
    );
}

#[test]
fn suppressed_new_finding_does_not_gate_in_diff_mode() {
    let project = committed_diff_project();
    fs::write(
        project.root().join("extra.ts"),
        "export function target() { return 2; }\n",
    )
    .expect("new offending source");
    let baseline = run(
        project.root(),
        &diff_args(&["--diff-base", "HEAD", "--format", "json"]),
    );
    assert_status(&baseline, 1);
    let baseline = json_stdout(&baseline);
    let new_finding = baseline["runs"][0]["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|finding| finding["diff"]["disposition"] == "new")
        .expect("one new finding");
    let suppression_path = project.root().join(".bifrost/suppressions.json");
    fs::create_dir_all(suppression_path.parent().unwrap()).unwrap();
    fs::write(
        &suppression_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "suppressions": [{
                "policy_id": "test.diff-gate",
                "finding_id": new_finding["id"],
                "identity_stability": "strong",
                "status": "accepted",
                "reason": "Reviewed compatibility boundary",
                "policy_hash_at_acceptance": baseline["rules"][0]["policy_hash"],
                "accepted_by": "security-review",
                "accepted_at": "2026-07-01",
                "expires_at": "2026-07-27"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let suppressed = run(
        project.root(),
        &diff_args(&["--diff-base", "HEAD", "--format", "json"]),
    );
    assert_status(&suppressed, 0);
    let suppressed = json_stdout(&suppressed);
    // The classification is unchanged; only the gate is.
    assert_eq!(suppressed["diff"]["new_count"], 1);
    assert_eq!(suppressed["diff"]["persisting_count"], 1);
}

#[test]
fn unreliable_diff_base_degrades_to_full_gating_with_a_loud_diagnostic() {
    // The committed suppressions document is invalid, so the base evaluation
    // is unreliable; the repaired working tree keeps the head reliable.
    let project = InlineTestProject::new()
        .file("app.ts", "export function target() { return 1; }\n")
        .file("policies/diff-gate.rqlp", DIFF_GATE_POLICY)
        .file(".bifrost/suppressions.json", "{ not json")
        .build();
    crate::common::init_git_repo_with_identity(project.root());
    crate::common::run_git(project.root(), &["add", "."]);
    crate::common::run_git(project.root(), &["commit", "-m", "base"]);
    fs::remove_file(project.root().join(".bifrost/suppressions.json"))
        .expect("repair working tree");

    let output = run(
        project.root(),
        &diff_args(&["--diff-base", "HEAD", "--format", "json"]),
    );
    assert_status(&output, 2);
    let report = json_stdout(&output);
    assert_eq!(report["diff"]["degraded"], true);
    assert_eq!(report["diff"]["new_count"], 0);
    assert!(
        report["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "diff-base-unreliable"),
        "{report:#}"
    );
    // Full gating: the finding carries no diff decision and still counts.
    assert!(report["runs"][0]["findings"][0].get("diff").is_none());
}

#[test]
fn unresolvable_diff_base_and_non_git_root_exit_two() {
    let project = committed_diff_project();
    let unresolvable = run(
        project.root(),
        &diff_args(&["--diff-base", "does-not-exist"]),
    );
    assert_status(&unresolvable, 2);
    assert!(unresolvable.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&unresolvable.stderr).contains("does-not-exist"),
        "{}",
        String::from_utf8_lossy(&unresolvable.stderr)
    );

    let plain = InlineTestProject::new()
        .file("app.ts", "export function target() { return 1; }\n")
        .file("policies/diff-gate.rqlp", DIFF_GATE_POLICY)
        .build();
    let non_git = run(plain.root(), &diff_args(&["--diff-base", "HEAD"]));
    assert_status(&non_git, 2);
    assert!(
        String::from_utf8_lossy(&non_git.stderr).contains("not inside a git repository"),
        "{}",
        String::from_utf8_lossy(&non_git.stderr)
    );
}

/// Documents the accepted rename limitation: a pure rename re-keys every
/// finding in the file, so the diff reports one fixed plus one new pair. A
/// future rename-tracking improvement must change this test deliberately.
#[test]
fn pure_rename_currently_reports_fixed_plus_new() {
    let project = committed_diff_project();
    fs::rename(
        project.root().join("app.ts"),
        project.root().join("renamed.ts"),
    )
    .expect("rename tracked source");

    let output = run(
        project.root(),
        &diff_args(&["--diff-base", "HEAD", "--format", "json"]),
    );
    assert_status(&output, 1);
    let report = json_stdout(&output);
    assert_eq!(report["diff"]["new_count"], 1);
    assert_eq!(report["diff"]["persisting_count"], 0);
    assert_eq!(report["diff"]["fixed_count"], 1);
    assert_eq!(
        report["runs"][0]["findings"][0]["primary"]["path"],
        "renamed.ts"
    );
    assert_eq!(
        report["runs"][0]["findings"][0]["diff"]["disposition"],
        "new"
    );
}

const BASELINE_GATE_ARGS: &[&str] = &[
    "--policy-file",
    "policies/dynamic-eval.rqlp",
    "--evaluation-date",
    "2026-08-08",
    "--fail-on",
    "warning",
];

/// One Python workspace with `count` distinct offending call sites.
fn bulk_finding_project(count: usize) -> BuiltInlineTestProject {
    let mut source = String::new();
    for index in 0..count {
        source.push_str(&format!(
            "def run_{index}(value):\n    return eval(value)\n\n"
        ));
    }
    InlineTestProject::new()
        .file("src/bulk.py", source)
        .file("policies/dynamic-eval.rqlp", DYNAMIC)
        .build()
}

#[test]
fn accept_current_onboards_beyond_the_suppression_cap_and_new_findings_still_gate() {
    // 600 findings: beyond the 512-record suppression cap, within the
    // 1000-findings-per-policy retention budget.
    let project = bulk_finding_project(600);
    let gating = run(project.root(), BASELINE_GATE_ARGS);
    assert_status(&gating, 1);

    let accepted = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-08-08",
            "--accept-current",
        ],
    );
    assert_status(&accepted, 0);
    let stderr = String::from_utf8_lossy(&accepted.stderr);
    assert!(
        stderr.contains(
            "baseline accepted 600 findings into .bifrost/baseline.json \
             (0 weak-identity findings excluded)"
        ),
        "{stderr}"
    );
    let document: Value = serde_json::from_str(
        &fs::read_to_string(project.root().join(".bifrost/baseline.json"))
            .expect("baseline document written"),
    )
    .expect("baseline document is JSON");
    assert_eq!(document["schema_version"], 1);
    let finding_ids = document["policies"][0]["finding_ids"]
        .as_array()
        .expect("finding ids");
    assert_eq!(finding_ids.len(), 600);

    let clean = run(project.root(), BASELINE_GATE_ARGS);
    assert_status(&clean, 0);
    let summary = String::from_utf8_lossy(&clean.stdout).to_string();
    assert!(
        summary.contains("; baseline: 600 accepted of 600 entries via .bifrost/baseline.json"),
        "{summary}"
    );
    assert!(summary.contains("0 active findings"), "{summary}");

    // Regeneration is explicit and idempotent: a second acceptance rewrites
    // the same document.
    let reaccepted = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-08-08",
            "--accept-current",
        ],
    );
    assert_status(&reaccepted, 0);
    let rewritten: Value = serde_json::from_str(
        &fs::read_to_string(project.root().join(".bifrost/baseline.json"))
            .expect("baseline document rewritten"),
    )
    .expect("rewritten baseline is JSON");
    assert_eq!(rewritten, document);

    // A finding introduced after acceptance still gates.
    fs::write(
        project.root().join("src/regression.py"),
        "def run_more(value):\n    return eval(value)\n",
    )
    .expect("new offending source");
    let regressed = run(
        project.root(),
        &extended_args(BASELINE_GATE_ARGS, &["--format", "json"]),
    );
    assert_status(&regressed, 1);
    let report = json_stdout(&regressed);
    assert_eq!(report["baseline"]["entry_count"], 600);
    assert_eq!(report["baseline"]["applied_count"], 600);
    assert_eq!(report["baseline"]["result_omitted_count"], 0);
}

fn extended_args<'a>(base: &[&'a str], extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = base.to_vec();
    args.extend_from_slice(extra);
    args
}

#[test]
fn baseline_review_agrees_across_json_and_sarif_and_drift_does_not_reactivate() {
    let project = bulk_finding_project(1);
    let accepted = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-08-08",
            "--accept-current",
        ],
    );
    assert_status(&accepted, 0);

    // Edit the policy's presentation: the semantic hash changes, the entries
    // drift, and the accepted finding stays accepted.
    let changed_policy = DYNAMIC
        .replace(
            ":message \"Dynamic evaluation is forbidden\"",
            ":message \"Dynamic evaluation requires review\"",
        )
        .replace(":severity warning", ":severity error");
    assert_ne!(changed_policy, DYNAMIC);
    fs::write(
        project.root().join("policies/dynamic-eval.rqlp"),
        changed_policy,
    )
    .expect("edit policy presentation");

    let json = run(
        project.root(),
        &extended_args(BASELINE_GATE_ARGS, &["--format", "json"]),
    );
    assert_status(&json, 0);
    let json = json_stdout(&json);
    assert_eq!(json["baseline"]["entry_count"], 1);
    assert_eq!(json["baseline"]["applied_count"], 1);
    assert_eq!(json["baseline"]["drifted_count"], 1);
    assert_eq!(
        json["baseline"]["entries"][0]["policy_hash_state"],
        "drifted"
    );
    assert_eq!(json["baseline"]["entries"][0]["applied"], true);
    assert_eq!(
        json["runs"][0]["findings"][0]["baseline"]["policy_hash_state"],
        "drifted"
    );

    let sarif = run(
        project.root(),
        &extended_args(BASELINE_GATE_ARGS, &["--format", "sarif"]),
    );
    assert_status(&sarif, 0);
    let sarif = json_stdout(&sarif);
    // Cross-format agreement: the SARIF run property is the same canonical
    // review object the JSON report carries.
    assert_eq!(
        sarif["runs"][0]["properties"]["bifrost.baseline"],
        json["baseline"]
    );
    let suppression = &sarif["runs"][0]["results"][0]["suppressions"][0];
    assert_eq!(suppression["kind"], "external");
    assert_eq!(suppression["status"], "accepted");
    assert_eq!(suppression["properties"]["bifrost.decision"], "baseline");
    assert_eq!(
        suppression["properties"]["bifrost.policyHashState"],
        "drifted"
    );
}

#[test]
fn malformed_baseline_is_exit_two_and_an_unreliable_run_refuses_acceptance() {
    // A malformed baseline document is a diagnostic and exit 2 on every run,
    // including an acceptance run, which must not overwrite it.
    let project = bulk_finding_project(1);
    let baseline_path = project.root().join(".bifrost/baseline.json");
    fs::create_dir_all(baseline_path.parent().unwrap()).unwrap();
    fs::write(&baseline_path, "{ not json").unwrap();

    let gating = run(
        project.root(),
        &extended_args(BASELINE_GATE_ARGS, &["--format", "json"]),
    );
    assert_status(&gating, 2);
    let report = json_stdout(&gating);
    assert!(report.get("baseline").is_none());
    assert!(
        report["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "baseline-load-failed"),
        "{report:#}"
    );

    let refused = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-08-08",
            "--accept-current",
        ],
    );
    assert_status(&refused, 2);
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("no baseline was written"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        fs::read_to_string(&baseline_path).unwrap(),
        "{ not json",
        "a refused acceptance must not touch the document"
    );

    // Any other unreliability refuses acceptance the same way and writes
    // nothing at all.
    fs::remove_file(&baseline_path).unwrap();
    fs::write(
        project.root().join(".bifrost/suppressions.json"),
        "{ not json",
    )
    .unwrap();
    let unreliable = run(
        project.root(),
        &[
            "--policy-file",
            "policies/dynamic-eval.rqlp",
            "--evaluation-date",
            "2026-08-08",
            "--accept-current",
        ],
    );
    assert_status(&unreliable, 2);
    assert!(
        !baseline_path.exists(),
        "an unreliable run cannot define a baseline"
    );
}

/// A minimal JDK stdlib pack pinned to exactly 21.0.2, installable into a
/// workspace-configured catalog so a CI-shaped run can activate it (#1868).
const JDK_21_FIXTURE_PACK: &str = r#"{
  "schema_version": 1,
  "pack_id": "fixture.jdk",
  "version": "21.0.2",
  "producer": { "name": "bifrost-fixture", "version": "1.0.0" },
  "language": "java",
  "ecosystem": "jdk",
  "compatibility": {
    "bifrost": ">=0.8.0, <1.0.0",
    "toolchains": [{ "name": "jdk", "requirement": "=21.0.2" }]
  },
  "provenance": { "source": "checked-in test source", "revision": "fixture-v1" },
  "license": "GPL-2.0-only WITH Classpath-exception-2.0",
  "completeness": "complete",
  "safety": { "generated_code_only": false, "review_required": false },
  "shards": [{
    "id": "jdk.core",
    "activation": [{
      "toolchain": { "name": "jdk", "version": "=21.0.2" },
      "targets": ["jvm"]
    }],
    "payload": {
      "kind": "declaration_facts",
      "types": [{
        "id": "jdk.java-util-arraylist",
        "name": "java.util.ArrayList",
        "type_kind": "class",
        "visibility": "public",
        "type_parameters": [],
        "hierarchy": [],
        "aliases": [],
        "extension_surfaces": [],
        "locator": {
          "kind": "artifact",
          "path": "java.base/java/util/ArrayList.java",
          "symbol": "java.util.ArrayList"
        }
      }],
      "members": [],
      "relations": []
    }
  }]
}"#;

/// One Java workspace whose packs document opts into the jvm ecosystem and
/// names a catalog pre-loaded with the JDK 21 fixture pack.
fn packs_document_project() -> BuiltInlineTestProject {
    let project = InlineTestProject::new()
        .file("src/Main.java", JAVA_RESOURCE_SOURCE)
        .file(
            ".bifrost/packs.json",
            r#"{ "schema_version": 1, "catalog": ".bifrost/packs-catalog", "ecosystems": ["jvm"] }"#,
        )
        .build();
    install_jdk_fixture_pack(project.root());
    project
}

/// Compile the JDK 21 fixture pack and install it into the workspace's
/// configured catalog directory.
fn install_jdk_fixture_pack(root: &Path) {
    install_fixture_pack(root, JDK_21_FIXTURE_PACK);
}

/// Compile `source` as a fixture pack and install it into the workspace's
/// configured catalog directory.
fn install_fixture_pack(root: &Path, source: &str) {
    use brokk_bifrost::analyzer::semantic_model::{
        CatalogOpenMode, CatalogOptions, CompilerOptions, DurablePackSource, DurablePackSourceKind,
        SemanticPackCatalog, SourceFormat, compile_source,
    };
    let compiled = compile_source(
        SourceFormat::Json,
        source.as_bytes(),
        &CompilerOptions::default(),
    )
    .unwrap_or_else(|diagnostics| panic!("fixture pack compilation failed: {diagnostics:#?}"));
    let catalog = SemanticPackCatalog::open(
        &root.join(".bifrost/packs-catalog"),
        CatalogOpenMode::ReadWrite,
        CatalogOptions::default(),
    )
    .expect("open the workspace-configured catalog");
    catalog
        .install(
            &compiled,
            &DurablePackSource {
                kind: DurablePackSourceKind::PreShipped,
                source_id: "test:fixture.jdk@21.0.2".to_owned(),
            },
        )
        .expect("install the fixture pack");
}

/// One fake JDK home: a `release` file with the exact version and no
/// `src.zip`, so discovery must select an installed pack or reject loudly.
fn fake_jdk_home(root: &Path, version: &str) -> PathBuf {
    let home = root.join(format!("jdk-{version}"));
    fs::create_dir_all(&home).expect("create fake JDK home");
    fs::write(
        home.join("release"),
        format!("JAVA_VERSION=\"{version}\"\n"),
    )
    .expect("write JDK release file");
    home
}

fn run_with_java_home(root: &Path, home: &Path, args: &[&str]) -> Output {
    bifrost(root)
        .env("JAVA_HOME", home)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run bifrost {args:?}: {error}"))
}

const PACKS_RUN_ARGS: &[&str] = &[
    "--policy-id",
    "bifrost.correctness.dynamic-evaluation",
    "--evaluation-date",
    "2026-07-28",
    "--fail-on",
    "never",
    "--format",
    "json",
];

#[test]
fn packs_document_activation_decisions_appear_in_the_policy_report() {
    let project = packs_document_project();
    let homes = tempfile::tempdir().expect("fake JDK home root");

    // Exact toolchain match: the installed pack is selected and the report
    // says so.
    let matched = run_with_java_home(
        project.root(),
        &fake_jdk_home(homes.path(), "21.0.2"),
        PACKS_RUN_ARGS,
    );
    assert_status(&matched, 0);
    let report = json_stdout(&matched);
    assert_eq!(report["packs"]["document_path"], ".bifrost/packs.json");
    assert_eq!(report["packs"]["ecosystems"], serde_json::json!(["jvm"]));
    assert_eq!(report["packs"]["complete"], true);
    let decisions = report["packs"]["decisions"].as_array().expect("decisions");
    assert!(
        decisions.iter().any(|decision| {
            decision["pack"] == "fixture.jdk@21.0.2" && decision["status"] == "selected"
        }),
        "{decisions:#?}"
    );

    // Near miss: a JDK 17 workspace against the JDK 21 pack never activates
    // silently; the decision names the installed and required versions.
    let near_miss = run_with_java_home(
        project.root(),
        &fake_jdk_home(homes.path(), "17.0.10"),
        PACKS_RUN_ARGS,
    );
    let report = json_stdout(&near_miss);
    assert_eq!(report["packs"]["complete"], false);
    let decisions = report["packs"]["decisions"].as_array().expect("decisions");
    let mismatch = decisions
        .iter()
        .find(|decision| decision["status"] == "version_mismatch")
        .unwrap_or_else(|| panic!("{decisions:#?}"));
    let reason = mismatch["reason"].as_str().expect("mismatch reason");
    assert!(
        reason.contains("17.0.10") && reason.contains("=21.0.2"),
        "the decision must name the installed and required versions: {reason}"
    );
    assert!(
        !decisions
            .iter()
            .any(|decision| decision["status"] == "selected"),
        "a near miss must not select any pack: {decisions:#?}"
    );

    // SARIF surfaces the same review as a run property.
    let sarif = run_with_java_home(
        project.root(),
        &fake_jdk_home(homes.path(), "21.0.2"),
        &[
            "--policy-id",
            "bifrost.correctness.dynamic-evaluation",
            "--evaluation-date",
            "2026-07-28",
            "--fail-on",
            "never",
            "--format",
            "sarif",
        ],
    );
    assert_status(&sarif, 0);
    let log = json_stdout(&sarif);
    let properties = &log["runs"][0]["properties"];
    assert_eq!(
        properties["bifrost.packActivation"]["ecosystems"],
        serde_json::json!(["jvm"])
    );
    assert_eq!(properties["bifrost.packActivation"]["complete"], true);
}

#[test]
fn a_run_without_a_packs_document_reports_no_packs_field() {
    let project = policy_project(&[]);
    let output = run(project.root(), PACKS_RUN_ARGS);
    assert_status(&output, 0);
    let report = json_stdout(&output);
    assert!(
        report.get("packs").is_none(),
        "a run without a packs document keeps the exact schema-version-3 shape"
    );
}

#[test]
fn a_malformed_packs_document_is_loud_and_makes_the_run_unreliable() {
    let project = InlineTestProject::new()
        .file("src/app.py", APP)
        .file(
            ".bifrost/packs.json",
            r#"{ "schema_version": 1, "ecosystems": ["jdk"] }"#,
        )
        .build();
    let output = run(project.root(), PACKS_RUN_ARGS);
    assert_status(&output, 2);
    let report = json_stdout(&output);
    let diagnostics = report["diagnostics"].as_array().expect("diagnostics");
    let failure = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == "packs-load-failed")
        .unwrap_or_else(|| panic!("{diagnostics:#?}"));
    let message = failure["message"].as_str().expect("diagnostic message");
    assert!(
        message.contains("unknown ecosystem") && message.contains("jdk"),
        "the diagnostic must name the invalid ecosystem: {message}"
    );
    assert!(report.get("packs").is_none());
}

/// One Java file whose only unresolved references are standard-library: the
/// probe workspace for the epic #1877 acceptance shape. `ArrayList` is a
/// type-position reference and `Collections` a static receiver, so the file
/// carries both site shapes the resolution asserts can examine.
const STDLIB_PROBE_SOURCE: &str = r#"import java.util.ArrayList;
import java.util.Collections;

class Main {
  int run() {
    ArrayList<String> names = new ArrayList<>();
    names.add("x");
    Collections.sort(names);
    return names.size();
  }
}
"#;

/// The JDK fixture pack extended with every standard-library type the probe
/// workspace names, so an exhaustive assertion over that workspace has a
/// declaration to reach for each one.
const STDLIB_PROBE_PACK: &str = r#"{
  "schema_version": 1,
  "pack_id": "fixture.jdk",
  "version": "21.0.2",
  "producer": { "name": "bifrost-fixture", "version": "1.0.0" },
  "language": "java",
  "ecosystem": "jdk",
  "compatibility": {
    "bifrost": ">=0.8.0, <1.0.0",
    "toolchains": [{ "name": "jdk", "requirement": "=21.0.2" }]
  },
  "provenance": { "source": "checked-in test source", "revision": "fixture-v1" },
  "license": "GPL-2.0-only WITH Classpath-exception-2.0",
  "completeness": "complete",
  "safety": { "generated_code_only": false, "review_required": false },
  "shards": [{
    "id": "jdk.core",
    "activation": [{
      "toolchain": { "name": "jdk", "version": "=21.0.2" },
      "targets": ["jvm"]
    }],
    "payload": {
      "kind": "declaration_facts",
      "types": [{
        "id": "jdk.java-util-arraylist",
        "name": "java.util.ArrayList",
        "type_kind": "class",
        "visibility": "public",
        "type_parameters": ["E"],
        "hierarchy": [],
        "aliases": [],
        "extension_surfaces": [],
        "locator": {
          "kind": "artifact",
          "path": "java.base/java/util/ArrayList.java",
          "symbol": "java.util.ArrayList"
        }
      }, {
        "id": "jdk.java-util-collections",
        "name": "java.util.Collections",
        "type_kind": "class",
        "visibility": "public",
        "type_parameters": [],
        "hierarchy": [],
        "aliases": [],
        "extension_surfaces": [],
        "locator": {
          "kind": "artifact",
          "path": "java.base/java/util/Collections.java",
          "symbol": "java.util.Collections"
        }
      }],
      "members": [{
        "id": "jdk.java-util-collections.sort",
        "owner": "jdk.java-util-collections",
        "name": "sort",
        "member_kind": "method",
        "visibility": "public",
        "is_static": true,
        "locator": {
          "kind": "artifact",
          "path": "java.base/java/util/Collections.java",
          "symbol": "java.util.Collections.sort"
        }
      }],
      "relations": []
    }
  }]
}"#;

/// The epic #1877 contract over the stdlib reference: the site resolves
/// through a real route (`external_root` or stronger, never name fallback),
/// and the winning selection never falls back past an authoritative boundary.
///
/// The asserted site is the type-position `ArrayList`, so this policy measures
/// the *type* half of the external surface. [`STDLIB_MEMBER_POLICY`] asserts
/// the same contract over the member half.
const STDLIB_BOUNDARY_POLICY: &str = r#"(policy
  :schema-version 1
  :id "probe.stdlib.boundary"
  :name "Stdlib references resolve without name fallback"
  :message "a standard-library reference did not resolve through a real route"
  :severity warning
  :analysis (analysis
    :type assertion
    :subject (rql (identifier :text/regex "^ArrayList$" :capture "target"))
    :asserts [(assert-resolution :id stdlib-route :at "target" :role type_operand
                :expect-tier external_root :at-least true)
              (assert-boundary :id stdlib-boundary :at "target" :role type_operand
                :forbid-fallback-past external_declared_unindexed)]))
"#;

/// The same epic #1877 contract asserted over the *member* half of the
/// external surface (#1900).
///
/// The subject is the static receiver `Collections`, whose reference site spans
/// the whole written name `Collections.sort`: a type for its head and a member
/// for its last segment. The fixture pack declares both, and only the member
/// declaration can carry this assertion -- the type alone leaves the spelling
/// unresolved, which is exactly why #1893 had to assert the type-position site.
const STDLIB_MEMBER_POLICY: &str = r#"(policy
  :schema-version 1
  :id "probe.stdlib.member"
  :name "Stdlib member references resolve without name fallback"
  :message "a standard-library member reference did not resolve through a real route"
  :severity warning
  :analysis (analysis
    :type assertion
    :subject (rql (identifier :text/regex "^Collections$" :capture "target"))
    :asserts [(assert-resolution :id stdlib-member-route :at "target" :role receiver_position
                :expect-tier external_root :at-least true)
              (assert-boundary :id stdlib-member-boundary :at "target" :role receiver_position
                :forbid-fallback-past external_declared_unindexed)]))
"#;

const STDLIB_PROBE_ARGS: &[&str] = &[
    "--policy-file",
    "policies/stdlib-boundary.rqlp",
    "--evaluation-date",
    "2026-07-28",
    "--fail-on",
    "warning",
    "--format",
    "json",
];

const STDLIB_MEMBER_ARGS: &[&str] = &[
    "--policy-file",
    "policies/stdlib-member.rqlp",
    "--evaluation-date",
    "2026-07-28",
    "--fail-on",
    "warning",
    "--format",
    "json",
];

/// The epic #1877 acceptance shape, end to end: a fixture that references a
/// standard-library API runs an exhaustive resolution-and-boundary assertion
/// to completion. With the JDK fixture pack selected the run concludes (exit
/// 0 or 1, never 2). Without a packs document the same policy is inconclusive
/// and the run honestly exits 2 instead of passing vacuously.
///
/// The whole chain runs here: the packs document opts the workspace in, the
/// catalog serves the installed pack, the fake JDK home satisfies the
/// toolchain gate, the pack's declaration facts reach the JVM external
/// declaration surface, and the resolver selects the external route they name
/// (#1893). No jar and no `src.zip` exists anywhere in the fixture, so the
/// pack is the only thing that can carry the assertion.
#[test]
fn packs_document_lets_a_stdlib_boundary_assertion_conclude() {
    let with_pack = InlineTestProject::new()
        .file("src/Main.java", STDLIB_PROBE_SOURCE)
        .file("policies/stdlib-boundary.rqlp", STDLIB_BOUNDARY_POLICY)
        .file(
            ".bifrost/packs.json",
            r#"{ "schema_version": 1, "catalog": ".bifrost/packs-catalog", "ecosystems": ["jvm"] }"#,
        )
        .build();
    install_fixture_pack(with_pack.root(), STDLIB_PROBE_PACK);
    let homes = tempfile::tempdir().expect("fake JDK home root");
    let concluded = run_with_java_home(
        with_pack.root(),
        &fake_jdk_home(homes.path(), "21.0.2"),
        STDLIB_PROBE_ARGS,
    );
    let code = concluded.status.code();
    assert!(
        code == Some(0) || code == Some(1),
        "with the pack selected the assertion must conclude, not report \
         unreliable: exit {code:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&concluded.stdout),
        String::from_utf8_lossy(&concluded.stderr)
    );
    let report = json_stdout(&concluded);
    assert_eq!(report["packs"]["complete"], true, "report: {report}");
    assert_eq!(
        report["runs"][0]["completion"]["type"], "complete",
        "report: {report}"
    );
    // A prohibition over zero rows is complete-and-clean by design, so
    // "complete" alone could also describe a run that concluded vacuously.
    // The paired run below is the discriminator: the same policy over the same
    // workspace is inconclusive without the pack, so only the pack can have
    // carried this conclusion.
    let without_pack = InlineTestProject::new()
        .file("src/Main.java", STDLIB_PROBE_SOURCE)
        .file("policies/stdlib-boundary.rqlp", STDLIB_BOUNDARY_POLICY)
        .build();
    let degraded = run(without_pack.root(), STDLIB_PROBE_ARGS);
    assert_status(&degraded, 2);
    let degraded_report = json_stdout(&degraded);
    assert_eq!(
        degraded_report["runs"][0]["completion"]["type"], "inconclusive",
        "without the pack the assertion must be inconclusive, not vacuously \
         clean: {degraded_report}"
    );
}

/// The same acceptance shape over the member half of the JVM external surface
/// (#1900): the asserted site is the static receiver of `Collections.sort`,
/// whose written name spells a member rather than a type.
///
/// #1893 had to assert a type-position site because no JVM external surface
/// carried member declarations at all. With the pack's `sort` member reaching
/// the surface, the receiver-position assertion concludes; the paired run
/// without a packs document stays honestly inconclusive, so only the pack's
/// member declaration can have carried it.
#[test]
fn packs_document_lets_a_stdlib_member_assertion_conclude() {
    let with_pack = InlineTestProject::new()
        .file("src/Main.java", STDLIB_PROBE_SOURCE)
        .file("policies/stdlib-member.rqlp", STDLIB_MEMBER_POLICY)
        .file(
            ".bifrost/packs.json",
            r#"{ "schema_version": 1, "catalog": ".bifrost/packs-catalog", "ecosystems": ["jvm"] }"#,
        )
        .build();
    install_fixture_pack(with_pack.root(), STDLIB_PROBE_PACK);
    let homes = tempfile::tempdir().expect("fake JDK home root");
    let concluded = run_with_java_home(
        with_pack.root(),
        &fake_jdk_home(homes.path(), "21.0.2"),
        STDLIB_MEMBER_ARGS,
    );
    let code = concluded.status.code();
    assert!(
        code == Some(0) || code == Some(1),
        "with the pack's member declaration the assertion must conclude: exit \
         {code:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&concluded.stdout),
        String::from_utf8_lossy(&concluded.stderr)
    );
    let report = json_stdout(&concluded);
    assert_eq!(report["packs"]["complete"], true, "report: {report}");
    assert_eq!(
        report["runs"][0]["completion"]["type"], "complete",
        "report: {report}"
    );

    let without_pack = InlineTestProject::new()
        .file("src/Main.java", STDLIB_PROBE_SOURCE)
        .file("policies/stdlib-member.rqlp", STDLIB_MEMBER_POLICY)
        .build();
    let degraded = run(without_pack.root(), STDLIB_MEMBER_ARGS);
    assert_status(&degraded, 2);
    let degraded_report = json_stdout(&degraded);
    assert_eq!(
        degraded_report["runs"][0]["completion"]["type"], "inconclusive",
        "without the pack the member assertion must be inconclusive, not \
         vacuously clean: {degraded_report}"
    );

    // The discriminator that names the *member*: the very same pack with its
    // members emptied still declares `java.util.Collections` and is still
    // selected, and the assertion still cannot conclude. So the conclusion
    // above rests on the `sort` declaration and not on the type, and a pack
    // that declares no members proves nothing about them.
    let mut types_only: serde_json::Value =
        serde_json::from_str(STDLIB_PROBE_PACK).expect("the fixture pack is JSON");
    types_only["shards"][0]["payload"]["members"] = serde_json::json!([]);
    let without_member = InlineTestProject::new()
        .file("src/Main.java", STDLIB_PROBE_SOURCE)
        .file("policies/stdlib-member.rqlp", STDLIB_MEMBER_POLICY)
        .file(
            ".bifrost/packs.json",
            r#"{ "schema_version": 1, "catalog": ".bifrost/packs-catalog", "ecosystems": ["jvm"] }"#,
        )
        .build();
    install_fixture_pack(without_member.root(), &types_only.to_string());
    let type_only = run_with_java_home(
        without_member.root(),
        &fake_jdk_home(homes.path(), "21.0.2"),
        STDLIB_MEMBER_ARGS,
    );
    assert_status(&type_only, 2);
    let type_only_report = json_stdout(&type_only);
    assert_eq!(
        type_only_report["packs"]["complete"], true,
        "the type-only pack still activates: {type_only_report}"
    );
    assert_eq!(
        type_only_report["runs"][0]["completion"]["type"], "inconclusive",
        "a pack that declares the owner type and no members must not carry a \
         member assertion: {type_only_report}"
    );
}

/// The one packs document every taint-summary case below opts into.
const PACKS_DOCUMENT_JVM: &str =
    r#"{ "schema_version": 1, "catalog": ".bifrost/packs-catalog", "ecosystems": ["jvm"] }"#;

/// A JVM workspace whose only cross-procedure taint path runs through a
/// bodiless method the workspace cannot analyze. `run` carries an
/// attacker-controlled value into `external` and the result into a sensitive
/// sink, so the flow reaches the sink only when a model transfers the argument
/// to the return. This mirrors the `s1_d1_src1_sink1` shape the summary-taint
/// lifecycle benchmark drives through the in-process API path.
const SUMMARY_TAINT_SOURCE: &str = r#"class App {
  static native String attacker();

  static native void sensitive(String value);

  native String external(String value);

  void run() {
    sensitive(this.external(attacker()));
  }
}
"#;

/// A `require-model` taint policy: the attacker return is untrusted, the sink
/// rejects untrusted argument 0, and an unmodeled call is required to carry a
/// model rather than be assumed transparent or inert. Without a model for
/// `external` the run is honestly inconclusive; with one it concludes with the
/// meeting.
const SUMMARY_TAINT_POLICY: &str = r#"(policy
  :schema-version 1
  :id "probe.summary.taint"
  :name "Summary-carried taint reaches a sink"
  :message "an attacker-controlled value reached a sensitive sink through an activated procedure summary"
  :severity warning
  :analysis (analysis
    :type taint
    :mode may
    :call-modeling (call-modeling :unmodeled require-model)
    :sources (endpoint-set :entries [
      (source :id attacker :display-name "attacker" :categories [input.user]
        :selector (rql :schema-version 1 (language java (call :callee (name "attacker"))))
        :bind return-value :labels [untrusted])])
    :sinks (endpoint-set :entries [
      (sink :id sensitive :display-name "sensitive" :categories [data.sensitive]
        :selector (rql :schema-version 1 (language java (call :callee (name "sensitive"))))
        :dangerous-operand (argument :index 0) :accepts [untrusted])]))
  :classification (classification
    :fallback (classification-id :taxonomy "Probe" :id "SUMMARY-TAINT")))
"#;

/// A procedure-summary pack that models `external` by transferring its argument
/// to its normal return. It activates through the exact JDK 21.0.2 toolchain
/// selector the declaration-facts stdlib packs use, so the document route's JVM
/// discovery selects it from the fake JDK home with no jar on disk; the payload
/// is the taint route's summary strand rather than declaration facts.
const SUMMARY_TAINT_PACK: &str = r#"{
  "schema_version": 1,
  "pack_id": "fixture.jdk",
  "version": "21.0.2",
  "producer": { "name": "bifrost-fixture", "version": "1.0.0" },
  "language": "java",
  "ecosystem": "jdk",
  "compatibility": {
    "bifrost": ">=0.8.0, <1.0.0",
    "toolchains": [{ "name": "jdk", "requirement": "=21.0.2" }]
  },
  "provenance": { "source": "checked-in test source", "revision": "fixture-v1" },
  "license": "GPL-2.0-only WITH Classpath-exception-2.0",
  "completeness": "complete",
  "safety": { "generated_code_only": false, "review_required": false },
  "shards": [{
    "id": "summaries.probe",
    "activation": [{
      "toolchain": { "name": "jdk", "version": "=21.0.2" },
      "targets": ["jvm"]
    }],
    "payload": {
      "kind": "procedure_summaries",
      "summaries": [{
        "id": "summary.external",
        "target": {
          "path": "src/App.java",
          "symbol": "external(String)",
          "has_receiver": true,
          "parameter_count": 1
        },
        "completeness": "complete",
        "transfers": [{
          "input": { "kind": "parameter", "ordinal": 0 },
          "exit_kind": "normal",
          "output": { "kind": "normal_return" }
        }],
        "effects": []
      }]
    }
  }]
}"#;

/// The same pack, selected identically, whose one summary targets a method the
/// workspace never calls. `external` therefore stays unmodeled, so a selection
/// alone must not manufacture a finding.
const SUMMARY_TAINT_UNCOVERED_PACK: &str = r#"{
  "schema_version": 1,
  "pack_id": "fixture.jdk",
  "version": "21.0.2",
  "producer": { "name": "bifrost-fixture", "version": "1.0.0" },
  "language": "java",
  "ecosystem": "jdk",
  "compatibility": {
    "bifrost": ">=0.8.0, <1.0.0",
    "toolchains": [{ "name": "jdk", "requirement": "=21.0.2" }]
  },
  "provenance": { "source": "checked-in test source", "revision": "fixture-v1" },
  "license": "GPL-2.0-only WITH Classpath-exception-2.0",
  "completeness": "complete",
  "safety": { "generated_code_only": false, "review_required": false },
  "shards": [{
    "id": "summaries.probe",
    "activation": [{
      "toolchain": { "name": "jdk", "version": "=21.0.2" },
      "targets": ["jvm"]
    }],
    "payload": {
      "kind": "procedure_summaries",
      "summaries": [{
        "id": "summary.uncovered",
        "target": {
          "path": "src/App.java",
          "symbol": "uncovered(String)",
          "has_receiver": true,
          "parameter_count": 1
        },
        "completeness": "complete",
        "transfers": [{
          "input": { "kind": "parameter", "ordinal": 0 },
          "exit_kind": "normal",
          "output": { "kind": "normal_return" }
        }],
        "effects": []
      }]
    }
  }]
}"#;

const SUMMARY_TAINT_ARGS: &[&str] = &[
    "--policy-file",
    "policies/summary-taint.rqlp",
    "--evaluation-date",
    "2026-07-28",
    "--fail-on",
    "warning",
    "--format",
    "json",
];

/// The epic #1877 taint acceptance, end to end through the shipped CLI: a
/// workspace with an installed, selected procedure-summary pack yields the
/// taint finding when the policy runs through the packs.json document route,
/// and yields nothing with the pack absent (#1915).
///
/// Before #1915 an activated summary pack changed taint results only for an
/// in-process API caller, because the CLI/document route passed no
/// `PolicySemanticModelContext`. The whole chain now runs here: the packs
/// document opts the workspace in, the catalog serves the installed summary
/// pack, the fake JDK home satisfies the toolchain gate, and the resolved
/// activation the document transaction already built now reaches the taint
/// evaluator. No jar and no `src.zip` exists anywhere, so the summary pack is
/// the only thing that can model `external` and carry the flow.
#[test]
fn packs_document_carries_procedure_summaries_into_the_cli_taint_route() {
    let homes = tempfile::tempdir().expect("fake JDK home root");

    // With the summary pack selected, `external` is modeled, so the flow
    // attacker -> external -> sensitive reaches the sink and retains a finding.
    let with_pack = InlineTestProject::new()
        .file("src/App.java", SUMMARY_TAINT_SOURCE)
        .file("policies/summary-taint.rqlp", SUMMARY_TAINT_POLICY)
        .file(".bifrost/packs.json", PACKS_DOCUMENT_JVM)
        .build();
    install_fixture_pack(with_pack.root(), SUMMARY_TAINT_PACK);
    let modeled = run_with_java_home(
        with_pack.root(),
        &fake_jdk_home(homes.path(), "21.0.2"),
        SUMMARY_TAINT_ARGS,
    );
    let report = json_stdout(&modeled);
    let decisions = report["packs"]["decisions"].as_array().expect("decisions");
    assert!(
        decisions.iter().any(|decision| {
            decision["pack"] == "fixture.jdk@21.0.2" && decision["status"] == "selected"
        }),
        "the summary pack must activate through the document route: {decisions:#?}"
    );
    let findings = report["runs"][0]["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("findings array: {report}"));
    assert_eq!(
        findings.len(),
        1,
        "the activated summary must carry the taint to the sink through the CLI \
         route: {report}"
    );

    // Fail-before control: the same workspace and policy with no packs
    // document. `external` is unmodeled, so `require-model` yields no finding
    // and the run stays honestly inconclusive rather than vacuously clean. Only
    // the pack can have carried the finding above.
    let without_pack = InlineTestProject::new()
        .file("src/App.java", SUMMARY_TAINT_SOURCE)
        .file("policies/summary-taint.rqlp", SUMMARY_TAINT_POLICY)
        .build();
    let unmodeled = run(without_pack.root(), SUMMARY_TAINT_ARGS);
    let unmodeled_report = json_stdout(&unmodeled);
    assert!(
        unmodeled_report.get("packs").is_none(),
        "a run without a packs document activates nothing: {unmodeled_report}"
    );
    assert!(
        unmodeled_report["runs"][0]["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("findings array: {unmodeled_report}"))
            .is_empty(),
        "without a model the flow must not reach the sink: {unmodeled_report}"
    );
    assert_eq!(
        unmodeled_report["runs"][0]["completion"]["type"], "inconclusive",
        "the unmodeled external call must surface as incompleteness, not a clean \
         pass: {unmodeled_report}"
    );

    // Honesty near miss: a pack that is selected but whose summary does not
    // cover `external` must not manufacture a finding. Activation carries the
    // summaries into taint exactly as above, but the uncovered call stays
    // unmodeled, so the run matches the fail-before control.
    let uncovered = InlineTestProject::new()
        .file("src/App.java", SUMMARY_TAINT_SOURCE)
        .file("policies/summary-taint.rqlp", SUMMARY_TAINT_POLICY)
        .file(".bifrost/packs.json", PACKS_DOCUMENT_JVM)
        .build();
    install_fixture_pack(uncovered.root(), SUMMARY_TAINT_UNCOVERED_PACK);
    let selected_but_uncovered = run_with_java_home(
        uncovered.root(),
        &fake_jdk_home(homes.path(), "21.0.2"),
        SUMMARY_TAINT_ARGS,
    );
    let uncovered_report = json_stdout(&selected_but_uncovered);
    let decisions = uncovered_report["packs"]["decisions"]
        .as_array()
        .expect("decisions");
    assert!(
        decisions.iter().any(|decision| {
            decision["pack"] == "fixture.jdk@21.0.2" && decision["status"] == "selected"
        }),
        "the uncovered pack still activates through the document route: {decisions:#?}"
    );
    assert!(
        uncovered_report["runs"][0]["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("findings array: {uncovered_report}"))
            .is_empty(),
        "a selected pack whose summary does not cover the call must not \
         manufacture a finding: {uncovered_report}"
    );
}
