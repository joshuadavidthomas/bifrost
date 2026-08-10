use super::*;
use std::mem::size_of;
use std::sync::Arc;

pub(super) fn weight_code_unit_set_by_file(
    _key: &ProjectFile,
    value: &Arc<HashSet<CodeUnit>>,
) -> u32 {
    let size = value.iter().fold(0usize, |acc, item| {
        acc + size_of::<CodeUnit>()
            + item.fq_name().len()
            + item.short_name().len()
            + item.package_name().len()
            + item.signature().map_or(0, str::len)
    });
    size.saturating_add(size_of::<HashSet<CodeUnit>>()) as u32
}

pub(super) fn weight_code_unit_vec_by_file(_key: &ProjectFile, value: &Arc<Vec<CodeUnit>>) -> u32 {
    let size = value.iter().fold(size_of::<Vec<CodeUnit>>(), |acc, item| {
        let item_size = size_of::<CodeUnit>()
            .saturating_add(item.fq_name().len())
            .saturating_add(item.short_name().len())
            .saturating_add(item.package_name().len())
            .saturating_add(item.signature().map_or(0, str::len));
        acc.saturating_add(item_size)
    });
    size.min(u32::MAX as usize) as u32
}

pub(super) fn weight_project_file_set(
    _key: &ProjectFile,
    value: &Arc<HashSet<ProjectFile>>,
) -> u32 {
    let size = value.iter().fold(0usize, |acc, item| {
        acc + size_of::<ProjectFile>()
            + item.root().as_os_str().len()
            + item.rel_path().as_os_str().len()
    });
    size.saturating_add(size_of::<HashSet<ProjectFile>>()) as u32
}

pub(super) fn weight_include_reachability(
    key: &(ProjectFile, ProjectFile, bool),
    _value: &bool,
) -> u32 {
    let (first, donor, _) = key;
    let size = size_of::<(ProjectFile, ProjectFile, bool)>()
        .saturating_add(first.root().as_os_str().len())
        .saturating_add(first.rel_path().as_os_str().len())
        .saturating_add(donor.root().as_os_str().len())
        .saturating_add(donor.rel_path().as_os_str().len());
    size.clamp(1, u32::MAX as usize) as u32
}

fn weight_code_unit(unit: &CodeUnit) -> usize {
    size_of::<CodeUnit>()
        .saturating_add(unit.fq_name().len())
        .saturating_add(unit.short_name().len())
        .saturating_add(unit.package_name().len())
        .saturating_add(unit.signature().map_or(0, str::len))
}

/// #1908: one entry is every callable declaration sharing a member identifier,
/// bucketed by owner terminal, so a hot identifier on a whale repo is thousands
/// of units. Weighed by content like its siblings rather than counted, so the
/// cell cannot hold an unbounded amount of the workspace.
// `&String`, not `&str`: moka's weigher is `Fn(&K, &V)` and the cell is keyed
// by `String`.
#[allow(clippy::ptr_arg)]
pub(super) fn weight_reconcile_candidates(
    key: &String,
    value: &Arc<CppReconcileCandidates>,
) -> u32 {
    // Bucketed units are clones of the same `CodeUnit`s, so charge each
    // candidate's content once and every bucket entry at the unit's own size.
    let size = value
        .iter()
        .fold(size_of::<CppReconcileCandidates>(), |acc, item| {
            acc.saturating_add(weight_code_unit(item))
        })
        .saturating_add(value.bucketed_len().saturating_mul(size_of::<CodeUnit>()))
        .saturating_add(key.len());
    size.clamp(1, u32::MAX as usize) as u32
}

/// #1908: one entry is every re-keyed definition one (identifier, owner
/// terminal) group produces, grouped under its canonical fq name.
pub(super) fn weight_reconciled_groups(
    key: &CppReconcileGroupKey,
    value: &Arc<HashMap<String, Arc<CppReconciledDefinitionIndex>>>,
) -> u32 {
    let size = value
        .iter()
        .fold(size_of::<HashMap<String, ()>>(), |acc, (fq_name, index)| {
            let entry = index.rekeyed.iter().fold(0usize, |units, unit| {
                // `provisional_of` holds one further unit per re-keyed unit.
                units.saturating_add(weight_code_unit(unit).saturating_mul(2))
            });
            acc.saturating_add(fq_name.len()).saturating_add(entry)
        })
        .saturating_add(key.member_identifier.len())
        .saturating_add(key.owner_terminal.as_ref().map_or(0, String::len));
    size.clamp(1, u32::MAX as usize) as u32
}
