use crate::common::{definition, go_analyzer_with_files};
use brokk_bifrost::CodeUnitIndex;
use brokk_bifrost::usages::{GoUsageGraphStrategy, UsageAnalyzer};

#[test]
fn go_promoted_generic_interface_method_keeps_exact_target() {
    let (project, analyzer) = go_analyzer_with_files(&[(
        "auth/auth.go",
        r#"
package auth

type Authenticator[T any] interface {
    Authenticate(value *T) error
}

type Handler[T any] struct {
    Authenticator[T]
}

func Use[T any](handler *Handler[T], value *T) error {
    return handler.Authenticate(value)
}

type Override[T any] struct {
    Authenticator[T]
}

func (*Override[T]) Authenticate(value *T) error {
    return nil
}

func UseOverride[T any](handler *Override[T], value *T) error {
    return handler.Authenticate(value)
}
"#,
    )]);
    let target = definition(&analyzer, "example.com/app/auth.Authenticator.Authenticate");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();

    let hits = GoUsageGraphStrategy::new()
        .find_usages(&analyzer, std::slice::from_ref(&target), &candidates, 1000)
        .into_either()
        .expect("generic embedded-interface promotion should resolve");

    assert_eq!(
        1,
        hits.len(),
        "only the promoted interface call should match: {hits:#?}"
    );
    let hit = hits.iter().next().expect("one promoted interface call");
    assert_eq!(hit.file, project.file("auth/auth.go"));
    assert!(hit.snippet.contains("handler.Authenticate(value)"));
    assert_eq!(
        hit.line, 13,
        "the direct override must not match: {hits:#?}"
    );
}
