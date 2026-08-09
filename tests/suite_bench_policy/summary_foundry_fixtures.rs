//! Milestone 3 acceptance for the procedure-summary foundry (#1871).
//!
//! The fixture engine must produce passing fixtures for twenty hand-picked
//! known-good entries and must fail a deliberately corrupted one. Every case
//! here runs through the production path: the generated pack goes through
//! `compile_source`, the workspace through the production analyzer, and the
//! generated `require-model` taint policy through
//! `evaluate_policy_inputs_with_analyzer_and_semantic_models`.
//!
//! The twenty entries come from all three places the foundry gets content:
//! milestone 2's derivation over the pinned `java.util.Objects` slice, one
//! milestone 1 translation of a signed CodeQL Models-as-Data row, and fifteen
//! hand-authored `String.format`-shaped propagations that stand in for the
//! adjudicated content stage 3 will produce.

use std::path::{Path, PathBuf};

use brokk_bifrost::analyzer::semantic_model::{
    AuthoredSummaryExitKind, AuthoredSummaryInput, AuthoredSummaryOutput, AuthoredSummaryTransfer,
};
use brokk_bifrost::semantic_packs::summary_foundry::codeql::translate_codeql_models;
use brokk_bifrost::semantic_packs::summary_foundry::derive::{
    DerivationLimits, derive_jvm_summaries,
};
use brokk_bifrost::semantic_packs::summary_foundry::fixture::{
    FixturePolarity, FixtureUnsupported, JavaFixtureLanguage, generate_fixture_suite,
};
use brokk_bifrost::semantic_packs::summary_foundry::ir::{
    FoundryArtifactBinding, FoundryBoundary, FoundryClaim, FoundryCompleteness, FoundryCorpus,
    FoundryEntry, FoundrySignature, FoundryTarget, summary_id,
};
use brokk_bifrost::semantic_packs::summary_foundry::read_codeql_models;
use brokk_bifrost::summary_foundry_fixtures::{FixtureRunReport, run_fixture_suite};

/// One input port a hand-authored classic claims.
#[derive(Debug, Clone, Copy)]
enum Port {
    Receiver,
    Parameter(u32),
}

impl Port {
    fn authored(self) -> AuthoredSummaryInput {
        match self {
            Self::Receiver => AuthoredSummaryInput::Receiver {},
            Self::Parameter(ordinal) => AuthoredSummaryInput::Parameter { ordinal },
        }
    }
}

/// One hand-authored classic: a propagation every JVM taint model states, in
/// the foundry's own IR.
struct Classic {
    artifact_path: &'static str,
    member: &'static str,
    types: &'static [&'static str],
    has_receiver: bool,
    inputs: &'static [Port],
    complete: bool,
}

