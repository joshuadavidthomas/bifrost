// Parked verbatim by Phase 1 of `.agents/plans/port-optimization-arc-to-upstream.md`.
//
// These are the `rust_*` fact-table row shapes, writer, reader and inverted
// lookups that the usage-v2 arc added to
// `crates/bifrost-analysis/src/analyzer/store/mod.rs`. Phase 1 restores
// upstream's whole-workspace `RustUsageIndex`, so nothing reads or writes these
// rows; the migrations that create the tables are retained (0017, 0018) so that
// Phase 2 re-lands the code without another schema version.
//
// This file is not a Cargo module and is never compiled.

// ==== store/mod.rs lines 3967-4285 at the Phase 1 merge ====
/// The four `rust_*` fact tables' rows for one blob, converted from
/// [`crate::analyzer::rust::facts::RustUsageFacts`] and validated for SQLite
/// binding.
///
/// Built during preparation rather than inside the write transaction, like
/// every other row shape here: the byte-offset conversions are the only thing
/// that can fail, and failing them must not abort a batch mid-commit. Empty for
/// every language except Rust.
#[derive(Debug, Default)]
struct RustFactRows {
    exports: Vec<RustExportRow>,
    import_targets: Vec<RustImportTargetRow>,
    modules: Vec<RustModuleRow>,
    /// `(identifier, context_mask)`
    identifier_occurrences: Vec<(String, i64)>,
    /// The `rust_module_scopes` / `rust_module_routes` /
    /// `rust_module_route_gates` / `rust_item_macros` rows (issue #1793).
    module_routes: RustModuleRouteRows,
}

/// The four module-route tables' rows for one blob.
#[derive(Debug, Default)]
struct RustModuleRouteRows {
    scopes: Vec<RustModuleScopeRow>,
    routes: Vec<RustModuleRouteRow>,
    /// `(route_ordinal, gate_ordinal, macro_name, invocation_start)`
    gates: Vec<(i64, i64, String, i64)>,
    item_macros: Vec<RustItemMacroRow>,
}

/// One `rust_module_scopes` row.
#[derive(Debug)]
struct RustModuleScopeRow {
    ordinal: i64,
    parent_ordinal: Option<i64>,
    module_name: String,
    path_attribute: Option<String>,
    imports_macros: i64,
    body_start: i64,
    body_end: i64,
}

/// One `rust_module_routes` row.
#[derive(Debug)]
struct RustModuleRouteRow {
    ordinal: i64,
    scope_ordinal: i64,
    module_name: String,
    path_attribute: Option<String>,
    visibility: String,
    imports_macros: i64,
    test_gated: i64,
    declaration_start: i64,
    declaration_end: i64,
}

/// One `rust_item_macros` row.
#[derive(Debug)]
struct RustItemMacroRow {
    ordinal: i64,
    macro_name: String,
    visible_after: i64,
    scope_start: i64,
    scope_end: i64,
    passthrough: i64,
}

/// One `rust_exports` row.
#[derive(Debug)]
struct RustExportRow {
    ordinal: i64,
    exported_name: Option<String>,
    source_path: String,
    imported_name: Option<String>,
    is_glob: i64,
}

/// One `rust_modules` row.
#[derive(Debug)]
struct RustModuleRow {
    ordinal: i64,
    module_name: String,
    is_inline: i64,
    start_byte: i64,
    end_byte: i64,
}

/// One `rust_import_targets` row. Named fields rather than positional columns
/// because there are eleven of them at the binding site.
#[derive(Debug)]
struct RustImportTargetRow {
    ordinal: i64,
    module_path: String,
    bound_name: Option<String>,
    imported_name: Option<String>,
    is_glob: i64,
    visibility: String,
    owner_module: String,
    owner_start: i64,
    owner_end: i64,
    local_start: Option<i64>,
    local_end: Option<i64>,
}

