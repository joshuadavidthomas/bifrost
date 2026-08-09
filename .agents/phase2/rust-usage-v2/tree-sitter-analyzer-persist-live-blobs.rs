// Parked verbatim by Phase 1 of `.agents/plans/port-optimization-arc-to-upstream.md`.
//
// `TreeSitterAnalyzer::persist_live_blobs`, the off-request writer the usage-v2
// Milestone 3 fact catch-up used to repair live blobs whose fact rows analysis
// had not written. Nothing writes fact rows in Phase 1, so it has no caller.
//
// This file is not a Cargo module and is never compiled.

    /// Re-parse `files` and persist their blobs into the store, without
    /// producing a new analyzer generation.
    ///
    /// This is the catch-up half of ExecPlan Milestone 3
    /// (`.agents/plans/rust-usage-index-v2.md`): a store-backed query answers
    /// from a blob's rows, so a live file whose blob was never persisted -- a
    /// write that failed and left a dirty in-memory state, say -- is invisible
    /// to it. Running the whole reconcile to repair a handful of files would
    /// build a new analyzer and drop every memo on it, which is precisely what
    /// this plan exists to stop.
    ///
    /// A file whose current bytes no longer hash to its live oid is skipped:
    /// persisting it would file the new content's rows under the old blob, and
    /// blob rows are content-addressed and shared, so a row that lies is worse
    /// than a row that is missing. The next `update` reconciles those files
    /// with their real oid anyway.
    pub(crate) fn persist_live_blobs(&self, files: &[ProjectFile]) {
        let mut targets = Vec::with_capacity(files.len());
        // One target per blob key, not per file: byte-identical files share a
        // blob, and persisting the same key twice in one batch is a hard error
        // in the persistence layer. Reconcile picks a representative for the
        // same reason (`representative_by_blob_key`).
        let mut claimed = HashSet::default();
        for file in files {
            let Some(oid) = self.live_snapshot().oid_for_path(file) else {
                continue;
            };
            let Ok(source) = self.project.read_source(file) else {
                continue;
            };
            if !CodeUnitIndex::indexed_source_matches(self, file, &source) {
                continue;
            }
            let storage_key = self.adapter.storage_language_key_for_file(file);
            let Some(generation) = self.store_context.generations.get(&storage_key).copied() else {
                continue;
            };
            if !claimed.insert((oid, storage_key.clone())) {
                continue;
            }
            targets.push((file.clone(), oid, storage_key, generation));
        }
        if targets.is_empty() {
            return;
        }
        Self::analyze_prepare_and_persist_files(
            self.adapter.as_ref(),
            self.project.as_ref(),
            &self.config,
            targets,
            None,
            &self.store_context,
            |_, _| {},
        );
    }