/// Fifteen `String.format`-shaped propagations.
///
/// Three claim `complete`, which is what gives the suite its negative
/// polarity: only a complete claim can deny a flow, so only a complete entry
/// gets a fixture that proves taint does *not* cross where the summary says it
/// does not.
const CLASSICS: &[Classic] = &[
    Classic {
        artifact_path: "java/lang/String.class",
        member: "format",
        types: &["String", "Object"],
        has_receiver: false,
        inputs: &[Port::Parameter(0), Port::Parameter(1)],
        complete: false,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "valueOf",
        types: &["Object"],
        has_receiver: false,
        inputs: &[Port::Parameter(0)],
        complete: false,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "substring",
        types: &["int"],
        has_receiver: true,
        inputs: &[Port::Receiver],
        complete: true,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "substring",
        types: &["int", "int"],
        has_receiver: true,
        inputs: &[Port::Receiver],
        complete: true,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "replace",
        types: &["CharSequence", "CharSequence"],
        has_receiver: true,
        inputs: &[Port::Receiver, Port::Parameter(1)],
        complete: true,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "trim",
        types: &[],
        has_receiver: true,
        inputs: &[Port::Receiver],
        complete: false,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "toLowerCase",
        types: &[],
        has_receiver: true,
        inputs: &[Port::Receiver],
        complete: false,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "strip",
        types: &[],
        has_receiver: true,
        inputs: &[Port::Receiver],
        complete: false,
    },
    Classic {
        artifact_path: "java/lang/String.class",
        member: "repeat",
        types: &["int"],
        has_receiver: true,
        inputs: &[Port::Receiver],
        complete: false,
    },
    Classic {
        artifact_path: "java/lang/StringBuilder.class",
        member: "append",
        types: &["String"],
        has_receiver: true,
        inputs: &[Port::Receiver, Port::Parameter(0)],
        complete: false,
    },
    Classic {
        artifact_path: "java/lang/StringBuilder.class",
        member: "toString",
        types: &[],
        has_receiver: true,
        inputs: &[Port::Receiver],
        complete: false,
    },
    Classic {
        artifact_path: "java/util/StringJoiner.class",
        member: "add",
        types: &["CharSequence"],
        has_receiver: true,
        inputs: &[Port::Receiver, Port::Parameter(0)],
        complete: false,
    },
    Classic {
        artifact_path: "java/util/Base64$Encoder.class",
        member: "encodeToString",
        types: &["byte[]"],
        has_receiver: true,
        inputs: &[Port::Receiver, Port::Parameter(0)],
        complete: false,
    },
    Classic {
        artifact_path: "java/nio/file/Paths.class",
        member: "get",
        types: &["String", "String[]"],
        has_receiver: false,
        inputs: &[Port::Parameter(0), Port::Parameter(1)],
        complete: false,
    },
    Classic {
        artifact_path: "java/net/URLDecoder.class",
        member: "decode",
        types: &["String", "String"],
        has_receiver: false,
        inputs: &[Port::Parameter(0)],
        complete: false,
    },
];