impl RustFactRows {
    fn from_facts(facts: &crate::analyzer::rust::facts::RustUsageFacts) -> Result<Self> {
        let exports = facts
            .exports
            .iter()
            .enumerate()
            .map(|(ordinal, export)| {
                Ok(RustExportRow {
                    ordinal: usize_to_i64(ordinal)?,
                    exported_name: export.exported_name.clone(),
                    source_path: export.source_path.clone(),
                    imported_name: export.imported_name.clone(),
                    is_glob: bool_to_i64(export.is_glob),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let import_targets = facts
            .import_targets
            .iter()
            .enumerate()
            .map(|(ordinal, target)| {
                let (local_start, local_end) = match target.local_extent {
                    Some((start, end)) => (Some(usize_to_i64(start)?), Some(usize_to_i64(end)?)),
                    None => (None, None),
                };
                Ok(RustImportTargetRow {
                    ordinal: usize_to_i64(ordinal)?,
                    module_path: target.module_path.clone(),
                    bound_name: target.bound_name.clone(),
                    imported_name: target.imported_name.clone(),
                    is_glob: bool_to_i64(target.is_glob),
                    visibility: encode_rust_visibility(&target.visibility),
                    owner_module: target.owner_module.clone(),
                    owner_start: usize_to_i64(target.owner_start)?,
                    owner_end: usize_to_i64(target.owner_end)?,
                    local_start,
                    local_end,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let modules = facts
            .modules
            .iter()
            .enumerate()
            .map(|(ordinal, module)| {
                Ok(RustModuleRow {
                    ordinal: usize_to_i64(ordinal)?,
                    module_name: module.module_name.clone(),
                    is_inline: bool_to_i64(module.is_inline),
                    start_byte: usize_to_i64(module.start_byte)?,
                    end_byte: usize_to_i64(module.end_byte)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let identifier_occurrences = facts
            .identifier_occurrences
            .iter()
            .map(|occurrence| {
                (
                    occurrence.identifier.clone(),
                    i64::from(occurrence.context_mask),
                )
            })
            .collect();
        let module_routes = RustModuleRouteRows::from_facts(&facts.module_routes)?;
        Ok(Self {
            exports,
            import_targets,
            modules,
            identifier_occurrences,
            module_routes,
        })
    }

    fn logical_rows(&self) -> usize {
        saturating_sum([
            self.exports.len(),
            self.import_targets.len(),
            self.modules.len(),
            self.identifier_occurrences.len(),
            self.module_routes.logical_rows(),
        ])
    }

    fn string_bytes(&self) -> usize {
        saturating_sum([
            saturating_sum(self.exports.iter().map(|row| {
                saturating_sum([
                    row.exported_name.as_ref().map_or(0, String::len),
                    row.source_path.len(),
                    row.imported_name.as_ref().map_or(0, String::len),
                ])
            })),
            saturating_sum(self.import_targets.iter().map(|row| {
                saturating_sum([
                    row.module_path.len(),
                    row.bound_name.as_ref().map_or(0, String::len),
                    row.imported_name.as_ref().map_or(0, String::len),
                    row.visibility.len(),
                    row.owner_module.len(),
                ])
            })),
            saturating_sum(self.modules.iter().map(|row| row.module_name.len())),
            saturating_sum(
                self.identifier_occurrences
                    .iter()
                    .map(|(identifier, _)| identifier.len()),
            ),
            self.module_routes.string_bytes(),
        ])
    }
}

impl RustModuleRouteRows {
    fn from_facts(facts: &crate::analyzer::rust::facts::RustModuleRouteFacts) -> Result<Self> {
        let scopes = facts
            .scopes
            .iter()
            .enumerate()
            .map(|(ordinal, scope)| {
                Ok(RustModuleScopeRow {
                    ordinal: usize_to_i64(ordinal)?,
                    parent_ordinal: scope.parent.map(usize_to_i64).transpose()?,
                    module_name: scope.module_name.clone(),
                    path_attribute: scope.path_attribute.clone(),
                    imports_macros: bool_to_i64(scope.imports_macros),
                    body_start: usize_to_i64(scope.body_start)?,
                    body_end: usize_to_i64(scope.body_end)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut routes = Vec::with_capacity(facts.routes.len());
        let mut gates = Vec::new();
        for (ordinal, route) in facts.routes.iter().enumerate() {
            let ordinal = usize_to_i64(ordinal)?;
            routes.push(RustModuleRouteRow {
                ordinal,
                scope_ordinal: usize_to_i64(route.scope)?,
                module_name: route.module_name.clone(),
                path_attribute: route.path_attribute.clone(),
                visibility: encode_rust_visibility(&route.visibility),
                imports_macros: bool_to_i64(route.imports_macros),
                test_gated: bool_to_i64(route.test_gated),
                declaration_start: usize_to_i64(route.declaration_start)?,
                declaration_end: usize_to_i64(route.declaration_end)?,
            });
            for (gate_ordinal, gate) in route.gates.iter().enumerate() {
                gates.push((
                    ordinal,
                    usize_to_i64(gate_ordinal)?,
                    gate.macro_name.clone(),
                    usize_to_i64(gate.invocation_start)?,
                ));
            }
        }
        let item_macros = facts
            .item_macros
            .iter()
            .enumerate()
            .map(|(ordinal, definition)| {
                Ok(RustItemMacroRow {
                    ordinal: usize_to_i64(ordinal)?,
                    macro_name: definition.name.clone(),
                    visible_after: usize_to_i64(definition.visible_after)?,
                    scope_start: usize_to_i64(definition.scope_start)?,
                    scope_end: usize_to_i64(definition.scope_end)?,
                    passthrough: bool_to_i64(definition.passthrough),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            scopes,
            routes,
            gates,
            item_macros,
        })
    }

    fn logical_rows(&self) -> usize {
        saturating_sum([
            self.scopes.len(),
            self.routes.len(),
            self.gates.len(),
            self.item_macros.len(),
        ])
    }

    fn string_bytes(&self) -> usize {
        saturating_sum([
            saturating_sum(self.scopes.iter().map(|row| {
                saturating_sum([
                    row.module_name.len(),
                    row.path_attribute.as_ref().map_or(0, String::len),
                ])
            })),
            saturating_sum(self.routes.iter().map(|row| {
                saturating_sum([
                    row.module_name.len(),
                    row.path_attribute.as_ref().map_or(0, String::len),
                    row.visibility.len(),
                ])
            })),
            saturating_sum(self.gates.iter().map(|(_, _, name, _)| name.len())),
            saturating_sum(self.item_macros.iter().map(|row| row.macro_name.len())),
        ])
    }
}

/// Read access to the per-file Rust usage fact tables.
///
/// Milestone 1 of `.agents/plans/rust-usage-index-v2.md` lands the tables and
/// this reader; Milestone 2's `RustUsageQueries` is the caller. The allow goes
/// away with that commit -- keeping the reader beside the writer it inverts is
/// what makes the round trip reviewable in one place.
#[allow(dead_code)]

// ==== store/mod.rs lines 4287-4557 at the Phase 1 merge ====
    /// Every persisted per-file Rust usage fact for one blob.
    ///
    /// This is the forward direction of the `rust_*` fact tables: "what does
    /// this file export, import, declare, and mention". A caller that already
    /// knows the file reads it directly; a caller searching by name reaches
    /// these rows through the inverted lookups below and then verifies each
    /// candidate against its facts.
    pub(crate) fn rust_usage_facts(&self, oid: Oid, lang: &str) -> Result<RustUsageFacts> {
        let conn = self.read_conn()?;
        read_rust_usage_facts(&conn, &oid.to_string(), lang)
    }

    /// Blobs that import `module_path`, spelled exactly as the importing file
    /// writes it. The inverted direction of `rust_import_targets`.
    pub(crate) fn rust_import_target_blobs(
        &self,
        lang: &str,
        module_path: &str,
    ) -> Result<Vec<Oid>> {
        self.rust_fact_blobs(
            "SELECT DISTINCT blob_oid FROM rust_import_targets
             WHERE lang = ?1 AND module_path = ?2",
            lang,
            module_path,
        )
    }

    /// Blobs that re-export `exported_name`. The inverted direction of
    /// `rust_exports`, and the seed of an export-chain walk.
    pub(crate) fn rust_export_blobs(&self, lang: &str, exported_name: &str) -> Result<Vec<Oid>> {
        self.rust_fact_blobs(
            "SELECT DISTINCT blob_oid FROM rust_exports
             WHERE lang = ?1 AND exported_name = ?2",
            lang,
            exported_name,
        )
    }

    /// Blobs whose text mentions `identifier`, with the OR of the contexts it
    /// was seen in. These are CANDIDATES, never usages: a hit means the name
    /// occurs, and the caller must still resolve it against the candidate's
    /// own facts. Comparison is case-sensitive, matching the spelling the
    /// declaration side stores.
    pub(crate) fn rust_identifier_occurrence_blobs(
        &self,
        lang: &str,
        identifier: &str,
    ) -> Result<Vec<(Oid, u32)>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT blob_oid, context_mask FROM rust_identifier_occurrences
             WHERE lang = ?1 AND identifier = ?2",
        )?;
        let rows = stmt.query_map(params![lang, identifier], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (oid, context_mask) = row?;
            out.push((
                Oid::from_str(&oid)?,
                u32::try_from(context_mask).map_err(|_| {
                    StoreError::new(format!(
                        "occurrence context mask out of range: {context_mask}"
                    ))
                })?,
            ));
        }
        out.sort();
        Ok(out)
    }

    /// Test hook: drop every persisted Rust fact row for `lang`, leaving the
    /// blobs analyzed.
    ///
    /// This synthesizes the exact state the Milestone 3 catch-up policy exists
    /// for -- live files whose blobs carry no fact rows -- which no production
    /// path can be asked to produce on demand. It follows
    /// `mark_parsed_blob_incomplete_for_test`, the store's existing way of
    /// putting itself into a state only recovery code should see.
    #[cfg(test)]
    pub(crate) fn delete_rust_facts_for_test(&self, lang: &str) {
        let conn = self.conn.lock().expect("analyzer store mutex poisoned");
        for table in [
            "rust_exports",
            "rust_import_targets",
            "rust_modules",
            "rust_identifier_occurrences",
            "rust_module_scopes",
            "rust_module_routes",
            "rust_module_route_gates",
            "rust_item_macros",
        ] {
            conn.execute(
                &format!("DELETE FROM {table} WHERE lang = ?1"),
                params![lang],
            )
            .expect("delete rust fact rows");
        }
    }

    /// Which of `oids` already carry Rust fact rows.
    ///
    /// `rust_modules` is the witness table: every analyzed Rust blob records
    /// its file-root extent at ordinal 0, so a blob absent from it has no facts
    /// at all. That is the same rule the reader applies when it treats an empty
    /// module list as "never analyzed" (`RustAnalyzer::rust_usage_facts_of_blob`).
    ///
    /// Chunked set membership over the primary key, following
    /// `parsed_blob_keys_conn_with_condition`: each chunk is a batch of index
    /// seeks, so the cost tracks the live file set rather than the table's
    /// accumulated history.
    pub(crate) fn blobs_with_rust_facts(&self, lang: &str, oids: &[Oid]) -> Result<HashSet<Oid>> {
        const OIDS_PER_QUERY: usize = 400;
        let mut unique: Vec<String> = oids.iter().map(Oid::to_string).collect();
        unique.sort();
        unique.dedup();
        let conn = self.read_conn()?;
        let mut present = set_with_capacity(unique.len());
        for chunk in unique.chunks(OIDS_PER_QUERY) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT DISTINCT blob_oid FROM rust_modules
                 WHERE lang = ? AND blob_oid IN ({placeholders})"
            );
            let mut stmt = conn.prepare_cached(&sql)?;
            let parameters = std::iter::once(lang).chain(chunk.iter().map(String::as_str));
            let rows =
                stmt.query_map(params_from_iter(parameters), |row| row.get::<_, String>(0))?;
            for row in rows {
                present.insert(Oid::from_str(&row?)?);
            }
        }
        Ok(present)
    }

    /// Every live blob's module-route facts, in one chunked pass.
    ///
    /// This is what replaced hydrating and parsing every analyzed Rust file to
    /// build `RustCargoRouteIndex` (issue #1793). The index is a
    /// whole-workspace product, so it genuinely needs every file's rows; asking
    /// per blob would be tens of thousands of round trips, where four chunked
    /// index seeks per batch is a scan of exactly the rows that exist.
    ///
    /// A blob with no rows is absent from the result, which the caller
    /// distinguishes from "this file declares nothing".
    pub(crate) fn rust_module_route_facts(
        &self,
        lang: &str,
        oids: &[Oid],
    ) -> Result<HashMap<Oid, RustModuleRouteFacts>> {
        const OIDS_PER_QUERY: usize = 400;
        let mut unique: Vec<String> = oids.iter().map(Oid::to_string).collect();
        unique.sort();
        unique.dedup();
        let conn = self.read_conn()?;
        let mut by_oid: HashMap<Oid, RustModuleRouteFacts> = HashMap::default();
        for chunk in unique.chunks(OIDS_PER_QUERY) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(", ");
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT blob_oid, parent_ordinal, module_name, path_attribute, imports_macros,
                        body_start, body_end
                 FROM rust_module_scopes
                 WHERE lang = ? AND blob_oid IN ({placeholders})
                 ORDER BY blob_oid, ordinal"
            ))?;
            let rows = stmt.query_map(
                params_from_iter(std::iter::once(lang).chain(chunk.iter().map(String::as_str))),
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        decode_rust_module_scope_row(row, 1)?,
                    ))
                },
            )?;
            for row in rows {
                let (oid, scope) = row?;
                by_oid
                    .entry(Oid::from_str(&oid)?)
                    .or_default()
                    .scopes
                    .push(scope?);
            }
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT blob_oid, scope_ordinal, module_name, path_attribute, visibility,
                        imports_macros, test_gated, declaration_start, declaration_end
                 FROM rust_module_routes
                 WHERE lang = ? AND blob_oid IN ({placeholders})
                 ORDER BY blob_oid, ordinal"
            ))?;
            let rows = stmt.query_map(
                params_from_iter(std::iter::once(lang).chain(chunk.iter().map(String::as_str))),
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        decode_rust_module_route_row(row, 1)?,
                    ))
                },
            )?;
            for row in rows {
                let (oid, route) = row?;
                by_oid
                    .entry(Oid::from_str(&oid)?)
                    .or_default()
                    .routes
                    .push(route?);
            }
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT blob_oid, route_ordinal, macro_name, invocation_start
                 FROM rust_module_route_gates
                 WHERE lang = ? AND blob_oid IN ({placeholders})
                 ORDER BY blob_oid, route_ordinal, gate_ordinal"
            ))?;
            let rows = stmt.query_map(
                params_from_iter(std::iter::once(lang).chain(chunk.iter().map(String::as_str))),
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        decode_rust_module_route_gate_row(row, 1)?,
                    ))
                },
            )?;
            for row in rows {
                let (oid, gate) = row?;
                let (route_ordinal, gate) = gate?;
                let facts = by_oid.entry(Oid::from_str(&oid)?).or_default();
                attach_rust_module_route_gate(&mut facts.routes, route_ordinal, gate)?;
            }
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT blob_oid, macro_name, visible_after, scope_start, scope_end, passthrough
                 FROM rust_item_macros
                 WHERE lang = ? AND blob_oid IN ({placeholders})
                 ORDER BY blob_oid, ordinal"
            ))?;
            let rows = stmt.query_map(
                params_from_iter(std::iter::once(lang).chain(chunk.iter().map(String::as_str))),
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        decode_rust_item_macro_row(row, 1)?,
                    ))
                },
            )?;
            for row in rows {
                let (oid, definition) = row?;
                by_oid
                    .entry(Oid::from_str(&oid)?)
                    .or_default()
                    .item_macros
                    .push(definition?);
            }
        }
        Ok(by_oid)
    }

    fn rust_fact_blobs(&self, sql: &str, lang: &str, key: &str) -> Result<Vec<Oid>> {
        let conn = self.read_conn()?;
        let mut stmt = conn.prepare_cached(sql)?;
        let rows = stmt.query_map(params![lang, key], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(Oid::from_str(&row?)?);
        }
        out.sort();
        Ok(out)
    }
}

