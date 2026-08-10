//! Git plumbing for the semantic content cache.
//!
//! The analyzer cache identifies the exact bytes that tree-sitter parsed. The
//! semantic cache has a different contract: clean tracked files use the Git
//! index OID without a content read. Dirty and untracked files use the OID of
//! their working bytes. Paths with a content-changing Git attribute also use
//! their working bytes, even when Git reports them as clean.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use git2::{
    AttrCheckFlags, AttrValue, Config, DiffOptions, ErrorCode, Index, ObjectType, Oid, Repository,
};
use growable_bloom_filter::GrowableBloom;

type Result<T> = std::result::Result<T, String>;

pub fn discover(root: &Path) -> Option<Repository> {
    brokk_bifrost_analysis::gitblob::discover(root)
}

pub fn is_git_repo(root: &Path) -> bool {
    brokk_bifrost_analysis::gitblob::is_git_repo(root)
}

pub fn working_tree_oids(
    repo: &Repository,
    rel_paths: &[String],
) -> Result<HashMap<String, String>> {
    let started = Instant::now();
    let workdir = repo
        .workdir()
        .ok_or_else(|| "repository has no working directory".to_string())?;
    let mut index = repo.index().map_err(|err| err.to_string())?;
    // Bifrost keeps this repository open while another process can run Git.
    // Reload the index so newly staged content gets its current index OID.
    index.read(true).map_err(|err| err.to_string())?;
    let dirty = dirty_worktree_paths(repo, &index, None)?;
    let index_oids: HashMap<String, Oid> = index
        .iter()
        .map(|entry| {
            let path = String::from_utf8(entry.path).map_err(|err| {
                format!("non-UTF-8 git index path while building semantic cache: {err}")
            })?;
            Ok((path, entry.id))
        })
        .collect::<Result<_>>()?;
    let mut transforms = ContentTransformProbe::new(repo, &index, workdir);
    let mut out = HashMap::with_capacity(rel_paths.len());
    let mut hashed = 0usize;
    for rel in rel_paths {
        let use_worktree = dirty.contains(rel)
            || !index_oids.contains_key(rel)
            || transforms.applies_to(Path::new(rel))?;
        let oid = if use_worktree {
            hashed += 1;
            hash_working_file(workdir, rel)?
        } else {
            *index_oids
                .get(rel)
                .expect("tracked clean semantic path has an index OID")
        };
        out.insert(rel.clone(), oid.to_string());
    }
    eprintln!(
        "bifrost semantic identities: files={}; index={}; hashed={hashed}; attr_lookups={}; time={:?}",
        rel_paths.len(),
        rel_paths.len() - hashed,
        transforms.lookups,
        started.elapsed()
    );
    Ok(out)
}

pub fn working_tree_oids_targeted(
    repo: &Repository,
    rel_paths: &[String],
) -> Result<HashMap<String, String>> {
    let started = Instant::now();
    let workdir = repo
        .workdir()
        .ok_or_else(|| "repository has no working directory".to_string())?;
    let mut index = repo.index().map_err(|err| err.to_string())?;
    // Watcher updates also run after external Git commands on a long-lived repo.
    index.read(true).map_err(|err| err.to_string())?;
    let dirty = dirty_worktree_paths(repo, &index, Some(rel_paths))?;
    let mut transforms = ContentTransformProbe::new(repo, &index, workdir);
    let mut out = HashMap::with_capacity(rel_paths.len());
    let mut hashed = 0usize;
    for rel in rel_paths {
        let path = Path::new(rel);
        let entry = index.get_path(path, 0);
        let use_worktree = dirty.contains(rel) || entry.is_none() || transforms.applies_to(path)?;
        let oid = if use_worktree {
            hashed += 1;
            hash_working_file(workdir, rel)?
        } else {
            entry
                .expect("tracked clean semantic path has an index OID")
                .id
        };
        out.insert(rel.clone(), oid.to_string());
    }
    eprintln!(
        "bifrost semantic watcher identities: files={}; index={}; hashed={hashed}; attr_lookups={}; time={:?}",
        rel_paths.len(),
        rel_paths.len() - hashed,
        transforms.lookups,
        started.elapsed()
    );
    Ok(out)
}

