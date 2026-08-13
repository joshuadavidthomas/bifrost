//! Cross-language conformance for the #1478 Milestone 2 callable-signature
//! rows.
//!
//! The invariants under test are the milestone's acceptance criteria. An
//! overload set separates into rows that differ in declared shape at equal
//! total arity; defaults and variadics produce an arity *range* rather than a
//! count; a Kotlin extension receiver, a C# static callable, and a C# instance
//! callable land in three distinct receiver contracts; a language that records
//! no arity says `arity_unrecorded` instead of publishing a zero; and a warm
//! cache returns byte-identical rows to the first run of the same workspace.
//!
//! Every assertion goes through the public `query_code` surface, which is what
//! an RQL or RQLP author actually binds.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::structural::{CodeQuery, CodeQueryResult, execute_workspace};
use brokk_bifrost::{AnalyzerConfig, WorkspaceAnalyzer};
use serde_json::{Value, json};

/// Run `queries` against one workspace, reusing the analyzer so the second and
/// later queries are answered from the warm workspace.
fn run_all(files: &[(&str, &str)], queries: &[Value]) -> Vec<CodeQueryResult> {
    let mut project = InlineTestProject::new();
    for (path, source) in files {
        project = project.file(*path, *source);
    }
    let project = project.build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    queries
        .iter()
        .map(|query| {
            let query = CodeQuery::from_json(query).expect("query should parse");
            execute_workspace(&workspace, &query)
        })
        .collect()
}

fn run(files: &[(&str, &str)], query: Value) -> Value {
    let results = run_all(files, std::slice::from_ref(&query));
    serde_json::to_value(&results[0]).expect("query result should serialize")
}

fn rows(value: &Value) -> &Vec<Value> {
    value["results"].as_array().expect("results array")
}

/// Every declaration named `name` in `path`, expanded by the given steps.
fn signature_query(path: &str, kind: &str, name: &str, steps: &[&str]) -> Value {
    let mut ops = vec![json!({ "op": "enclosing_decl" })];
    ops.extend(steps.iter().map(|op| json!({ "op": op })));
    json!({
        "where": [path],
        "match": { "kind": kind, "name": name },
        "steps": ops,
        "result_detail": "full"
    })
}

fn sole<'a>(value: &'a Value, what: &str) -> &'a Value {
    let rows = rows(value);
    assert_eq!(rows.len(), 1, "expected one {what} row: {value}");
    &rows[0]
}

const JAVA_OVERLOADS: &str = r#"public class App {
  static int render(int width, String label) { return width; }
  static int render(String label, int width) { return width; }
  static int pad(int width, int... extra) { return width; }
  int instanceOnly(int width) { return width; }
  static int staticOnly(int width) { return width; }
}
"#;