// ==== store/mod.rs lines 4559-4722 at the Phase 1 merge ====
/// Write one blob's `rust_*` fact rows. Shared by the prepared and legacy write
/// paths so both persist exactly the same rows.
fn insert_rust_fact_rows(
    tx: &Transaction<'_>,
    oid: &str,
    lang: &str,
    rows: &RustFactRows,
) -> Result<()> {
    if !rows.exports.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_exports(
               blob_oid, lang, ordinal, exported_name, source_path, imported_name, is_glob
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for row in &rows.exports {
            stmt.execute(params![
                oid,
                lang,
                row.ordinal,
                row.exported_name,
                row.source_path,
                row.imported_name,
                row.is_glob,
            ])?;
        }
    }
    if !rows.import_targets.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_import_targets(
               blob_oid, lang, ordinal, module_path, bound_name, imported_name, is_glob,
               visibility, owner_module, owner_start, owner_end, local_start, local_end
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        for row in &rows.import_targets {
            stmt.execute(params![
                oid,
                lang,
                row.ordinal,
                row.module_path,
                row.bound_name,
                row.imported_name,
                row.is_glob,
                row.visibility,
                row.owner_module,
                row.owner_start,
                row.owner_end,
                row.local_start,
                row.local_end,
            ])?;
        }
    }
    if !rows.modules.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_modules(
               blob_oid, lang, ordinal, module_name, is_inline, start_byte, end_byte
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for row in &rows.modules {
            stmt.execute(params![
                oid,
                lang,
                row.ordinal,
                row.module_name,
                row.is_inline,
                row.start_byte,
                row.end_byte,
            ])?;
        }
    }
    if !rows.identifier_occurrences.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_identifier_occurrences(
               blob_oid, lang, identifier, context_mask
             ) VALUES(?1, ?2, ?3, ?4)",
        )?;
        for (identifier, context_mask) in &rows.identifier_occurrences {
            stmt.execute(params![oid, lang, identifier, context_mask])?;
        }
    }
    let routes = &rows.module_routes;
    if !routes.scopes.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_module_scopes(
               blob_oid, lang, ordinal, parent_ordinal, module_name, path_attribute,
               imports_macros, body_start, body_end
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for row in &routes.scopes {
            stmt.execute(params![
                oid,
                lang,
                row.ordinal,
                row.parent_ordinal,
                row.module_name,
                row.path_attribute,
                row.imports_macros,
                row.body_start,
                row.body_end,
            ])?;
        }
    }
    if !routes.routes.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_module_routes(
               blob_oid, lang, ordinal, scope_ordinal, module_name, path_attribute,
               visibility, imports_macros, test_gated, declaration_start, declaration_end
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for row in &routes.routes {
            stmt.execute(params![
                oid,
                lang,
                row.ordinal,
                row.scope_ordinal,
                row.module_name,
                row.path_attribute,
                row.visibility,
                row.imports_macros,
                row.test_gated,
                row.declaration_start,
                row.declaration_end,
            ])?;
        }
    }
    if !routes.gates.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_module_route_gates(
               blob_oid, lang, route_ordinal, gate_ordinal, macro_name, invocation_start
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (route_ordinal, gate_ordinal, macro_name, invocation_start) in &routes.gates {
            stmt.execute(params![
                oid,
                lang,
                route_ordinal,
                gate_ordinal,
                macro_name,
                invocation_start,
            ])?;
        }
    }
    if !routes.item_macros.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO rust_item_macros(
               blob_oid, lang, ordinal, macro_name, visible_after, scope_start, scope_end,
               passthrough
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for row in &routes.item_macros {
            stmt.execute(params![
                oid,
                lang,
                row.ordinal,
                row.macro_name,
                row.visible_after,
                row.scope_start,
                row.scope_end,
                row.passthrough,
            ])?;
        }
    }
    Ok(())
}


