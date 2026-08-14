//! Issue #2066: a program lexical binding must beat a same-spelled owned
//! member when resolving a bare JavaScript call or construction.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn definition_at(
    project: &BuiltInlineTestProject,
    source: &str,
    path: &str,
    anchor: &str,
    token: &str,
) -> Value {
    let anchor_start = source.find(anchor).expect("reference anchor");
    let start = anchor_start + anchor.find(token).expect("focused token");
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

fn fqns(result: &Value) -> BTreeSet<&str> {
    result["definitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|definition| definition["fqn"].as_str())
        .collect()
}

#[test]
fn program_bindings_beat_owned_members_for_bare_calls_and_constructions() {
    let source = r#"const URL = require('url').URL;
const fresh = require('fresh');
const accepts = require('accepts');
const vary = require('vary');

const request = module.exports = {
  get URL() { return new URL('https://example.com'); },
  get fresh() { return fresh('etag', 'modified'); },
  accepts() { return accepts('json'); },
  vary() { return vary('accept'); },
};

function shadowed(fresh) { return fresh(); }
"#;
    let path = "lib/request.js";
    let project = InlineTestProject::with_language(Language::JavaScript)
        .file(path, source)
        .build();

    for (anchor, token, expected) in [
        ("URL('https", "URL", "request.js.URL"),
        ("fresh('etag'", "fresh", "request.js.fresh"),
        ("accepts('json'", "accepts", "request.js.accepts"),
        ("vary('accept'", "vary", "request.js.vary"),
    ] {
        let result = definition_at(&project, source, path, anchor, token);
        assert_eq!(result["status"], "resolved", "{result:#}");
        assert_eq!(fqns(&result), BTreeSet::from([expected]), "{result:#}");
    }

    let shadowed = definition_at(&project, source, path, "return fresh();", "fresh");
    assert_eq!(shadowed["status"], "resolved", "{shadowed:#}");
    assert_eq!(
        shadowed["definitions"][0]["kind"], "parameter",
        "{shadowed:#}"
    );
    assert!(
        shadowed["definitions"][0].get("fqn").is_none(),
        "{shadowed:#}"
    );
}