fn dirty_worktree_paths(
    repo: &Repository,
    index: &Index,
    rel_paths: Option<&[String]>,
) -> Result<HashSet<String>> {
    // A targeted request with no paths asks about nothing, so nothing can be
    // dirty. libgit2 treats a diff with no pathspec as match-all, so building
    // the diff anyway walks the whole worktree to answer that empty question.
    if rel_paths.is_some_and(|paths| paths.is_empty()) {
        return Ok(HashSet::new());
    }

    let mut options = DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_typechange(true)
        .ignore_submodules(true)
        .skip_binary_check(true);
    if let Some(paths) = rel_paths {
        options.disable_pathspec_match(true);
        for path in paths {
            options.pathspec(path);
        }
    }

    let diff = repo
        .diff_index_to_workdir(Some(index), Some(&mut options))
        .map_err(|err| err.to_string())?;
    let mut dirty = HashSet::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.old_file().path() {
            dirty.insert(path.to_string_lossy().into_owned());
        }
        if let Some(path) = delta.new_file().path() {
            dirty.insert(path.to_string_lossy().into_owned());
        }
    }
    Ok(dirty)
}

/// Answers whether Git can show worktree bytes that differ from the index blob
/// for a path, which is true when `filter`, `ident` or `working-tree-encoding`
/// is set on it.
///
/// libgit2 answers per path and re-reads every attribute source on each call,
/// so a whole-worktree walk pays three lookups for each file: 512,667 of them
/// on Firefox (issue #1904). Git reads those attributes from a fixed set of
/// sources: the system attributes file, `core.attributesFile` or its XDG
/// default, `$GIT_DIR/info/attributes`, and a `.gitattributes` file in each
/// directory from the worktree root down to the path, taken from the worktree
/// or, when it is absent there, from the index. No source above a path means no
/// rule and no macro can match it, so the lookups are skipped. libgit2 stays
/// the authority for every path that does have a source above it; this type
/// only decides whether to ask, and it never reads a rule itself.
///
/// The cached verdicts describe files on disk and in the index, so an instance
/// is valid for one identity walk.
struct ContentTransformProbe<'a> {
    repo: &'a Repository,
    index: &'a Index,
    workdir: &'a Path,
    /// A source outside the directory chain exists, so every path needs libgit2.
    repository_wide_rules: bool,
    /// `.gitattributes` presence for each directory already inspected, keyed by
    /// its worktree-relative path.
    directories: HashMap<PathBuf, bool>,
    /// Attribute lookups handed to libgit2. Reported in the walk's timing line
    /// and asserted by the tests that pin the skipped case.
    lookups: usize,
}

impl<'a> ContentTransformProbe<'a> {
    fn new(repo: &'a Repository, index: &'a Index, workdir: &'a Path) -> Self {
        Self {
            repo,
            index,
            workdir,
            repository_wide_rules: has_repository_wide_attribute_rules(repo),
            directories: HashMap::new(),
            lookups: 0,
        }
    }

    fn applies_to(&mut self, path: &Path) -> Result<bool> {
        if !self.has_rules_above(path) {
            return Ok(false);
        }
        for name in ["filter", "ident", "working-tree-encoding"] {
            self.lookups += 1;
            let value = self
                .repo
                .get_attr(path, name, AttrCheckFlags::FILE_THEN_INDEX)
                .map_err(|err| {
                    format!("reading Git attribute {name} for {}: {err}", path.display())
                })?;
            if !matches!(
                AttrValue::from_string(value),
                AttrValue::False | AttrValue::Unspecified
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn has_rules_above(&mut self, path: &Path) -> bool {
        if self.repository_wide_rules {
            return true;
        }
        let mut directory = path.parent();
        while let Some(current) = directory {
            if self.directory_has_attributes(current) {
                return true;
            }
            directory = current.parent();
        }
        false
    }

    fn directory_has_attributes(&mut self, directory: &Path) -> bool {
        if let Some(present) = self.directories.get(directory) {
            return *present;
        }
        // FILE_THEN_INDEX reads the worktree copy first and the index copy when
        // the worktree has none, so either presence makes the rules reachable.
        let candidate = directory.join(".gitattributes");
        let present =
            self.workdir.join(&candidate).is_file() || self.index.get_path(&candidate, 0).is_some();
        self.directories.insert(directory.to_path_buf(), present);
        present
    }
}

/// Whether an attribute file that can carry a rule or a macro for any path in
/// `repo` exists.
///
/// These are the sources outside the worktree's directory chain, in libgit2's
/// own order: the system file, `core.attributesFile` or its XDG default, and
/// `$GIT_DIR/info/attributes`. A source that cannot be resolved counts as
/// present, so the probe keeps its per-path behavior instead of skipping a rule
/// it could not rule out.
fn has_repository_wide_attribute_rules(repo: &Repository) -> bool {
    let mut files = vec![
        // A linked worktree keeps `info` in the common directory.
        repo.path().join("info").join("attributes"),
        repo.commondir().join("info").join("attributes"),
    ];
    // libgit2 finds the system attributes file beside the system config file.
    if let Ok(system_config) = Config::find_system()
        && let Some(directory) = system_config.parent()
    {
        files.push(directory.join("gitattributes"));
    }
    if cfg!(unix) {
        files.push(PathBuf::from("/etc/gitattributes"));
    }
    let Ok(config) = repo.config() else {
        return true;
    };
    match config.get_path("core.attributesFile") {
        Ok(configured) => files.push(configured),
        // libgit2 falls back to the XDG location when the key is unset.
        Err(error) if error.code() == ErrorCode::NotFound => files.extend(xdg_attributes_file()),
        Err(_) => return true,
    }
    files.iter().any(|file| file.exists())
}

fn xdg_attributes_file() -> Option<PathBuf> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })?;
    Some(config_home.join("git").join("attributes"))
}