// ==== store/mod.rs lines 8261-8560 at the Phase 1 merge ====
/// Read back one blob's `rust_*` fact rows, in the order they were written.
///
/// The inverse of [`insert_rust_fact_rows`], and the only place the persisted
/// column encodings are decoded. A visibility this build did not write means
/// the row came from a schema this build does not own, which the schema-version
/// file name already prevents -- so it is an assertion, not a recovery path.
#[allow(dead_code)]
fn read_rust_usage_facts(conn: &Connection, oid: &str, lang: &str) -> Result<RustUsageFacts> {
    let mut exports = Vec::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT exported_name, source_path, imported_name, is_glob FROM rust_exports
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| {
            Ok(RustExportFact {
                exported_name: row.get(0)?,
                source_path: row.get(1)?,
                imported_name: row.get(2)?,
                is_glob: row.get::<_, i64>(3)? != 0,
            })
        })?;
        for row in rows {
            exports.push(row?);
        }
    }
    let mut import_targets = Vec::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT module_path, bound_name, imported_name, is_glob, visibility,
                    owner_module, owner_start, owner_end, local_start, local_end
             FROM rust_import_targets
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| {
            Ok((
                RustImportTargetFact {
                    module_path: row.get(0)?,
                    bound_name: row.get(1)?,
                    imported_name: row.get(2)?,
                    is_glob: row.get::<_, i64>(3)? != 0,
                    visibility: RustVisibility::Private,
                    owner_module: row.get(5)?,
                    owner_start: 0,
                    owner_end: 0,
                    local_extent: None,
                },
                row.get::<_, String>(4)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<i64>>(9)?,
            ))
        })?;
        for row in rows {
            let (mut target, visibility, owner_start, owner_end, local_start, local_end) = row?;
            target.visibility = decode_rust_visibility(&visibility)
                .unwrap_or_else(|| panic!("unknown persisted Rust visibility: {visibility}"));
            target.owner_start = i64_to_usize(owner_start)?;
            target.owner_end = i64_to_usize(owner_end)?;
            target.local_extent = match (local_start, local_end) {
                (Some(start), Some(end)) => Some((i64_to_usize(start)?, i64_to_usize(end)?)),
                (None, None) => None,
                mismatched => panic!("half-open persisted local import extent: {mismatched:?}"),
            };
            import_targets.push(target);
        }
    }
    let mut modules = Vec::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT module_name, is_inline, start_byte, end_byte FROM rust_modules
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? != 0,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (module_name, is_inline, start_byte, end_byte) = row?;
            modules.push(RustModuleFact {
                module_name,
                is_inline,
                start_byte: i64_to_usize(start_byte)?,
                end_byte: i64_to_usize(end_byte)?,
            });
        }
    }
    let mut identifier_occurrences = Vec::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT identifier, context_mask FROM rust_identifier_occurrences
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY identifier",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (identifier, context_mask) = row?;
            identifier_occurrences.push(RustIdentifierOccurrence {
                identifier,
                context_mask: u32::try_from(context_mask).map_err(|_| {
                    StoreError::new(format!(
                        "occurrence context mask out of range: {context_mask}"
                    ))
                })?,
            });
        }
    }
    let module_routes = read_rust_module_route_facts(conn, oid, lang)?;
    Ok(RustUsageFacts {
        exports,
        import_targets,
        modules,
        identifier_occurrences,
        module_routes,
    })
}

