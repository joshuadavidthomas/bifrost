# Select the default Muninn model from accelerator availability

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must stay current.

Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Bifrost semantic search will have no named embedding profiles. When neither model override is set, Bifrost will use `brokkai/Muninn` with a CUDA or Metal accelerator. It will use `brokkai/Muninn-small` without one. Full Muninn output will use its supported 512-dimensional Matryoshka truncation. `BIFROST_EMBED_MODEL_ID` and `BIFROST_EMBED_MODEL_DIR` will continue to select an explicit model.

Users can verify the behavior through focused Rust tests. These tests will show the GPU and CPU default choices. Sidecar tests will show that model metadata controls dimensions, prompts, and pooling without a profile name.

## Progress

- [x] (2026-08-10 21:00Z) Read the current model resolution, sidecar protocol, tests, and operator documentation.
- [x] (2026-08-10 21:05Z) Verify the two Hugging Face model contracts and repository files.
- [x] (2026-08-10 21:25Z) Replace the Rust profile table with accelerator-based model resolution and a dynamic sidecar contract.
- [x] (2026-08-10 21:35Z) Replace Python profile selection with model metadata loading.
- [x] (2026-08-10 21:45Z) Update tests, documentation, package comments, and notices.
- [x] (2026-08-10 22:00Z) Run formatting, Python lint, 56 NLP crate tests, and the full `nlp` feature compile check. The policy tool was not installed.
- [x] (2026-08-10 22:10Z) Prepare the exact changed-file set for a multiline checkpoint commit.

## Surprises & Discoveries

- Observation: The current Python sidecar ignores `BIFROST_EMBED_MODEL_ID` and selects its model from `BIFROST_EMBED_PROFILE`.
  Evidence: `scripts/voyage_sidecar.py` sets `MODEL_ID` from `PROFILES` and only checks `BIFROST_EMBED_MODEL_DIR` for an override.
- Observation: Muninn and Muninn-small require different output handling.
  Evidence: Muninn declares 2,048-dimensional mean pooling. Muninn-small declares 384-dimensional CLS pooling.
- Observation: Both requested repositories contain Sentence Transformers metadata files.
  Evidence: Each repository contains `config_sentence_transformers.json` and `1_Pooling/config.json`.
- Observation: The installed tool set does not include the `bifrost-policy-checking` skill or its `run_policy` operation.
  Evidence: The available tool catalog contains no policy or skill operation.

## Decision Log

- Decision: Read serving dimensions, prompts, and pooling from model metadata in the sidecar.
  Rationale: A second profile table would keep the mechanism that the user asked to remove. Model metadata is the authoritative contract for explicit model overrides.
  Date/Author: 2026-08-10 / Codex
- Decision: Let the sidecar ready frame give Rust the output dimension.
  Rationale: Rust cannot know the dimension of an explicit model ID or local model before the model metadata loads.
  Date/Author: 2026-08-10 / Codex
- Decision: Apply `BIFROST_EMBED_MODEL_ID` before automatic default selection, and apply `BIFROST_EMBED_MODEL_DIR` only as the model source.
  Rationale: This preserves both overrides. It also keeps a stable repository label when a local directory supplies model files.
  Date/Author: 2026-08-10 / Codex
- Decision: Truncate the bidirectional Qwen Muninn model to 512 dimensions.
  Rationale: The user requested the 512-dimensional Muninn mode. Architecture detection also applies the choice to a local Muninn directory.
  Date/Author: 2026-08-10 / Codex

## Outcomes & Retrospective

The implementation removes named profiles from Rust and Python. Automatic selection now uses full Muninn with an accelerator and Muninn-small without one. Full Muninn output truncates to 512 dimensions. Both model override variables remain active. The focused test suite passed 56 tests, and the full `nlp` feature compiled. The real-model smoke test remains intentionally unrun because it downloads model weights.

## Context and Orientation

