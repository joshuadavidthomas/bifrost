# Issue #1480, Milestone 0: claimed-language probe

This note records the declared semantic capability support that Milestone 0 of
`.agents/plans/issue-1480-flow-sensitive-state-bounded-rewrite.md` needs. The
flow-state family (state events plus reaching-definition, dominance, and
same-evaluation relations) is built only on the production semantic CFG, so the
claimed-language set is exactly the set of language adapters that declare CFG
lowering support.

The table below is read from the adapter capability tables in the source, not
from documentation. Each row is one `*_capabilities()` function:

- `crates/bifrost-analysis/src/analyzer/cpp/semantic.rs` (`cpp_capabilities`)
- `crates/bifrost-analysis/src/analyzer/csharp/semantic.rs` (`csharp_capabilities`)
- `crates/bifrost-analysis/src/analyzer/go/semantic.rs` (`go_capabilities`)
- `crates/bifrost-analysis/src/analyzer/java/semantic/mod.rs` (`java_capabilities`)
- `crates/bifrost-analysis/src/analyzer/js_ts/semantic/mod.rs` (`js_ts_capabilities`)
- `crates/bifrost-analysis/src/analyzer/kotlin/semantic/mod.rs` (`kotlin_capabilities`)
- `crates/bifrost-analysis/src/analyzer/php/semantic.rs` (`php_capabilities`)
- `crates/bifrost-analysis/src/analyzer/python/semantic.rs` (`python_capabilities`)
- `crates/bifrost-analysis/src/analyzer/ruby/semantic.rs` (`ruby_capabilities`)
- `crates/bifrost-analysis/src/analyzer/rust/semantic.rs` (`rust_capabilities`)
- `crates/bifrost-analysis/src/analyzer/scala/semantic.rs` (`scala_capabilities`)

The capability enum and the `Complete` / `Partial` / `Unsupported` ladder are
declared in `crates/bifrost-analysis/src/analyzer/semantic/capabilities.rs`. A
capability that an adapter never names is `Unsupported` by default, because the
table is total.

## Declared support for the six capabilities this plan reads

| Language | Procedures | ProgramPoints | NormalControlFlow | Assignments | Values | LocalFlow |
| --- | --- | --- | --- | --- | --- | --- |
| C++ | Complete | Complete | Partial | Partial | Partial | Partial |
| C# | Complete | Complete | Partial | Partial | Partial | Partial |
| Go | Complete | Complete | Partial | Partial | Partial | Partial |
| Java | Complete | Complete | Partial | Partial | Partial | Partial |
| JavaScript / TypeScript | Complete | Complete | Partial | Partial | Partial | Partial |
| Kotlin | Complete | Complete | Partial | Partial | Partial | Partial |
| PHP | Complete | Complete | Partial | Partial | Partial | Partial |
| Python | Complete | Complete | Partial | Partial | Partial | Partial |
| Ruby | Complete | Complete | Partial | Partial | Partial | Partial |
| Rust | Complete | Complete | Partial | Partial | Partial | Partial |
| Scala | Complete | Complete | Partial | Partial | Partial | Partial |

## Which languages qualify

The flow-state family needs at least `Partial` on `Procedures`,
`ProgramPoints`, `NormalControlFlow`, and `Assignments`. All eleven adapters
above meet that bar, so the claimed-language set for Milestone 2 and Milestone 3
is the full adapter set: C++, C#, Go, Java, JavaScript, TypeScript, Kotlin, PHP,
Python, Ruby, Rust, and Scala. JavaScript and TypeScript share one adapter and
one capability table, so they qualify or fail together.

No language qualifies at `Complete` depth. Every adapter declares `Procedures`
and `ProgramPoints` as `Complete` and every adapter declares
`NormalControlFlow`, `Assignments`, `Values`, and `LocalFlow` as `Partial`.
There is therefore no "deep" tier to distinguish inside the flow-state family on
these six axes: the depth distinction that the sibling plans drew between
claimed languages does not exist here.

Two consequences for the later milestones:

1. Per-language claim gating cannot be derived from these six capabilities
   alone, because they do not discriminate. Milestone 2 must state per-axis
   completeness from the actual lowering result of each procedure (the CFG rows
   already carry `partial` completeness and a `semantic_analysis_partial`
   diagnostic today), not from a static language allow list.
2. `Values` and `LocalFlow` are `Partial` everywhere, so a flow relation
   derived from them must report `may` certainty unless the derivation itself
   proves the all-paths property from the CFG. Reading `Complete` off a
   capability table will never be available as a shortcut.

C has no capability table of its own. The C++ lowerer owns it: the module doc
of `crates/bifrost-analysis/src/analyzer/cpp/semantic.rs` states that it lowers
"C and C++", and `CppSemanticLowerer::capabilities` returns `cpp_capabilities()`
for both. C therefore inherits the C++ row above.