/// Read back one blob's module-route facts.
///
/// The per-blob inverse of the `rust_module_*` / `rust_item_macros` inserts.
/// The Cargo-route build does NOT come through here -- it reads every live
/// blob's rows in one chunked pass (`AnalyzerStore::rust_module_route_facts`) --
/// so this exists to keep the per-blob round trip complete and reviewable.
fn read_rust_module_route_facts(
    conn: &Connection,
    oid: &str,
    lang: &str,
) -> Result<RustModuleRouteFacts> {
    let mut facts = RustModuleRouteFacts::default();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT parent_ordinal, module_name, path_attribute, imports_macros,
                    body_start, body_end
             FROM rust_module_scopes
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| {
            decode_rust_module_scope_row(row, 0)
        })?;
        for row in rows {
            facts.scopes.push(row??);
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT scope_ordinal, module_name, path_attribute, visibility, imports_macros,
                    test_gated, declaration_start, declaration_end
             FROM rust_module_routes
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| {
            decode_rust_module_route_row(row, 0)
        })?;
        for row in rows {
            facts.routes.push(row??);
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT route_ordinal, macro_name, invocation_start
             FROM rust_module_route_gates
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY route_ordinal, gate_ordinal",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| {
            decode_rust_module_route_gate_row(row, 0)
        })?;
        for row in rows {
            let (route_ordinal, gate) = row??;
            attach_rust_module_route_gate(&mut facts.routes, route_ordinal, gate)?;
        }
    }
    {
        let mut stmt = conn.prepare_cached(
            "SELECT macro_name, visible_after, scope_start, scope_end, passthrough
             FROM rust_item_macros
             WHERE blob_oid = ?1 AND lang = ?2 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![oid, lang], |row| decode_rust_item_macro_row(row, 0))?;
        for row in rows {
            facts.item_macros.push(row??);
        }
    }
    Ok(facts)
}