`crates/bifrost-nlp/src/engine.rs` defines embedding selection and the `Embedder` interface. It currently contains three static `ModelProfile` values. `crates/bifrost-nlp/src/voyage_sidecar.rs` starts Python workers and checks their ready messages against the selected profile. `scripts/voyage_sidecar.py` repeats the same profile table and performs the model forward pass.

The sidecar sends a ready message before it accepts embedding requests. This change will make that message authoritative for the vector dimension. The Rust client will use the reported dimension to decode later binary vector responses.

## Plan of Work

First, remove `BIFROST_EMBED_PROFILE`, `ModelProfile`, and all named profile constants from `engine.rs`. Add constants for the full and small Muninn repository IDs. Select the full model when `accelerator_available()` is true. Select the small model otherwise. Preserve `BIFROST_EMBED_MODEL_ID` as the first choice.

Next, change `voyage_sidecar.rs` so spawned and remote sidecars report their dimension. Remove profile values from process arguments and validation. Retain cache invalidation through the model fingerprint, dimension, document contract, and vector-storage contract.

Then, change `scripts/voyage_sidecar.py`. Select `BIFROST_EMBED_MODEL_DIR` first as the load source. Otherwise, select `BIFROST_EMBED_MODEL_ID`. Read Sentence Transformers prompt and pooling metadata from that source. Use mean pooling for Muninn and CLS pooling for Muninn-small. Keep the optimized Qwen SDPA path for the full model.

Finally, update tests and all user-visible text that names Voyage as the default. The third-party notice will name both Muninn repositories and their Apache-2.0 license.

## Concrete Steps

Run all commands from `/home/jonathan/Projects/bifrost`.

Apply edits with `apply_patch`. Then run:

    cargo fmt
    cargo test -p brokk-bifrost-nlp
    cargo check --features nlp
    ruff check scripts/voyage_sidecar.py

Do not run a real-model test unless explicitly authorized. It downloads model weights. Do not put build output in `/tmp`.

Run the installed repository policy checker once before edits and once after edits when its `run_policy` tool is available. Run `bifrost.code-smells` with every executable repository policy root that this repository identifies.

## Validation and Acceptance

Focused tests must prove that an explicit `BIFROST_EMBED_MODEL_ID` wins. They must prove that CPU selection returns `brokkai/Muninn-small`. A deterministic accelerator probe test must prove that a GPU selection returns `brokkai/Muninn` without depending on the test host hardware.

Sidecar protocol tests must accept dimensions reported by the ready message. Existing matrix decoding, timeout, and process cleanup tests must continue to pass.

The codebase must contain no `BIFROST_EMBED_PROFILE` or named profile selection. Product documentation must describe automatic accelerator selection and both explicit overrides.

## Idempotence and Recovery

The edits and validation commands are safe to repeat. Cargo uses the repository build location. Do not set a temporary Cargo target directory. Existing unrelated working-tree files must remain unchanged and uncommitted.

## Artifacts and Notes

The verified remote contracts are:

    brokkai/Muninn: 2048 native dimensions, truncated to 512, mean pooling, 8192 served tokens
    brokkai/Muninn-small: 384 dimensions, CLS pooling, 8192 tokens

Both model cards specify separate query and document prompts. The sidecar must read these values from `config_sentence_transformers.json`.

## Interfaces and Dependencies

`engine::embed_repo_id()` will return the explicit model ID or an automatic Muninn default. The function will not read a profile variable.

`Embedder::dim()` will directly report the vector dimension. Production sidecars will get this value from the ready frame. Test embedders will keep their configured deterministic dimension.

The Python sidecar will use existing PyTorch and Transformers dependencies. It will not add Sentence Transformers as a run-time dependency.

Revision note: Created this plan after repository and Hugging Face contract inspection. The plan records the removal of profile-driven model selection and the required dynamic sidecar contract.

Revision note: Added the user's requirement to serve full Muninn with 512-dimensional Matryoshka truncation.

Revision note: Recorded implementation completion, validation results, the unavailable policy tool, and the intentionally skipped model download.
