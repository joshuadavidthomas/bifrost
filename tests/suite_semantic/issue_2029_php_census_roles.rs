//! Issue #2029: PHP census probe eligibility and inverse membership are
//! separate contracts. Declarations are not probes, while structured
//! references inside parser recovery still back inverse hits.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ProbeSeed, ReferenceDifferentialConfig, run_reference_differential,
};
use brokk_bifrost::{AnalyzerConfig, Language};

#[test]
fn php_census_excludes_declarations_and_backs_recovered_references() {
    let source = r#"<?php
namespace App\Demo;

final class Id
{
    public static function make(): self
    {
        return new self();
    }
}

final class Result
{
    public const FLAG = 1;
    public ?Result $adjacent = null;

    public function withId(Id $id): self
    {
        return $this;
    }

    public function withInput(string $input): self
    {
        return $this;
    }

    public function trigger(string $input): self
    {
        $this->withInput($input);
        return clone($this, [
            'id' => Id::make(),
            'adjacent' => $this->adjacent?->withInput($input),
        ]);
    }

    public function flag(): int
    {
        return self::FLAG;
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Php)
        .file("src/Result.php", source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let report = run_reference_differential(
        workspace.analyzer(),
        &ReferenceDifferentialConfig {
            corpus_language: "php".to_string(),
            max_files: 10,
            max_sites: 1_000,
            max_candidates_per_file: 1_000,
            max_source_bytes: 20_000,
            max_targets: 1_000,
            max_usage_files: 10,
            max_usages: 1_000,
            probe_seed: ProbeSeed::Census,
            ..ReferenceDifferentialConfig::default()
        },
    )
    .expect("run PHP census differential");

    let namespace_segments = [
        source.find("App\\Demo").expect("namespace App"),
        source.find("Demo;").expect("namespace Demo"),
    ];
    let constant_declaration = source.find("FLAG =").expect("constant declaration");
    for declaration in namespace_segments
        .into_iter()
        .chain(std::iter::once(constant_declaration))
    {
        assert!(
            report
                .sites
                .iter()
                .all(|site| site.start_byte != declaration),
            "PHP declaration at {declaration} became a probe: {report:#?}"
        );
    }
    assert!(
        report.summary.declaration_sites_excluded >= 3,
        "structured PHP declarations were not accounted: {report:#?}"
    );

    let id_type_reference = source.find("Id $id").expect("Id type reference");
    let clean_method_call = source
        .find("withInput($input);")
        .expect("clean method call");
    for reference in [id_type_reference, clean_method_call] {
        assert!(
            report.sites.iter().any(|site| site.start_byte == reference),
            "ordinary PHP reference at {reference} was not probed: {report:#?}"
        );
    }

    assert!(
        report.inverse_precision_findings.is_empty(),
        "static-scope and nullsafe references must be membership-backed: {report:#?}"
    );
}
