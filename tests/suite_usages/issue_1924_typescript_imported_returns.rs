use crate::common::InlineTestProject;
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{AnalyzerConfig, Language};

#[test]
fn imported_typescript_return_type_preserves_receiver_after_reassignment() {
    let consumer = r#"
import type { State, OtherState } from "./state";
import { refresh as apply, refreshOther } from "./state";

export function run(state: State, otherState: OtherState) {
  const spread = { ...state };
  const spreadValue = spread.value;

  let current = apply(state);
  const importedValue = current.value;
  current = apply(current);
  const reassignedValue = current.value;

  let reassigned = { ...otherState };
  reassigned = refreshOther(otherState);
  const decoy = reassigned.value;
  return spreadValue + importedValue + reassignedValue + decoy;
}
"#;
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file(
            "state.ts",
            r#"
export interface State { value: number }
export interface OtherState { value: number }

export function refresh(state: State): State {
  return { ...state };
}

export function refreshOther(state: OtherState): OtherState {
  return { ...state };
}
"#,
        )
        .file("consumer.ts", consumer)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let analyzer = workspace.analyzer();
    let state_file = project.file("state.ts");
    let target = analyzer
        .get_declarations(&state_file)
        .into_iter()
        .find(|unit| unit.is_field() && unit.fq_name() == "State.value")
        .expect("State.value declaration");

    let result = UsageFinder::new()
        .query(analyzer, std::slice::from_ref(&target), 1000, 1000)
        .result;
    let consumer_file = project.file("consumer.ts");
    let reference_offsets = result
        .all_hits_including_imports()
        .into_iter()
        .filter(|hit| hit.file == consumer_file)
        .map(|hit| hit.start_offset)
        .collect::<Vec<_>>();

    for receiver in ["spread.value", "current.value"] {
        let wanted = consumer.find(receiver).expect("wanted reference")
            + receiver.find('.').expect("member separator")
            + 1;
        assert!(
            reference_offsets.contains(&wanted),
            "object spread and imported State returns must preserve the receiver at {receiver}: {result:#?}"
        );
    }
    let reassigned = consumer
        .rfind("current.value")
        .expect("reassigned State reference")
        + "current.".len();
    assert!(
        reference_offsets.contains(&reassigned),
        "same-type reassignment must preserve the State receiver: {result:#?}"
    );
    let decoy = consumer.find("reassigned.value").expect("decoy reference") + "reassigned.".len();
    assert!(
        !reference_offsets.contains(&decoy),
        "the same-named field on OtherState must not become a State.value usage: {result:#?}"
    );
}