fn classic_entry(classic: &Classic) -> FoundryEntry {
    let target = FoundryTarget {
        artifact_path: classic.artifact_path.to_owned(),
        member: classic.member.to_owned(),
        signature: FoundrySignature::Overload {
            types: classic
                .types
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        },
    };
    FoundryEntry {
        id: summary_id(FoundryCorpus::Derived, &target),
        corpus: FoundryCorpus::Derived,
        target,
        boundary: FoundryBoundary {
            has_receiver: classic.has_receiver,
            parameter_count: classic.types.len() as u32,
        },
        claim: FoundryClaim::Flows,
        completeness: if classic.complete {
            FoundryCompleteness::Complete
        } else {
            FoundryCompleteness::Partial
        },
        transfers: classic
            .inputs
            .iter()
            .map(|port| AuthoredSummaryTransfer {
                input: port.authored(),
                exit_kind: AuthoredSummaryExitKind::Normal,
                output: AuthoredSummaryOutput::NormalReturn {},
            })
            .collect(),
        artifact: FoundryArtifactBinding::Unresolved,
        evidence: Vec::new(),
        notes: Vec::new(),
        derivation: None,
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// The pinned JDK slices milestone 2 derives from.
fn pinned_jdk_slices() -> PathBuf {
    repository_root()
        .join("crates/bifrost-semantic-packs/testdata/summary-sources/temurin-jdk-21.0.8+9")
}

/// The pinned Models-as-Data slice milestone 1 translates.
fn codeql_slice() -> PathBuf {
    repository_root().join("crates/bifrost-semantic-packs/testdata/summary-corpora/codeql")
}

fn derived_entries(symbols: &[&str]) -> Vec<FoundryEntry> {
    let run = derive_jvm_summaries(&pinned_jdk_slices(), DerivationLimits::default())
        .expect("the pinned JDK slices derive");
    symbols
        .iter()
        .map(|symbol| {
            run.entries
                .iter()
                .find(|entry| entry.target.signature.symbol(&entry.target.member) == *symbol)
                .unwrap_or_else(|| panic!("no derived entry for {symbol}"))
                .clone()
        })
        .collect()
}

fn translated_codeql_entries() -> Vec<FoundryEntry> {
    let files = read_codeql_models(&codeql_slice()).expect("the pinned CodeQL slice reads");
    translate_codeql_models(&files)
        .expect("the pinned CodeQL slice translates")
        .entries
}

fn codeql_entry(entries: &[FoundryEntry], artifact_path: &str, symbol: &str) -> FoundryEntry {
    entries
        .iter()
        .find(|entry| {
            entry.target.artifact_path == artifact_path
                && entry.target.signature.symbol(&entry.target.member) == symbol
        })
        .unwrap_or_else(|| panic!("no translated entry for {artifact_path}#{symbol}"))
        .clone()
}

/// Generate and run every case of one entry, and report what happened.
fn run_entry(entry: &FoundryEntry, root: &Path) -> FixtureRunReport {
    let suite = generate_fixture_suite(entry, &JavaFixtureLanguage)
        .unwrap_or_else(|error| panic!("{} has no fixture: {error}", entry.id));
    let report = run_fixture_suite(&suite, root, &JavaFixtureLanguage)
        .unwrap_or_else(|error| panic!("{} could not run: {error}", entry.id));
    println!(
        "entry={} target={} cases={:?}",
        report.entry_id,
        report.target,
        report
            .cases
            .iter()
            .map(|case| format!(
                "{}:{}/{}",
                case.polarity.as_str(),
                case.observed_findings,
                case.expected_findings
            ))
            .collect::<Vec<_>>()
    );
    report
}

fn assert_suite_passes(entries: &[FoundryEntry], label: &str) {
    let directory = tempfile::tempdir().expect("fixture workspace root");
    let mut positives = 0usize;
    let mut fail_befores = 0usize;
    let mut negatives = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        let report = run_entry(entry, &directory.path().join(format!("{label}-{index}")));
        assert!(
            report.passed(),
            "{} failed: {:#?}",
            report.target,
            report.failures()
        );
        for case in &report.cases {
            match case.polarity {
                FixturePolarity::Positive => {
                    positives += 1;
                    assert!(
                        case.observed_findings > 0,
                        "a positive case must observe the claimed flow: {case:#?}"
                    );
                }
                FixturePolarity::FailBefore => fail_befores += 1,
                FixturePolarity::Negative => negatives += 1,
            }
        }
    }
    assert_eq!(positives, entries.len());
    assert_eq!(fail_befores, entries.len());
    println!("{label}: {positives} positive, {fail_befores} fail-before, {negatives} negative");
}

/// Milestone 2's derived `java.util.Objects` entries, and the one signed row
/// the pinned CodeQL slice carries.
///
/// `requireNonNullElseGet(T,Supplier<? extends T>)` is the interesting one: its
/// pinned parameter spelling carries a wildcard type argument, so it also
/// proves the generator's declaration and the analyzer's symbol agree on a
/// non-trivial spelling.
#[test]
fn generated_fixtures_pass_for_derived_and_translated_entries() {
    let mut entries = derived_entries(&[
        "requireNonNull(T)",
        "requireNonNullElse(T,T)",
        "requireNonNullElseGet(T,Supplier<? extends T>)",
        "toString(Object,String)",
    ]);
    entries.push(codeql_entry(
        &translated_codeql_entries(),
        "java/lang/String.class",
        "concat(String)",
    ));

    assert_eq!(entries.len(), 5);
    assert_suite_passes(&entries, "corpus");
}

#[test]
fn generated_fixtures_pass_for_hand_authored_string_propagations() {
    let entries = CLASSICS[..8].iter().map(classic_entry).collect::<Vec<_>>();

    assert_eq!(entries.len(), 8);
    assert_suite_passes(&entries, "classics-a");
}

#[test]
fn generated_fixtures_pass_for_hand_authored_builder_and_path_propagations() {
    let entries = CLASSICS[8..].iter().map(classic_entry).collect::<Vec<_>>();

    assert_eq!(entries.len(), 7);
    assert_suite_passes(&entries, "classics-b");
}

/// The complete entries carry the negative polarity, and it is not vacuous:
/// the same fixture family observes the claimed flow through the claimed port
/// and observes nothing through the port the entry denies.
#[test]
fn a_complete_entry_proves_taint_does_not_cross_where_it_says_it_does_not() {
    let directory = tempfile::tempdir().expect("fixture workspace root");
    let entry = classic_entry(
        CLASSICS
            .iter()
            .find(|classic| classic.member == "replace")
            .expect("the replace classic is complete"),
    );

    let report = run_entry(&entry, directory.path());

    assert!(report.passed(), "{:#?}", report.failures());
    let negative = report
        .cases
        .iter()
        .find(|case| case.polarity == FixturePolarity::Negative)
        .expect("a complete entry gets a negative case");
    assert_eq!(negative.observed_findings, 0);
    let positive = report
        .cases
        .iter()
        .find(|case| case.polarity == FixturePolarity::Positive)
        .expect("every suite has a positive case");
    assert_eq!(
        positive.observed_findings, 2,
        "the negative is only meaningful because the claimed ports do carry taint: {positive:#?}"
    );
}

/// A corrupted entry claims a transfer from a port its own target does not
/// have. The production pack compiler is what refuses it, so the corruption is
/// caught by shipping machinery rather than by a check written for this test.
#[test]
fn a_corrupted_entry_fails_the_fixture_gate() {
    let directory = tempfile::tempdir().expect("fixture workspace root");
    let mut corrupted = classic_entry(
        CLASSICS
            .iter()
            .find(|classic| classic.member == "valueOf")
            .expect("the valueOf classic is one parameter wide"),
    );
    corrupted.transfers.push(AuthoredSummaryTransfer {
        input: AuthoredSummaryInput::Parameter { ordinal: 1 },
        exit_kind: AuthoredSummaryExitKind::Normal,
        output: AuthoredSummaryOutput::NormalReturn {},
    });

    let suite = generate_fixture_suite(&corrupted, &JavaFixtureLanguage)
        .expect("the corrupted entry still renders a fixture");
    let report = run_fixture_suite(&suite, directory.path(), &JavaFixtureLanguage)
        .expect("the corrupted entry still runs");

    assert!(
        !report.passed(),
        "a transfer the target does not have must fail the gate: {report:#?}"
    );
    let positive = report
        .cases
        .iter()
        .find(|case| case.polarity == FixturePolarity::Positive)
        .expect("every suite has a positive case");
    assert!(
        positive
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("summary.invalid_ordinal")),
        "the production pack compiler must name the invalid ordinal: {positive:#?}"
    );
}

