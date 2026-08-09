// Parked verbatim by Phase 1 of `.agents/plans/port-optimization-arc-to-upstream.md`.
//
// These are the usage-v2 arc's store-level pins for the `rust_*` fact tables,
// removed from `crates/bifrost-analysis/src/analyzer/store/mod.rs`'s test module
// only because the code they exercise is parked in `store-rust-facts.rs`.
// Phase 2 restores both together. They are the specification: each one names a
// property the fact tables must have, and none of them is replaced by anything
// upstream's index does.
//
// This file is not a Cargo module and is never compiled.

    #[test]
    fn rust_fact_tables_record_exports_imports_modules_and_occurrences() {
        let temp = tempfile::TempDir::new().unwrap();
        let (store, oid) = rust_usage_fact_store(temp.path());
        let facts = store.rust_usage_facts(oid, "rust").unwrap();

        let exports: Vec<_> = facts
            .exports
            .iter()
            .map(|export| {
                (
                    export.exported_name.as_deref(),
                    export.source_path.as_str(),
                    export.imported_name.as_deref(),
                    export.is_glob,
                )
            })
            .collect();
        assert_eq!(
            exports,
            vec![
                (Some("Exported"), "alpha", Some("Exported"), false),
                (Some("Alias"), "beta", Some("Renamed"), false),
                (None, "gamma", None, true),
            ],
            "private and non-root `use` declarations are not exports: {:?}",
            facts.exports
        );

        let imports: Vec<_> = facts
            .import_targets
            .iter()
            .map(|target| {
                (
                    target.module_path.as_str(),
                    target.bound_name.as_deref(),
                    target.is_glob,
                    target.owner_module.as_str(),
                    target.local_extent.is_some(),
                )
            })
            .collect();
        assert_eq!(
            imports,
            vec![
                ("alpha", Some("Exported"), false, "", false),
                ("beta", Some("Alias"), false, "", false),
                ("gamma", None, true, "", false),
                ("delta", Some("Private"), false, "", false),
                ("crate", Some("Scoped"), false, "inline", false),
                ("crate", Some("Local"), false, "inline", true),
            ],
            "import rows were {:?}",
            facts.import_targets
        );
        assert_eq!(
            facts.import_targets[0].visibility,
            crate::analyzer::rust::imports::RustVisibility::Public
        );
        assert_eq!(
            facts.import_targets[3].visibility,
            crate::analyzer::rust::imports::RustVisibility::Private
        );

        let modules: Vec<_> = facts
            .modules
            .iter()
            .map(|module| (module.module_name.as_str(), module.is_inline))
            .collect();
        assert_eq!(
            modules,
            vec![("", true), ("detached", false), ("inline", true)],
            "module rows were {:?}",
            facts.modules
        );
        assert_eq!(facts.modules[0].start_byte, 0);
        assert_eq!(facts.modules[0].end_byte, RUST_USAGE_FACT_FIXTURE.len());

        let mask = |name: &str| {
            facts
                .identifier_occurrences
                .iter()
                .find(|occurrence| occurrence.identifier == name)
                .map(|occurrence| occurrence.context_mask)
        };
        assert_eq!(
            mask("helper"),
            Some(crate::analyzer::rust::facts::RUST_OCCURRENCE_CODE)
        );
        assert_eq!(
            mask("in_a_comment"),
            Some(crate::analyzer::rust::facts::RUST_OCCURRENCE_COMMENT)
        );
        assert_eq!(
            mask("in_a_string"),
            Some(crate::analyzer::rust::facts::RUST_OCCURRENCE_STRING)
        );
    }

    #[test]
    fn rust_fact_tables_answer_the_inverted_name_lookups() {
        let temp = tempfile::TempDir::new().unwrap();
        let (store, oid) = rust_usage_fact_store(temp.path());

        assert_eq!(store.rust_export_blobs("rust", "Alias").unwrap(), vec![oid]);
        assert!(
            store
                .rust_export_blobs("rust", "Private")
                .unwrap()
                .is_empty(),
            "a private import must not answer an export lookup"
        );
        assert_eq!(
            store.rust_import_target_blobs("rust", "delta").unwrap(),
            vec![oid]
        );
        assert_eq!(
            store
                .rust_identifier_occurrence_blobs("rust", "helper")
                .unwrap(),
            vec![(oid, crate::analyzer::rust::facts::RUST_OCCURRENCE_CODE)]
        );
        assert!(
            store
                .rust_identifier_occurrence_blobs("rust", "HELPER")
                .unwrap()
                .is_empty(),
            "identifier lookups are case-sensitive"
        );
        assert!(
            store
                .rust_identifier_occurrence_blobs("java", "helper")
                .unwrap()
                .is_empty(),
            "identifier lookups are scoped to one language"
        );
    }

    #[test]
    fn rust_fact_rows_cascade_with_their_blob() {
        let temp = tempfile::TempDir::new().unwrap();
        let (store, oid) = rust_usage_fact_store(temp.path());
        let count = |store: &AnalyzerStore| {
            let conn = store.conn.lock().unwrap();
            [
                "rust_exports",
                "rust_import_targets",
                "rust_modules",
                "rust_identifier_occurrences",
                "rust_module_scopes",
                "rust_module_routes",
                "rust_module_route_gates",
                "rust_item_macros",
            ]
            .into_iter()
            .map(|table| {
                conn.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE blob_oid = ?1 AND lang = ?2"),
                    params![oid.to_string(), "rust"],
                    |row| row.get::<_, usize>(0),
                )
                .unwrap()
            })
            .sum::<usize>()
        };

        assert!(count(&store) > 0, "fixture must persist fact rows");
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "DELETE FROM blobs WHERE blob_oid = ?1 AND lang = ?2",
                params![oid.to_string(), "rust"],
            )
            .unwrap();
        }
        assert_eq!(
            count(&store),
            0,
            "deleting the blob must cascade every rust_* fact row away"
        );
    }

    #[test]
    fn rust_fact_rows_are_stable_across_a_re_analysis_of_the_same_content() {
        let temp = tempfile::TempDir::new().unwrap();
        let file = write_file(temp.path(), "src/lib.rs", RUST_USAGE_FACT_FIXTURE);
        let oid = oid_for(RUST_USAGE_FACT_FIXTURE.as_bytes());
        let store = AnalyzerStore::open_in_memory().unwrap();
        store
            .write_parsed_blob(oid, "rust", &RustAdapter, &parse_state(&RustAdapter, &file))
            .unwrap();
        let first = store.rust_usage_facts(oid, "rust").unwrap();

        // Same bytes at a different path: the content key is unchanged, so the
        // second analysis must produce byte-identical rows. Nothing persisted
        // here may be path-derived.
        let moved = write_file(temp.path(), "src/other/lib.rs", RUST_USAGE_FACT_FIXTURE);
        store
            .write_parsed_blob(
                oid,
                "rust",
                &RustAdapter,
                &parse_state(&RustAdapter, &moved),
            )
            .unwrap();
        let second = store.rust_usage_facts(oid, "rust").unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn rust_module_route_tables_record_scopes_routes_gates_and_item_macros() {
        let temp = tempfile::TempDir::new().unwrap();
        let (store, oid) = rust_module_route_store(temp.path(), "src/lib.rs");
        let routes = store.rust_usage_facts(oid, "rust").unwrap().module_routes;

        let scopes: Vec<_> = routes
            .scopes
            .iter()
            .map(|scope| {
                (
                    scope.parent,
                    scope.module_name.as_str(),
                    scope.path_attribute.as_deref(),
                    scope.imports_macros,
                )
            })
            .collect();
        assert_eq!(
            scopes,
            vec![
                (None, "", None, true),
                (Some(0), "scope", Some("elsewhere"), false),
            ],
            "scopes were {:?}",
            routes.scopes
        );
        assert_eq!(routes.scopes[0].body_start, 0);
        assert_eq!(
            routes.scopes[0].body_end,
            RUST_MODULE_ROUTE_FIXTURE.len(),
            "the root scope spans the whole source"
        );

        let described: Vec<_> = routes
            .routes
            .iter()
            .map(|route| {
                (
                    route.scope,
                    route.module_name.as_str(),
                    route.path_attribute.as_deref(),
                    route.imports_macros,
                    route.test_gated,
                    route
                        .gates
                        .iter()
                        .map(|gate| gate.macro_name.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            described,
            vec![
                (0, "plain", None, false, false, vec![]),
                (0, "macro_source", None, true, false, vec![]),
                (0, "gated", None, false, true, vec![]),
                (
                    0,
                    "relocated",
                    Some("custom/target.rs"),
                    false,
                    false,
                    vec![]
                ),
                (1, "deep", None, false, false, vec![]),
                (0, "replayed", None, false, false, vec!["replay"]),
            ],
            "routes were {:?}",
            routes.routes
        );
        assert_eq!(
            routes
                .item_macros
                .iter()
                .map(|definition| (definition.name.as_str(), definition.passthrough))
                .collect::<Vec<_>>(),
            vec![("replay", true)],
            "item macros were {:?}",
            routes.item_macros
        );
        let gate = &routes.routes[5].gates[0];
        assert_eq!(
            RUST_MODULE_ROUTE_FIXTURE
                .get(gate.invocation_start..)
                .map(|rest| rest.starts_with("replay! { mod replayed; }")),
            Some(true),
            "the gate points at the invocation that produced the route"
        );
    }

    #[test]
    fn rust_module_route_rows_cascade_with_their_blob() {
        let temp = tempfile::TempDir::new().unwrap();
        let (store, oid) = rust_module_route_store(temp.path(), "src/lib.rs");
        let counts = |store: &AnalyzerStore| {
            let conn = store.conn.lock().unwrap();
            [
                "rust_module_scopes",
                "rust_module_routes",
                "rust_module_route_gates",
                "rust_item_macros",
            ]
            .map(|table| {
                conn.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE blob_oid = ?1 AND lang = ?2"),
                    params![oid.to_string(), "rust"],
                    |row| row.get::<_, usize>(0),
                )
                .unwrap()
            })
        };

        assert!(
            counts(&store).iter().all(|count| *count > 0),
            "every module-route table must carry rows for this fixture: {:?}",
            counts(&store)
        );
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "DELETE FROM blobs WHERE blob_oid = ?1 AND lang = ?2",
                params![oid.to_string(), "rust"],
            )
            .unwrap();
        }
        assert_eq!(counts(&store), [0, 0, 0, 0]);
    }

    /// Nothing in the module-route rows may be path-derived: the blob key is a
    /// content hash, so the same bytes at a different path must produce
    /// byte-identical rows. Directory resolution belongs to the reader.
    #[test]
    fn rust_module_route_rows_are_stable_across_a_re_analysis_of_the_same_content() {
        let temp = tempfile::TempDir::new().unwrap();
        let (store, oid) = rust_module_route_store(temp.path(), "src/lib.rs");
        let first = store.rust_usage_facts(oid, "rust").unwrap().module_routes;
        assert!(!first.routes.is_empty(), "fixture must persist route rows");

        let moved = write_file(
            temp.path(),
            "src/deep/nested/mod.rs",
            RUST_MODULE_ROUTE_FIXTURE,
        );
        store
            .write_parsed_blob(
                oid,
                "rust",
                &RustAdapter,
                &parse_state(&RustAdapter, &moved),
            )
            .unwrap();

        assert_eq!(
            store.rust_usage_facts(oid, "rust").unwrap().module_routes,
            first
        );
    }

    /// The Cargo-route build reads every live blob at once, and that batched
    /// read must agree with the per-blob one it replaces column for column.
    #[test]
    fn batched_module_route_facts_match_the_per_blob_read() {
        let temp = tempfile::TempDir::new().unwrap();
        let (store, oid) = rust_module_route_store(temp.path(), "src/lib.rs");
        let other = write_file(temp.path(), "src/plain.rs", "mod leaf;\n");
        let other_oid = oid_for(b"mod leaf;\n");
        store
            .write_parsed_blob(
                other_oid,
                "rust",
                &RustAdapter,
                &parse_state(&RustAdapter, &other),
            )
            .unwrap();
        let absent = oid_for(b"pub struct NeverAnalyzed;\n");

        let batched = store
            .rust_module_route_facts("rust", &[oid, other_oid, absent])
            .unwrap();

        assert_eq!(batched.len(), 2, "an unanalyzed blob contributes no entry");
        for key in [oid, other_oid] {
            assert_eq!(
                batched.get(&key),
                Some(&store.rust_usage_facts(key, "rust").unwrap().module_routes),
                "batched and per-blob reads disagree for {key}"
            );
        }
    }


// ---- fixtures the parked tests above use ----

    /// The fixture the `rust_*` fact-table tests share: one file that
    /// re-exports, imports (named, glob, aliased, function-local), declares an
    /// inline and a file module, and mentions identifiers in code, a comment,
    /// and a string.
    const RUST_USAGE_FACT_FIXTURE: &str = "\
pub use alpha::Exported;
pub use beta::Renamed as Alias;
pub use gamma::*;
use delta::Private;
mod detached;
mod inline {
    use crate::Scoped;
    pub fn helper() {
        use crate::Local;
        let _ = \"in_a_string\";
    }
}
// in_a_comment
";


    fn rust_usage_fact_store(temp: &Path) -> (AnalyzerStore, Oid) {
        let file = write_file(temp, "src/lib.rs", RUST_USAGE_FACT_FIXTURE);
        let oid = oid_for(RUST_USAGE_FACT_FIXTURE.as_bytes());
        let store = AnalyzerStore::open_in_memory().unwrap();
        store
            .write_parsed_blob(oid, "rust", &RustAdapter, &parse_state(&RustAdapter, &file))
            .unwrap();
        (store, oid)
    }


    /// The module-route fixture (issue #1793): a file whose declarations cover
    /// every column the Cargo route index reads -- an inline scope with a
    /// `#[path]`, a `#[macro_use]` declaration, a bare `#[cfg(test)]` gate, a
    /// `#[path]` on a declaration, an item macro definition, and a declaration
    /// that only exists inside that macro's expansion.
    const RUST_MODULE_ROUTE_FIXTURE: &str = "\
macro_rules! replay {
    ($($item:item)*) => { $($item)* };
}
mod plain;
#[macro_use]
mod macro_source;
#[cfg(test)]
mod gated;
#[path = \"custom/target.rs\"]
mod relocated;
#[path = \"elsewhere\"]
mod scope {
    mod deep;
}
replay! { mod replayed; }
";


    fn rust_module_route_store(temp: &Path, rel_path: &str) -> (AnalyzerStore, Oid) {
        let file = write_file(temp, rel_path, RUST_MODULE_ROUTE_FIXTURE);
        let oid = oid_for(RUST_MODULE_ROUTE_FIXTURE.as_bytes());
        let store = AnalyzerStore::open_in_memory().unwrap();
        store
            .write_parsed_blob(oid, "rust", &RustAdapter, &parse_state(&RustAdapter, &file))
            .unwrap();
        (store, oid)
    }
