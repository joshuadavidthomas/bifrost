//! Issue #1803: a C# preprocessor directive inside a declaration must not cost
//! the file its members.
//!
//! Before the fix, tree-sitter's recovery wrapped the members around a
//! mid-declaration `#if` in `preproc_if` / `preproc_else` nodes that the
//! declaration walk does not descend into, so the members disappeared from the
//! index. The analyzer now parses C# through included ranges that hide
//! directive lines and inactive conditional branches, so each member is a
//! direct child of its declaration list again.
//!
//! The two shapes below are the ones the FIRD differential campaign found:
//! markdig's `StringSlice.cs` (a member signature split across `#if NET`) and
//! commandlineparser's `TypeConverter.cs` (`#if` inside a ternary).

use crate::common::csharp_analyzer_with_files;
use brokk_bifrost::{CSharpAnalyzer, CodeUnit, CodeUnitIndex, ProjectFile};

/// The markdig `StringSlice` shape: fields, then a `#if NET` / `#else` split
/// signature, then three more methods that used to vanish with it.
const STRING_SLICE: &str = r#"namespace Markdig.Helpers
{
    public struct StringSlice
    {
        public string Text;
        public int Start;

#if NET
        [MethodImpl(MethodImplOptions.AggressiveInlining)]
        public readonly void TrimStart()
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

        public char PeekChar()
        {
            return Text[Start];
        }

        public bool Match(string text)
        {
            return Text == text;
        }
    }
}
"#;

/// The commandlineparser `TypeConverter` shape: `#if` inside a ternary.
const TYPE_CONVERTER: &str = r#"namespace CommandLine.Core
{
    static class TypeConverter
    {
        public static object ChangeType(string value, System.Type type)
        {
            return type == typeof(int)
                ? ParseInt(value)
#if !SKIP_FSHARP
                : IsFsharpOption(type)
                    ? ParseOption(value)
                    : ParseOther(value);
#else
                : ParseOther(value);
#endif
        }

        public static object ParseInt(string value) { return 0; }

        public static bool IsFsharpOption(System.Type type) { return false; }
    }
}
"#;

fn short_names(analyzer: &CSharpAnalyzer, file: &ProjectFile) -> Vec<String> {
    let mut names: Vec<String> = analyzer
        .declarations(file)
        .iter()
        .map(CodeUnit::short_name)
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
}

#[test]
fn members_after_a_split_signature_are_indexed() {
    let (project, analyzer) = csharp_analyzer_with_files(&[("StringSlice.cs", STRING_SLICE)]);
    let file = project.file("StringSlice.cs");
    let names = short_names(&analyzer, &file);

    for member in [
        "StringSlice",
        "StringSlice.Text",
        "StringSlice.Start",
        "StringSlice.TrimStart",
        "StringSlice.NextChar",
        "StringSlice.PeekChar",
        "StringSlice.Match",
    ] {
        assert!(
            names.contains(&member.to_string()),
            "{member} must be indexed; the file declares {names:?}"
        );
    }
}

#[test]
fn a_split_member_is_declared_once_at_its_real_location() {
    let (project, analyzer) = csharp_analyzer_with_files(&[("StringSlice.cs", STRING_SLICE)]);
    let file = project.file("StringSlice.cs");

    let trim_start: Vec<CodeUnit> = analyzer
        .declarations(&file)
        .into_iter()
        .filter(|unit| unit.short_name() == "StringSlice.TrimStart")
        .collect();
    assert_eq!(
        trim_start.len(),
        1,
        "the raw parse saw TrimStart twice, once per branch: {trim_start:?}"
    );

    // The range must point at the raw file, not at some transformed copy.
    let source = analyzer.get_source(&trim_start[0], true).expect("source");
    assert!(
        source.contains("void TrimStart()"),
        "the recovered range must cover the real declaration: {source}"
    );
    assert!(
        !source.contains("#if"),
        "the recovered range must not swallow the directive: {source}"
    );
}

#[test]
fn a_directive_inside_a_ternary_still_yields_the_type_and_its_methods() {
    let (project, analyzer) = csharp_analyzer_with_files(&[("TypeConverter.cs", TYPE_CONVERTER)]);
    let file = project.file("TypeConverter.cs");
    let names = short_names(&analyzer, &file);

    for member in [
        "TypeConverter",
        "TypeConverter.ChangeType",
        "TypeConverter.ParseInt",
        "TypeConverter.IsFsharpOption",
    ] {
        assert!(
            names.contains(&member.to_string()),
            "{member} must be indexed; the file declares {names:?}"
        );
    }
}

#[test]
fn a_member_that_exists_only_in_an_inactive_branch_is_absent() {
    // Honesty check: the first branch wins, so the `#else` member is not in the
    // index. The Milestone 4 diagnostic is what tells a consumer this happened.
    const EITHER_OR: &str = r#"public class Config
{
#if NET
    public void Modern() { }
#else
    public void Legacy() { }
#endif
    public void Always() { }
}
"#;
    let (project, analyzer) = csharp_analyzer_with_files(&[("Config.cs", EITHER_OR)]);
    let names = short_names(&analyzer, &project.file("Config.cs"));

    assert!(names.contains(&"Config.Modern".to_string()), "{names:?}");
    assert!(names.contains(&"Config.Always".to_string()), "{names:?}");
    assert!(!names.contains(&"Config.Legacy".to_string()), "{names:?}");
}

#[test]
fn an_if_false_block_hides_its_members() {
    const DISABLED: &str = r#"public class Config
{
#if false
    public void Disabled() { }
#endif
    public void Live() { }
}
"#;
    let (project, analyzer) = csharp_analyzer_with_files(&[("Config.cs", DISABLED)]);
    let names = short_names(&analyzer, &project.file("Config.cs"));

    assert!(names.contains(&"Config.Live".to_string()), "{names:?}");
    assert!(!names.contains(&"Config.Disabled".to_string()), "{names:?}");
}

#[test]
fn a_region_directive_does_not_change_what_is_indexed() {
    const REGIONS: &str = r#"public class Config
{
#region Members
    public void First() { }
    public void Second() { }
#endregion
}
"#;
    let (project, analyzer) = csharp_analyzer_with_files(&[("Config.cs", REGIONS)]);
    let names = short_names(&analyzer, &project.file("Config.cs"));

    assert!(names.contains(&"Config.First".to_string()), "{names:?}");
    assert!(names.contains(&"Config.Second".to_string()), "{names:?}");
}