/// Three shapes the fixture engine cannot carry today, each refused with its
/// own reason rather than passed silently. All three come from the pinned
/// corpora, so they are the real distribution and not invented cases.
#[test]
fn entries_the_fixture_engine_cannot_carry_are_refused_with_a_typed_reason() {
    let entries = translated_codeql_entries();

    // A Models-as-Data row without a signature pins no overload, so its symbol
    // names no exact procedure and the production binder has nothing to bind.
    let unsigned = codeql_entry(&entries, "java/util/Objects.class", "requireNonNull");
    assert_eq!(
        generate_fixture_suite(&unsigned, &JavaFixtureLanguage),
        Err(FixtureUnsupported::NoPinnedOverload)
    );

    // A `neutralModel` row states no flow at all, which the authored IR cannot
    // spell, so there is nothing to execute (milestone 1's
    // `no_flow_claim_not_carried`).
    let no_flow = codeql_entry(&entries, "java/lang/String.class", "length()");
    assert_eq!(no_flow.claim, FoundryClaim::NoFlow);
    assert_eq!(
        generate_fixture_suite(&no_flow, &JavaFixtureLanguage),
        Err(FixtureUnsupported::NoAuthoredSummary)
    );

    // A write into the receiver needs a fixture that reads the mutated cell
    // back, which this stage does not build.
    let receiver_output = codeql_entry(
        &entries,
        "java/lang/AbstractStringBuilder.class",
        "<init>(String)",
    );
    assert_eq!(
        generate_fixture_suite(&receiver_output, &JavaFixtureLanguage),
        Err(FixtureUnsupported::OutputPortUnsupported {
            output: "receiver".to_owned()
        })
    );
}
