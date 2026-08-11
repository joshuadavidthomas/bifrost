//! Issue #1803: navigation must reach a member declared after a preprocessor
//! directive that splits a member signature.
//!
//! The declaration side of this is covered in
//! `suite_analyzers::csharp_preprocessor_split_declarations_test`. What this
//! file proves is the other half: the usage graph parses the caller and the
//! callee through the same directive-aware included ranges, so a call to
//! `TrimStart` on the split type resolves to that method and not to a
//! same-named member of an unrelated type.

use crate::common::InlineTestProject;
use brokk_bifrost::CodeUnitIndex;
use brokk_bifrost::usages::{CSharpUsageGraphStrategy, UsageAnalyzer, UsageHit};
use brokk_bifrost::{CSharpAnalyzer, CodeUnit, CodeUnitType, Language};
use std::collections::BTreeSet;

/// The declaring file: a `#if NET` / `#else` split signature, then the members
/// that used to disappear with it.
const SLICE: &str = r#"namespace Demo;

public struct StringSlice
{
    public string Text;
    public int Start;

#if NET
    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public void TrimStart()
#else
    public void TrimStart()
#endif
    {
        Start++;
    }

    public char NextChar()
    {
        return Text[Start];
    }
}

/// A same-named member on an unrelated type, so a hit that lands here is a
/// conflation rather than a resolution.
public interface IDecoy
{
    void TrimStart();
}
"#;

/// The calling file.
const CALLER: &str = r#"namespace Demo;

public class Reader
{
    public void Consume(StringSlice slice)
    {
        slice.TrimStart();
        var c = slice.NextChar();
    }
}
"#;

fn method(analyzer: &CSharpAnalyzer, owner: &str, name: &str) -> CodeUnit {
    let declarations = analyzer.get_all_declarations();
    declarations
        .iter()
        .find(|unit| {
            unit.kind() == CodeUnitType::Function
                && unit.identifier() == name
                && analyzer
                    .parent_of(unit)
                    .is_some_and(|parent| parent.fq_name() == owner)
        })
        .cloned()
        .unwrap_or_else(|| panic!("missing {owner}.{name} in {declarations:#?}"))
}

fn hits(analyzer: &CSharpAnalyzer, target: &CodeUnit) -> BTreeSet<UsageHit> {
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    CSharpUsageGraphStrategy::new()
        .find_usages(analyzer, std::slice::from_ref(target), &candidates, 1000)
        .into_either()
        .unwrap_or_else(|err| panic!("{} should resolve: {err}", target.fq_name()))
}

fn analyzer() -> (crate::common::BuiltInlineTestProject, CSharpAnalyzer) {
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("StringSlice.cs", SLICE)
        .file("Reader.cs", CALLER)
        .build();
    let analyzer = CSharpAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

#[test]
fn a_call_reaches_the_method_declared_after_a_split_signature() {
    let (project, analyzer) = analyzer();
    let caller = project.file("Reader.cs");

    let target = method(&analyzer, "Demo.StringSlice", "TrimStart");
    let hits = hits(&analyzer, &target);

    assert!(
        hits.iter().any(|hit| hit.file == caller),
        "the call in Reader.cs must reach the split member: {hits:#?}"
    );
}

#[test]
fn the_decoy_member_of_an_unrelated_type_collects_no_hit() {
    let (_project, analyzer) = analyzer();

    let decoy = method(&analyzer, "Demo.IDecoy", "TrimStart");
    let hits = hits(&analyzer, &decoy);

    assert!(
        hits.is_empty(),
        "nothing calls IDecoy.TrimStart, so a hit here is a conflation: {hits:#?}"
    );
}

#[test]
fn a_member_declared_after_the_split_is_navigable_too() {
    let (project, analyzer) = analyzer();
    let caller = project.file("Reader.cs");

    let target = method(&analyzer, "Demo.StringSlice", "NextChar");
    let hits = hits(&analyzer, &target);

    assert!(
        hits.iter().any(|hit| hit.file == caller),
        "NextChar is declared after the split and must still be reachable: {hits:#?}"
    );
}