/// `base` is the index of this row shape's first column, so the per-blob reads
/// (which select the columns alone) and the batched reads (which select
/// `blob_oid` first) share one decoder.
fn decode_rust_module_scope_row(
    row: &rusqlite::Row<'_>,
    base: usize,
) -> rusqlite::Result<Result<RustModuleScopeFact>> {
    let parent = row.get::<_, Option<i64>>(base)?;
    let module_name = row.get::<_, String>(base + 1)?;
    let path_attribute = row.get::<_, Option<String>>(base + 2)?;
    let imports_macros = row.get::<_, i64>(base + 3)? != 0;
    let body_start = row.get::<_, i64>(base + 4)?;
    let body_end = row.get::<_, i64>(base + 5)?;
    Ok((|| {
        Ok(RustModuleScopeFact {
            parent: parent.map(i64_to_usize).transpose()?,
            module_name,
            path_attribute,
            imports_macros,
            body_start: i64_to_usize(body_start)?,
            body_end: i64_to_usize(body_end)?,
        })
    })())
}

fn decode_rust_module_route_row(
    row: &rusqlite::Row<'_>,
    base: usize,
) -> rusqlite::Result<Result<RustModuleRouteFact>> {
    let scope = row.get::<_, i64>(base)?;
    let module_name = row.get::<_, String>(base + 1)?;
    let path_attribute = row.get::<_, Option<String>>(base + 2)?;
    let visibility = row.get::<_, String>(base + 3)?;
    let imports_macros = row.get::<_, i64>(base + 4)? != 0;
    let test_gated = row.get::<_, i64>(base + 5)? != 0;
    let declaration_start = row.get::<_, i64>(base + 6)?;
    let declaration_end = row.get::<_, i64>(base + 7)?;
    Ok((|| {
        Ok(RustModuleRouteFact {
            scope: i64_to_usize(scope)?,
            module_name,
            path_attribute,
            visibility: decode_rust_visibility(&visibility)
                .unwrap_or_else(|| panic!("unknown persisted Rust visibility: {visibility}")),
            imports_macros,
            test_gated,
            declaration_start: i64_to_usize(declaration_start)?,
            declaration_end: i64_to_usize(declaration_end)?,
            gates: Vec::new(),
        })
    })())
}

