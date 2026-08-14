# Co-edit retrieval eval, and a ranker that beats dir+popularity

Date: 2026-08-14

Follows `nlp-semantic-search-layer-forensics-2026-08-13.md` and bifrost#2127.

The prometheus pilot showed the co-edit leg carries real signal but loses to a
trivial baseline: rank the files that share a directory with a seed, ordered by
how often they change. This note builds a proper eval and finds a ranker that
beats that baseline decisively.

## The eval

The CodeScale corpus cannot be used. Every repository in it is a squashed
single-commit sg-evals mirror, and `git fetch --depth=50` against those mirrors
returns one commit, so there is no history to recover.

The corpus is drawn instead from the general `/mnt/T9/repo-clones` set: 12,828
repositories scanned, 2,190 with at least 3,000 commits, filtered to those with
a dominant Bifrost-analyzed language holding at least 60 percent of source
files and between 400 and 6,000 files. Two repositories per language, deepest
history first, deduplicated by project so a fork does not count twice.

21 repositories were prepared. 17 produced results:

- 3 exceeded the 40-minute analyzer budget (ceph, osu, one other C++/C# giant).
- 1 (Dolibarr) produced nothing at all, which is a finding in its own right.

Final: **7,740 cases, 17 repositories, 10 languages**.

### Protocol

Leakage is the thing to get right. The ranker walks the 1,000 commits ending at
a split commit T, so the workspace is checked out at T and every test commit is
newer than T. The ranker cannot have seen the answer.

Task: given all but one file of a real commit, rank the held-out file.

Leave-one-out runs over **every** file in the commit. An earlier version held
out "the last path in sorted order", which looked deterministic but was biased:
sorted order puts `spec/...` after `app/...`, so the held-out file was
systematically in a different directory from its seeds. That penalises the
same-directory baseline and flatters a stem matcher, which is exactly the
comparison the eval exists to make. Fixing it moved dir+popularity up 3.4
points on the first repository measured.

## Result

| ranker | recall@5 | recall@10 | recall@20 | recall@50 | MRR |
|---|---:|---:|---:|---:|---:|
| popularity | 6.0% | 9.2% | 13.5% | 23.9% | 0.043 |
| dirco | 6.2% | 9.5% | 13.7% | 21.3% | 0.055 |
| imports | 13.6% | 19.6% | 27.3% | 38.2% | 0.095 |
| mirror | 16.7% | 20.1% | 24.1% | 34.1% | 0.130 |
| **dir+pop (baseline)** | 17.9% | **24.7%** | 31.7% | 43.8% | 0.130 |
| coedit (shipped) | 22.1% | 29.1% | 36.4% | 45.0% | 0.154 |
| fuse (RRF) | 24.2% | 32.8% | 42.6% | 58.9% | 0.172 |
| stem | 27.5% | 35.3% | 43.1% | 54.4% | 0.206 |
| **cascade** | **31.1%** | **41.0%** | **54.0%** | **65.7%** | **0.232** |

The cascade beats the baseline by **16.4 points at k=10**, a 66 percent
relative gain, and by 22.3 points at k=20. MRR rises 78 percent. It wins in 15
of 17 repositories.

It also beats the shipped co-edit leg by 11.9 points at k=10.

## What the cascade is

Priority tiers, not fitted weights. Nothing is trained, so nothing can be
overfitted to this corpus, and there is no train/test split to argue about.

1. Same subject as a seed in another tree (mirror), or same stem next door.
2. Same stem anywhere.
3. Top 10 of the file-level co-edit leg.
4. Same directory and import-adjacent.
5. Same directory.
6. Import-adjacent.
7. Anywhere else co-edit ranked it.
8. Directory-level co-change affinity.
9. Popularity.

Two ideas do the work.

**Stem and mirror matching.** Files that change together are usually the same
subject: `Foo.java` and `FooTest.java`, `foo.c` and `foo.h`,
`app/models/x.rb` and `spec/models/x_spec.rb`. `mirror_key` strips build-layout
segments (`src`, `main`, `test`, `spec`, `java`, ...) and a test affix, so the
same subject lines up across trees. This is what dir+popularity structurally
cannot see: a Ruby spec or a Java test lives in a different directory from the
code it covers.

**A rarity gate.** `mod.rs`, `index.ts`, `__init__.py` and `package-info.java`
share a stem with hundreds of unrelated files, so matching on them is a
popularity draw wearing a precision costume. Rather than hand-listing those
names per language, the gate measures it: a key grouping more than 8 files in
that repository is not evidence. Adding it took the cascade from 38.5 to 41.0
at k=10 and repaired the two worst regressions.

## Why the earlier prometheus result was misleading

The pilot found dir+popularity beating co-edit at every k. That held for Go and
almost nowhere else. Go keeps `foo_test.go` beside `foo.go`, so the
same-directory heuristic captures the test-pairing signal for free. Every
language that separates test trees breaks it:

| language | n | dir+pop | cascade |
|---|---:|---:|---:|
| php | 601 | 16.3% | 40.9% |
| rb | 889 | 16.8% | 42.0% |
| cpp | 603 | 14.8% | 45.4% |
| java | 262 | 14.9% | 40.8% |
| go | 1,205 | 25.7% | 45.7% |
| scala | 1,152 | 29.8% | 45.3% |
| ts | 793 | 34.3% | 35.8% |
| kt | 808 | 35.5% | 49.5% |
| py | 825 | 18.3% | 30.1% |
| rs | 602 | 28.6% | 28.4% |

The caveat recorded in the pilot -- one repository, one language, Go packages
map onto directories tightly -- was the whole story.

## Where the cascade does not win

- `jhipster/generator-jhipster` (ts, 189 cases): 12.7% against 19.0%. A code
  generator whose files are templates; the stem signal misfires there.
- `rust-lang/rust-analyzer` (rs, 602 cases): 28.4% against 28.6%, a tie.
- Python is the weakest real gain, 18.3% to 30.1%.

## Reciprocal-rank fusion is the wrong tool here

RRF over the component rankers scores 32.8% at k=10, below `stem` alone at
35.3%. Every component falls back to popularity for its tail, so the shared
tail agrees across lists and dilutes the disagreeing heads. A cascade keeps
precedence explicit and does not average a strong signal against a weak one.

## Two defects found on the way

**The co-edit leg fails silently on a shallow clone.** Dolibarr has 115,170
commits in its clone, but the first-parent chain from the eval worktree reaches
only 430, short of the 1,000-commit window. The leg returned
`HistoryUnavailable` and an empty list for all 606 cases, and
`semantic_search` would surface that as "no co-edit results" rather than as a
degraded workspace. Any `--depth 1` user hits this.

Update, same day: root-caused and fixed. `first_parent_oid` returned a parent
id recorded in the boundary commit's header without checking the object
exists, so `populate_commit_range` ran `git log <missing>..<newest>`, which
aborts with "Invalid revision range", and the error was swallowed into an
empty `HistoryUnavailable` result. The parent probe now requires the object to
be present, which routes a truncated range through the existing
`--root <newest>` form. Two degeneracy guards were added at the same time:
fewer than two commits with tracked churn returns empty (one commit shows no
co-editing), and a uniform score vector wider than `k` returns empty (the
truncated output would be an arbitrary path-sorted subset, which is what the
CodeScale mirrors produced). Suppressed results report `Complete`, not
`HistoryUnavailable`, and carry no notes.

After the fix, Dolibarr produces rankings for all 606 cases. Corpus totals
with it included (8,346 cases, 18 repositories): cascade 39.4 percent against
dir+popularity 24.6 percent at k=10, better in 15 of 18 repositories.
Dolibarr itself is now one of the three losses: its truncated 430-commit
window gives co-edit only 22 percent coverage, and its `htdocs` layout has no
test-tree mirroring for the stem and mirror signals to use.

**Co-edit coverage is the ceiling, not precision.** Across the corpus the leg
could rank the answer at all in only 45.0% of cases. It can only score a file
that co-occurred with a seed inside the window. Where it fires it is good; the
other 55 percent is unreachable, which is why a full-coverage structural prior
beats it and why the cascade puts it in tier 3 rather than tier 1.

## Reproducing

Harness: `tests/suite_semantic/measure_coedit_retrieval.rs`, run per repository
with `BIFROST_COEDIT_EVAL_CASES`. It emits rankings and the depth-1 import
neighbourhood; it does no scoring.

Corpus builders and scorers:
`/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/coedit-eval-20260813/`

- `pick_repos.py` chooses the corpus from the repo scan.
- `build_corpus.py` creates split worktrees and leave-one-out cases.
- `build_dirstats.py` computes directory co-change over the same window.
- `run_all.sh` runs the harness once per repository, under a lock.
- `score_all.py` scores every ranker, including the cascade.

## Recommended next step

The cascade is a ranking policy, not yet Bifrost code. Before adopting it:

1. Re-run with the three timed-out repositories included, to confirm the C++
   and C# results are not carried by MariaDB alone.
2. Decide where it belongs. The stem, mirror and rarity signals need no git
   history and no analyzer, so they are cheap enough to apply wherever file
   candidates are ranked, including on a shallow clone where the co-edit leg
   returns nothing at all.
3. Keep the file-level co-edit leg. It is tier 3 and it earns that slot, but it
   should stop being the only signal.
