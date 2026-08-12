//! Issue #1808: a nested C# type uses an enclosing type's member across types.

use crate::common::InlineTestProject;
use brokk_bifrost::usages::{CSharpUsageGraphStrategy, UsageAnalyzer, UsageHitKind};
use brokk_bifrost::{CSharpAnalyzer, CodeUnitIndex, CodeUnitType, Language};

#[test]
fn csharp_enclosing_type_bare_call_is_a_cross_type_usage() {
    let project = InlineTestProject::with_language(Language::CSharp)
        .file(
            "Demo.cs",
            r#"namespace Demo;

public class Outer
{
    public static object Helper(object left, object right) => left;

    public object SameOwner(object left, object right) => Helper(left, right);

    public class Mid
    {
        public class Inner
        {
            public object CrossType(object left, object right) => Helper(left, right);
        }
    }
}
"#,
        )
        .build();
    let analyzer = CSharpAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_all_declarations()
        .iter()
        .find(|unit| unit.kind() == CodeUnitType::Function && unit.fq_name() == "Demo.Outer.Helper")
        .cloned()
        .expect("outer helper declaration");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let result = CSharpUsageGraphStrategy::new().find_usages(
        &analyzer,
        std::slice::from_ref(&target),
        &candidates,
        1000,
    );
    let all_hits = result.all_hits_including_imports();
    let external = result.into_either().expect("C# inverse query succeeds");

    assert!(
        external.iter().any(|hit| {
            hit.enclosing.fq_name() == "Demo.Outer$Mid$Inner.CrossType"
                && hit.snippet.contains("Helper(left, right)")
        }),
        "the enclosing-type call must be a proven cross-type hit: {external:#?}"
    );
    assert!(
        all_hits.iter().any(|hit| {
            hit.enclosing.fq_name() == "Demo.Outer.SameOwner"
                && hit.kind == UsageHitKind::SelfReceiver
        }),
        "the same-owner call must remain a self-receiver site: {all_hits:#?}"
    );
}
