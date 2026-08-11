use crate::common::InlineTestProject;
use crate::common::usage_graph::{has_edge, usage_graph_at};
use brokk_bifrost::usages::{UsageFinder, UsageHitKind};
use brokk_bifrost::{CSharpAnalyzer, CodeUnit, CodeUnitIndex, CodeUnitType, Language};

fn member_function(analyzer: &CSharpAnalyzer, owner: &str, name: &str) -> CodeUnit {
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

#[test]
fn csharp_collection_initializer_method_groups_keep_their_exact_owner() {
    let source = r#"
namespace Demo;

public delegate void Handler();

public static class Owner
{
    public static void Wanted() { }
    public static void Other() { }

    public static readonly System.Collections.Generic.Dictionary<int, Handler> Map = new()
    {
        { 1, Wanted }, // dictionary-positive
        { 2, Other },
    };

    public static readonly Handler[] Handlers =
    {
        Wanted, // array-positive
    };

    public static Handler[] LocalShadow()
    {
        Handler Wanted = () => { };
        return new[] { Wanted }; // local-shadow
    }
}

public static class Decoy
{
    public static void Wanted() { }
    public static readonly Handler[] Handlers = { Wanted }; // decoy-owner
}
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("src/Fixture.cs", source)
        .build();
    let analyzer = CSharpAnalyzer::from_project(project.project().clone());
    let file = project.file("src/Fixture.cs");
    let target = member_function(&analyzer, "Demo.Owner", "Wanted");

    let result = UsageFinder::new().find_usages_default(&analyzer, &[target]);
    let reference_offsets = result
        .all_hits()
        .iter()
        .filter(|hit| hit.file == file && hit.kind == UsageHitKind::Reference)
        .map(|hit| hit.start_offset)
        .collect::<Vec<_>>();
    let expected = [
        "Wanted }, // dictionary-positive",
        "Wanted, // array-positive",
    ]
    .map(|marker| source.find(marker).expect("positive method group"));
    assert_eq!(reference_offsets, expected, "hits={:#?}", result.all_hits());

    for marker in ["Wanted }; // local-shadow", "Wanted }; // decoy-owner"] {
        let offset = source.find(marker).expect("method-group near miss");
        assert!(
            result
                .all_hits()
                .iter()
                .all(|hit| hit.start_offset != offset),
            "near miss {marker:?} must not match the target: {:#?}",
            result.all_hits()
        );
    }
}

#[test]
fn csharp_generic_extension_dispatch_records_the_call_and_handler_group() {
    let extensions = r#"
namespace Xunit.Sdk;

public interface IMessageSinkMessage { }
public sealed class TestMessage : IMessageSinkMessage { }
public sealed class OtherMessage : IMessageSinkMessage { }
public sealed class LastMessage : IMessageSinkMessage { }
public delegate void MessageHandler<TMessage>(TMessage message)
    where TMessage : IMessageSinkMessage;

public static class MessageSinkMessageExtensions
{
    public static bool DispatchWhen<TMessage>(
        this IMessageSinkMessage message,
        MessageHandler<TMessage>? callback)
        where TMessage : IMessageSinkMessage => true;
}
"#;
    let consumer = r#"
using Xunit.Sdk;

namespace Xunit.Runner.Common;

public sealed class ExecutionSink
{
    private void HandleTestMessage(TestMessage message) { }
    private void HandleOtherMessage(OtherMessage message) { }
    private void HandleLastMessage(LastMessage message) { }

    public bool Run(IMessageSinkMessage message)
    {
        return message.DispatchWhen<TestMessage>(HandleTestMessage) && true;
    }

    public bool RunAssigned(IMessageSinkMessage message)
    {
        var result =
            message.DispatchWhen<TestMessage>(HandleTestMessage)
            && message.DispatchWhen<OtherMessage>(HandleOtherMessage)
            && message.DispatchWhen<LastMessage>(HandleLastMessage);
        return result;
    }

    public bool Comparison(
        IMessageSinkMessage message,
        int TestMessage,
        bool HandleTestMessage)
    {
        return message.DispatchWhen < TestMessage > (HandleTestMessage) && true;
    }
}
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("src/Extensions.cs", extensions)
        .file("src/ExecutionSink.cs", consumer)
        .build();
    let analyzer = CSharpAnalyzer::from_project(project.project().clone());
    let consumer_file = project.file("src/ExecutionSink.cs");
    let extension = member_function(
        &analyzer,
        "Xunit.Sdk.MessageSinkMessageExtensions",
        "DispatchWhen",
    );
    let extension_result = UsageFinder::new().find_usages_default(&analyzer, &[extension]);
    let extension_offsets = extension_result
        .all_hits()
        .iter()
        .filter(|hit| hit.file == consumer_file && hit.kind == UsageHitKind::Reference)
        .map(|hit| hit.start_offset)
        .collect::<Vec<_>>();
    let expected_extension_offsets = [
        "DispatchWhen<TestMessage>(HandleTestMessage) && true",
        "DispatchWhen<TestMessage>(HandleTestMessage)\n            &&",
        "DispatchWhen<OtherMessage>",
        "DispatchWhen<LastMessage>",
    ]
    .map(|marker| consumer.find(marker).expect("expected extension call"));
    assert_eq!(
        extension_offsets,
        expected_extension_offsets,
        "the recovered calls must exclude the value-shadowed comparison: {:#?}",
        extension_result.all_hits(),
    );

    for (handler_name, markers) in [
        (
            "HandleTestMessage",
            vec![
                "HandleTestMessage) && true",
                "HandleTestMessage)\n            &&",
            ],
        ),
        (
            "HandleOtherMessage",
            vec!["HandleOtherMessage)\n            &&"],
        ),
        ("HandleLastMessage", vec!["HandleLastMessage);"]),
    ] {
        let handler = member_function(&analyzer, "Xunit.Runner.Common.ExecutionSink", handler_name);
        let result = UsageFinder::new().find_usages_default(&analyzer, &[handler]);
        let expected = markers
            .iter()
            .map(|marker| consumer.find(marker).expect("expected handler group"))
            .collect::<Vec<_>>();
        let offsets = result
            .all_hits()
            .iter()
            .filter(|hit| hit.file == consumer_file && hit.kind == UsageHitKind::Reference)
            .map(|hit| hit.start_offset)
            .collect::<Vec<_>>();
        assert_eq!(
            offsets,
            expected,
            "the recovered call must exclude the value-shadowed comparison: {:#?}",
            result.all_hits(),
        );
    }

    let graph = usage_graph_at(project.root(), "{}");
    for target in [
        "Xunit.Sdk.MessageSinkMessageExtensions.DispatchWhen",
        "Xunit.Runner.Common.ExecutionSink.HandleTestMessage",
    ] {
        assert!(
            has_edge(&graph, "Xunit.Runner.Common.ExecutionSink.Run", target),
            "recovered generic call must produce the exact inverted edge to {target}: {}",
            graph["edges"]
        );
        assert!(
            !has_edge(
                &graph,
                "Xunit.Runner.Common.ExecutionSink.Comparison",
                target
            ),
            "value-shadowed comparison must not become a recovered call to {target}: {}",
            graph["edges"]
        );
    }
    for target in [
        "Xunit.Sdk.MessageSinkMessageExtensions.DispatchWhen",
        "Xunit.Runner.Common.ExecutionSink.HandleTestMessage",
        "Xunit.Runner.Common.ExecutionSink.HandleOtherMessage",
        "Xunit.Runner.Common.ExecutionSink.HandleLastMessage",
    ] {
        assert!(
            has_edge(
                &graph,
                "Xunit.Runner.Common.ExecutionSink.RunAssigned",
                target
            ),
            "assigned recovery chain must produce the exact inverted edge to {target}: {}",
            graph["edges"]
        );
    }
}