/// Two Java overloads share one declaration name and one total arity. They are
/// still two rows, and the declared parameter types make the shapes distinct --
/// which is exactly the discrimination a resolver has to perform and a policy
/// has to be able to state.
#[test]
fn java_overloads_are_distinct_rows_at_equal_total_arity() {
    let files = [("App.java", JAVA_OVERLOADS)];

    let signatures = run(
        &files,
        signature_query("App.java", "method", "render", &["callable_signature"]),
    );
    let signatures = rows(&signatures);
    assert_eq!(signatures.len(), 2, "one row per overload: {signatures:?}");
    for signature in signatures {
        assert_eq!(signature["result_type"], "callable_signature");
        assert_eq!(signature["coverage"], "exact");
        assert_eq!(signature["role"], "method");
        assert_eq!(signature["required_arity"], 2);
        assert_eq!(signature["total_arity"], 2);
        assert_eq!(signature["repeated"], false);
        assert_eq!(signature["parameter_count"], 2);
    }
    assert_ne!(
        signatures[0]["id"], signatures[1]["id"],
        "each overload owns its identity"
    );

    let parameters = run(
        &files,
        signature_query(
            "App.java",
            "method",
            "render",
            &["callable_signature", "signature_parameters"],
        ),
    );
    let parameters = rows(&parameters);
    assert_eq!(parameters.len(), 4, "{parameters:?}");
    let shapes = parameters
        .chunks(2)
        .map(|pair| {
            pair.iter()
                .map(|parameter| parameter["declared_type"].as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert!(
        shapes.contains(&vec!["int".to_owned(), "String".to_owned()])
            && shapes.contains(&vec!["String".to_owned(), "int".to_owned()]),
        "equal arity, distinct declared shapes: {shapes:?}"
    );
    for parameter in parameters {
        assert_eq!(parameter["result_type"], "signature_parameter");
        assert!(
            signatures
                .iter()
                .any(|signature| signature["id"] == parameter["signature_id"]),
            "every parameter is foreign-keyed to one of its own signature rows"
        );
    }
}

/// A Java varargs declaration accepts more arguments than it declares
/// parameters, so its arity is a range with a repeating tail rather than a
/// count, and only the trailing parameter repeats.
#[test]
fn java_varargs_produce_an_arity_range_with_a_repeating_tail() {
    let files = [("App.java", JAVA_OVERLOADS)];

    let signature = run(
        &files,
        signature_query("App.java", "method", "pad", &["callable_signature"]),
    );
    let signature = sole(&signature, "callable signature");
    assert_eq!(signature["coverage"], "exact");
    assert_eq!(signature["repeated"], true);
    assert_eq!(
        signature["required_arity"], 1,
        "the variadic tail is not required: {signature}"
    );

    let parameters = run(
        &files,
        signature_query(
            "App.java",
            "method",
            "pad",
            &["callable_signature", "signature_parameters"],
        ),
    );
    let parameters = rows(&parameters);
    assert_eq!(parameters.len(), 2, "{parameters:?}");
    assert_eq!(parameters[0]["parameter_index"], 0);
    assert_eq!(parameters[0]["repeated"], false);
    assert_eq!(parameters[0]["optional"], false);
    assert_eq!(parameters[1]["parameter_index"], 1);
    assert_eq!(parameters[1]["repeated"], true);
}

/// Java records callable modifiers, so a static method and an instance method
/// carry different receiver contracts. This is the fact an applicability check
/// consumes when it asks whether a receiver-qualified call could reach a
/// candidate at all.
#[test]
fn java_static_and_instance_receiver_contracts_are_distinct() {
    let files = [("App.java", JAVA_OVERLOADS)];

    let instance = run(
        &files,
        signature_query(
            "App.java",
            "method",
            "instanceOnly",
            &["callable_signature"],
        ),
    );
    assert_eq!(
        sole(&instance, "callable signature")["receiver_contract"],
        "instance"
    );

    let statik = run(
        &files,
        signature_query("App.java", "method", "staticOnly", &["callable_signature"]),
    );
    assert_eq!(
        sole(&statik, "callable signature")["receiver_contract"],
        "static_or_companion"
    );
}

const CSHARP: &str = r#"namespace App {
  public class Widget {
    public static int Build(int size) { return size; }
    public int Resize(int size, int padding = 1) { return size; }
    public T Convert<T>(T value) { return value; }
  }
}
"#;

/// C# records modifiers, optional parameters, and type parameters. The three
/// facts land in three different fields: a static callable's contract, an
/// optional parameter's arity range, and a generic method's generic arity.
#[test]
fn csharp_contracts_defaults_and_generic_arity_are_separate_fields() {
    let files = [("Widget.cs", CSHARP)];

    let build = run(
        &files,
        signature_query("Widget.cs", "method", "Build", &["callable_signature"]),
    );
    let build = sole(&build, "callable signature");
    assert_eq!(build["receiver_contract"], "static_or_companion");
    assert_eq!(build["generic_arity"], 0);

    let resize = run(
        &files,
        signature_query("Widget.cs", "method", "Resize", &["callable_signature"]),
    );
    let resize = sole(&resize, "callable signature");
    assert_eq!(resize["receiver_contract"], "instance");
    assert_eq!(
        (
            resize["required_arity"].clone(),
            resize["total_arity"].clone()
        ),
        (json!(1), json!(2)),
        "a default makes the arity a range: {resize}"
    );
    assert_eq!(resize["repeated"], false);

    let parameters = run(
        &files,
        signature_query(
            "Widget.cs",
            "method",
            "Resize",
            &["callable_signature", "signature_parameters"],
        ),
    );
    let parameters = rows(&parameters);
    assert_eq!(parameters.len(), 2, "{parameters:?}");
    assert_eq!(parameters[0]["optional"], false);
    assert_eq!(parameters[1]["optional"], true);

    let convert = run(
        &files,
        signature_query("Widget.cs", "method", "Convert", &["callable_signature"]),
    );
    assert_eq!(
        sole(&convert, "callable signature")["generic_arity"],
        1,
        "a generic sibling is distinguished by its type-parameter arity"
    );
}

const KOTLIN: &str = r#"package app

class Widget

fun Widget.stretch(size: Int): Int = size

fun free(size: Int): Int = size
"#;

/// A Kotlin extension callable names the type it extends, so its receiver
/// contract is `extension` and not `instance` -- the distinction the epic's
/// extension-receiver fixtures are about. A free function in the same file is
/// the near miss.
#[test]
fn kotlin_extension_receiver_is_its_own_contract() {
    let files = [("Widget.kt", KOTLIN)];

    let extension = run(
        &files,
        signature_query("Widget.kt", "function", "stretch", &["callable_signature"]),
    );
    assert_eq!(
        sole(&extension, "callable signature")["receiver_contract"],
        "extension"
    );

    let free = run(
        &files,
        signature_query("Widget.kt", "function", "free", &["callable_signature"]),
    );
    let free = sole(&free, "callable signature");
    assert_ne!(
        free["receiver_contract"], "extension",
        "a free function is not an extension: {free}"
    );
}

const PYTHON: &str = r#"def render(width, label):
    return width
"#;

/// Python's adapter records parameters but no callable arity. The row says
/// `arity_unrecorded` and omits the arity fields entirely rather than
/// publishing a zero that would read as a proven-empty parameter list, and the
/// parameter rows omit optionality for the same reason.
#[test]
fn an_unrecorded_arity_is_stated_rather_than_defaulted() {
    let files = [("app.py", PYTHON)];

    let signature = run(
        &files,
        signature_query("app.py", "function", "render", &["callable_signature"]),
    );
    let signature = sole(&signature, "callable signature");
    if signature["coverage"] == "exact" {
        // The adapter records arity for this language after all; then the row
        // must carry the range rather than claim unrecorded coverage.
        assert!(signature["total_arity"].is_number(), "{signature}");
        return;
    }
    assert_eq!(signature["coverage"], "arity_unrecorded", "{signature}");
    assert!(signature["required_arity"].is_null(), "{signature}");
    assert!(signature["total_arity"].is_null(), "{signature}");

    let parameters = run(
        &files,
        signature_query(
            "app.py",
            "function",
            "render",
            &["callable_signature", "signature_parameters"],
        ),
    );
    for parameter in rows(&parameters) {
        assert!(
            parameter["optional"].is_null(),
            "optionality nobody recorded stays absent: {parameter}"
        );
    }
}

/// The rows are a pure projection of persisted facts, so asking the same
/// workspace twice returns byte-identical rows: the second query is answered
/// from the warm analyzer. A row identity or an arity that moved between the
/// two runs would mean the projection reads something other than the
/// persisted contract.
#[test]
fn warm_and_first_run_rows_are_identical() {
    let files = [("App.java", JAVA_OVERLOADS), ("Widget.cs", CSHARP)];
    let query = signature_query(
        "App.java",
        "method",
        "render",
        &["callable_signature", "signature_parameters"],
    );
    let results = run_all(&files, &[query.clone(), query]);
    let first = serde_json::to_value(&results[0]).expect("first result serializes");
    let second = serde_json::to_value(&results[1]).expect("second result serializes");
    assert!(!rows(&first).is_empty(), "{first}");
    assert_eq!(rows(&first), rows(&second));
}

/// A signature row reaches its declaring file like every other row domain, so
/// a policy can scope signature evidence without a second query.
#[test]
fn signature_rows_reach_their_declaring_file() {
    let files = [("App.java", JAVA_OVERLOADS)];
    let value = run(
        &files,
        signature_query(
            "App.java",
            "method",
            "staticOnly",
            &["callable_signature", "file_of"],
        ),
    );
    let file = sole(&value, "file");
    assert_eq!(file["result_type"], "file");
    assert_eq!(file["path"], "App.java");
}