fn hash_working_file(workdir: &Path, rel: &str) -> Result<Oid> {
    Oid::hash_file(ObjectType::Blob, workdir.join(rel)).map_err(|err| err.to_string())
}

pub fn read_blob(repo: &Repository, oid_hex: &str) -> Result<Vec<u8>> {
    brokk_bifrost_analysis::gitblob::read_blob(repo, oid_hex)
}

pub fn reachable_bloom(repo: &Repository) -> Result<GrowableBloom> {
    brokk_bifrost_analysis::gitblob::reachable_bloom(repo)
}

pub fn worktree_roots(repo: &Repository) -> Result<Vec<PathBuf>> {
    brokk_bifrost_analysis::gitblob::worktree_roots(repo)
}

pub fn uncommitted_oids(root: &Path) -> Result<HashSet<String>> {
    brokk_bifrost_analysis::gitblob::uncommitted_oids(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn run_git<const N: usize>(root: &Path, args: [&str; N]) {
        let status = Command::new("git")
            .current_dir(root)
            .args(["-c", "commit.gpgSign=false"])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> (tempfile::TempDir, Repository) {
        let temp = tempfile::tempdir().unwrap();
        run_git(temp.path(), ["init"]);
        run_git(temp.path(), ["config", "user.email", "test@example.com"]);
        run_git(temp.path(), ["config", "user.name", "Test"]);
        // Repository scope wins over the host's global and system config, so
        // the attribute-source checks see the same absent file everywhere.
        run_git(
            temp.path(),
            ["config", "core.attributesFile", "missing-attributes"],
        );
        std::fs::write(temp.path().join("tracked.rs"), "fn first() {}\n").unwrap();
        run_git(temp.path(), ["add", "tracked.rs"]);
        run_git(temp.path(), ["commit", "-m", "initial"]);
        let repo = Repository::open(temp.path()).unwrap();
        (temp, repo)
    }

    #[test]
    fn clean_and_staged_files_use_the_current_index_oid() {
        let (temp, repo) = init_repo();
        let path = "tracked.rs".to_string();
        let index_oid = repo
            .index()
            .unwrap()
            .get_path(Path::new(&path), 0)
            .unwrap()
            .id;
        assert_eq!(
            working_tree_oids(&repo, std::slice::from_ref(&path)).unwrap()[&path],
            index_oid.to_string()
        );

        std::fs::write(temp.path().join(&path), "fn staged() {}\n").unwrap();
        run_git(temp.path(), ["add", "tracked.rs"]);
        let mut index = repo.index().unwrap();
        index.read(true).unwrap();
        let staged_oid = index.get_path(Path::new(&path), 0).unwrap().id;
        assert_eq!(
            working_tree_oids_targeted(&repo, std::slice::from_ref(&path)).unwrap()[&path],
            staged_oid.to_string()
        );
    }

    #[test]
    fn dirty_and_untracked_files_use_working_byte_oids() {
        let (temp, repo) = init_repo();
        std::fs::write(temp.path().join("tracked.rs"), "fn dirty() {}\n").unwrap();
        std::fs::write(temp.path().join("new.rs"), "fn new_file() {}\n").unwrap();
        let paths = ["tracked.rs".to_string(), "new.rs".to_string()];
        let resolved = working_tree_oids(&repo, &paths).unwrap();

        for path in paths {
            assert_eq!(
                resolved[&path],
                Oid::hash_file(ObjectType::Blob, temp.path().join(path))
                    .unwrap()
                    .to_string()
            );
        }
    }

    #[test]
    fn an_empty_targeted_request_does_not_diff_the_worktree() {
        let (temp, repo) = init_repo();
        std::fs::write(temp.path().join("tracked.rs"), "fn dirty() {}\n").unwrap();
        let index = repo.index().unwrap();

        // The whole-worktree form finds the edit. The targeted form asked about
        // no path, so it must report no dirty path and must not walk the tree.
        assert!(
            dirty_worktree_paths(&repo, &index, None)
                .unwrap()
                .contains("tracked.rs")
        );
        assert!(
            dirty_worktree_paths(&repo, &index, Some(&[]))
                .unwrap()
                .is_empty()
        );
        assert!(working_tree_oids_targeted(&repo, &[]).unwrap().is_empty());
    }

    #[test]
    fn a_repository_without_attribute_files_asks_libgit2_nothing() {
        let (_temp, repo) = init_repo();
        let index = repo.index().unwrap();
        let mut transforms = ContentTransformProbe::new(&repo, &index, repo.workdir().unwrap());

        assert!(!transforms.applies_to(Path::new("tracked.rs")).unwrap());
        assert_eq!(transforms.lookups, 0);
    }

    #[test]
    fn only_paths_under_a_gitattributes_file_reach_libgit2() {
        let (temp, repo) = init_repo();
        std::fs::create_dir(temp.path().join("sub")).unwrap();
        std::fs::write(temp.path().join("sub/.gitattributes"), "*.rs filter=fake\n").unwrap();
        std::fs::write(temp.path().join("sub/nested.rs"), "fn nested() {}\n").unwrap();
        let index = repo.index().unwrap();
        let mut transforms = ContentTransformProbe::new(&repo, &index, repo.workdir().unwrap());

        assert!(transforms.applies_to(Path::new("sub/nested.rs")).unwrap());
        let under_rules = transforms.lookups;
        assert!(under_rules > 0);
        assert!(!transforms.applies_to(Path::new("tracked.rs")).unwrap());
        assert_eq!(transforms.lookups, under_rules);
    }

    #[test]
    fn a_gitattributes_file_only_in_the_index_still_reaches_libgit2() {
        let (temp, repo) = init_repo();
        std::fs::write(temp.path().join(".gitattributes"), "*.rs ident\n").unwrap();
        run_git(temp.path(), ["add", ".gitattributes"]);
        std::fs::remove_file(temp.path().join(".gitattributes")).unwrap();
        let mut index = repo.index().unwrap();
        index.read(true).unwrap();
        let mut transforms = ContentTransformProbe::new(&repo, &index, repo.workdir().unwrap());

        assert!(transforms.applies_to(Path::new("tracked.rs")).unwrap());
        assert!(transforms.lookups > 0);
    }

    #[test]
    fn ident_attributes_use_the_transformed_working_bytes() {
        let (temp, repo) = init_repo();
        std::fs::write(temp.path().join(".gitattributes"), "ident.txt ident\n").unwrap();
        std::fs::write(temp.path().join("ident.txt"), "$Id$\n").unwrap();
        run_git(temp.path(), ["add", ".gitattributes", "ident.txt"]);
        run_git(temp.path(), ["commit", "-m", "ident"]);
        std::fs::remove_file(temp.path().join("ident.txt")).unwrap();
        run_git(temp.path(), ["checkout", "--", "ident.txt"]);

        let path = "ident.txt".to_string();
        let index_oid = repo
            .index()
            .unwrap()
            .get_path(Path::new(&path), 0)
            .unwrap()
            .id;
        let working_oid = Oid::hash_file(ObjectType::Blob, temp.path().join(&path)).unwrap();
        assert_ne!(working_oid, index_oid);
        assert_eq!(
            working_tree_oids(&repo, std::slice::from_ref(&path)).unwrap()[&path],
            working_oid.to_string()
        );
    }
}