fn decode_rust_module_route_gate_row(
    row: &rusqlite::Row<'_>,
    base: usize,
) -> rusqlite::Result<Result<(usize, RustMacroGateFact)>> {
    let route_ordinal = row.get::<_, i64>(base)?;
    let macro_name = row.get::<_, String>(base + 1)?;
    let invocation_start = row.get::<_, i64>(base + 2)?;
    Ok((|| {
        Ok((
            i64_to_usize(route_ordinal)?,
            RustMacroGateFact {
                macro_name,
                invocation_start: i64_to_usize(invocation_start)?,
            },
        ))
    })())
}

fn decode_rust_item_macro_row(
    row: &rusqlite::Row<'_>,
    base: usize,
) -> rusqlite::Result<Result<RustRulesItemMacroDefinition>> {
    let name = row.get::<_, String>(base)?;
    let visible_after = row.get::<_, i64>(base + 1)?;
    let scope_start = row.get::<_, i64>(base + 2)?;
    let scope_end = row.get::<_, i64>(base + 3)?;
    let passthrough = row.get::<_, i64>(base + 4)? != 0;
    Ok((|| {
        Ok(RustRulesItemMacroDefinition {
            name,
            visible_after: i64_to_usize(visible_after)?,
            scope_start: i64_to_usize(scope_start)?,
            scope_end: i64_to_usize(scope_end)?,
            passthrough,
        })
    })())
}

/// Attach one gate row to the route it belongs to.
///
/// Gate rows are read in `(route_ordinal, gate_ordinal)` order, so appending
/// preserves the outermost-first order the reader relies on. A gate naming a
/// route that does not exist can only come from rows this build did not write.
fn attach_rust_module_route_gate(
    routes: &mut [RustModuleRouteFact],
    route_ordinal: usize,
    gate: RustMacroGateFact,
) -> Result<()> {
    let route = routes.get_mut(route_ordinal).ok_or_else(|| {
        StoreError::new(format!(
            "module route gate names missing route {route_ordinal}: {gate:?}"
        ))
    })?;
    route.gates.push(gate);
    Ok(())
}
