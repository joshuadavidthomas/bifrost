//! Issue #2049: an applicable concrete superclass method takes precedence
//! over an applicable interface declaration at the same hierarchy depth.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn definition_after(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    anchor: &str,
    needle: &str,
) -> Value {
    let anchor_start = source.find(anchor).expect("anchor");
    let start = anchor_start
        + source[anchor_start..]
            .find(needle)
            .expect("needle after anchor");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

fn definition_fqns(result: &Value) -> BTreeSet<&str> {
    result["definitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|definition| definition["fqn"].as_str())
        .collect()
}

#[test]
fn concrete_superclass_method_beats_same_level_interface_declaration() {
    let source = r#"package p;

interface Cluster {}
class Client implements Cluster {}

interface ClusterCapable {
    Cluster getMongoCluster();
}

class Support<C extends Cluster> {
    public C getMongoCluster() { return null; }
}

class Factory extends Support<Client> implements ClusterCapable {
    Cluster bare() { return getMongoCluster(); }
    Cluster explicit() { return this.getMongoCluster(); }
}

interface Left { int choose(); }
interface Right { int choose(); }
abstract class InterfacePeers implements Left, Right {
    int call() { return choose(); }
}

class WrongArityBase {
    int select(int value) { return value; }
}
interface ZeroArity { int select(); }
abstract class ApplicableInterface extends WrongArityBase implements ZeroArity {
    int call() { return select(); }
}

class OverrideFactory extends Support<Client> implements ClusterCapable {
    public Client getMongoCluster() { return null; }
    Cluster call() { return getMongoCluster(); }
}
"#;
    let path = "src/p/Hierarchy.java";
    let project = InlineTestProject::with_language(Language::Java)
        .file(path, source)
        .build();

    for anchor in [
        "Cluster bare() { return getMongoCluster",
        "Cluster explicit() { return this.getMongoCluster",
    ] {
        let result = definition_after(&project, path, source, anchor, "getMongoCluster");
        assert_eq!(result["status"], "resolved", "{result:#}");
        assert_eq!(
            definition_fqns(&result),
            BTreeSet::from(["p.Support.getMongoCluster"]),
            "{result:#}"
        );
    }

    let peers = definition_after(
        &project,
        path,
        source,
        "abstract class InterfacePeers",
        "choose",
    );
    assert_eq!(peers["status"], "ambiguous", "{peers:#}");
    assert_eq!(
        definition_fqns(&peers),
        BTreeSet::from(["p.Left.choose", "p.Right.choose"]),
        "{peers:#}"
    );

    let applicable = definition_after(
        &project,
        path,
        source,
        "abstract class ApplicableInterface",
        "select",
    );
    assert_eq!(applicable["status"], "resolved", "{applicable:#}");
    assert_eq!(
        definition_fqns(&applicable),
        BTreeSet::from(["p.ZeroArity.select"]),
        "{applicable:#}"
    );

    let direct = definition_after(
        &project,
        path,
        source,
        "Cluster call() { return getMongoCluster",
        "getMongoCluster",
    );
    assert_eq!(direct["status"], "resolved", "{direct:#}");
    assert_eq!(
        definition_fqns(&direct),
        BTreeSet::from(["p.OverrideFactory.getMongoCluster"]),
        "{direct:#}"
    );
}
