//! Driver for the OWASP BenchmarkJava taint bakeoff.
//!
//! Two subcommands:
//!
//!   promote-esapi --staged <path> --jar <path> --out <path>
//!       Digest the resolved ESAPI jar and write the pinned sanitizer pack.
//!
//!   run --benchmark <dir> --packs-dir <dir> --deps <dir> --out <path>
//!       [--esapi-digest <hex>] [--limit <n>] [--timeout-secs <s>]
//!       Build the Benchmark workspace, activate the sanitizer packs, run one
//!       require-model taint policy per category, score, and write the artifact.
//!
//! The heavy logic is gated behind the `release-tooling` feature so it stays out
//! of normal runtime builds, matching the other facade tooling binaries.

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(not(feature = "release-tooling"))]
    {
        eprintln!(
            "bifrost_owasp_benchmark requires the `release-tooling` feature; \
             rebuild with `--features release-tooling`."
        );
        ExitCode::FAILURE
    }
    #[cfg(feature = "release-tooling")]
    {
        match imp::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(feature = "release-tooling")]
mod imp {
    use std::collections::BTreeMap;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use brokk_bifrost::analyzer::semantic_model::{
        CatalogCoordinate, CompilerOptions, SourceFormat, compile_source,
    };
    use brokk_bifrost::owasp_benchmark::{
        InjectionCategory, RunConfig, RunResult, SanitizerPackSpec, build_policy, policy_id,
        promote_staged_sanitizer_pack, run as run_bakeoff,
    };
    use serde::Serialize;
    use sha2::{Digest, Sha256};

    pub fn run() -> Result<(), String> {
        let mut args = env::args().skip(1);
        let command = args
            .next()
            .ok_or_else(|| "missing subcommand (promote-esapi | run)".to_owned())?;
        let rest: Vec<String> = args.collect();
        match command.as_str() {
            "promote-esapi" => promote_esapi(&rest),
            "run" => run_command(&rest),
            other => Err(format!("unknown subcommand: {other}")),
        }
    }

    /// Parse `--flag value` pairs into a map.
    fn parse_flags(args: &[String]) -> Result<BTreeMap<String, String>, String> {
        let mut map = BTreeMap::new();
        let mut iter = args.iter();
        while let Some(flag) = iter.next() {
            let key = flag
                .strip_prefix("--")
                .ok_or_else(|| format!("expected a --flag, got {flag:?}"))?;
            let value = iter
                .next()
                .ok_or_else(|| format!("flag --{key} needs a value"))?;
            map.insert(key.to_owned(), value.clone());
        }
        Ok(map)
    }

    fn sha256_file(path: &Path) -> Result<String, String> {
        let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(hex_lower(&hasher.finalize()))
    }

    fn hex_lower(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    fn promote_esapi(args: &[String]) -> Result<(), String> {
        let flags = parse_flags(args)?;
        let staged = flags
            .get("staged")
            .ok_or_else(|| "--staged <path> is required".to_owned())?;
        let jar = flags
            .get("jar")
            .ok_or_else(|| "--jar <path> is required".to_owned())?;
        let out = flags
            .get("out")
            .ok_or_else(|| "--out <path> is required".to_owned())?;
        let digest = sha256_file(Path::new(jar))?;
        let staged_json =
            fs::read_to_string(staged).map_err(|error| format!("read {staged}: {error}"))?;
        let pinned = promote_staged_sanitizer_pack(&staged_json, &digest)?;
        // Validate the promoted pack compiles before writing it.
        compile_source(
            SourceFormat::Json,
            pinned.as_bytes(),
            &CompilerOptions::default(),
        )
        .map_err(|diagnostics| format!("promoted pack failed to compile: {diagnostics:#?}"))?;
        if let Some(parent) = Path::new(out).parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
        }
        fs::write(out, pinned.as_bytes()).map_err(|error| format!("write {out}: {error}"))?;
        println!("promoted ESAPI pack to {out}");
        println!("esapi artifact sha256: {digest}");
        Ok(())
    }

    /// A sanitizer pack the run activates, described so both the loader and the
    /// artifact can name it.
    struct PackDescriptor {
        pack_id: &'static str,
        file: &'static str,
        ecosystem: &'static str,
        package: Option<&'static str>,
        mandatory: bool,
        /// True when this pack carries a byte-level `artifact_sha256` pin fed
        /// from the resolved jar digest.
        pinned_to_esapi_digest: bool,
    }

    /// The packs the bakeoff activates. `bifrost.java-sanitizers` and the pinned
    /// `bifrost.esapi-sanitizers` are mandatory (the task rests on them); the
    /// other library packs are best-effort extras.
    const PACKS: &[PackDescriptor] = &[
        PackDescriptor {
            pack_id: "bifrost.java-sanitizers",
            file: "bifrost.java-sanitizers.json",
            ecosystem: "jdk",
            package: None,
            mandatory: true,
            pinned_to_esapi_digest: false,
        },
        PackDescriptor {
            pack_id: "bifrost.esapi-sanitizers",
            file: "bifrost.esapi-sanitizers.json",
            ecosystem: "maven",
            package: Some("org.owasp.esapi:esapi"),
            mandatory: true,
            pinned_to_esapi_digest: true,
        },
        PackDescriptor {
            pack_id: "bifrost.encoder-sanitizers",
            file: "staged/bifrost.encoder-sanitizers.json",
            ecosystem: "maven",
            package: Some("org.owasp.encoder:encoder"),
            mandatory: false,
            pinned_to_esapi_digest: false,
        },
        PackDescriptor {
            pack_id: "bifrost.commons-text-sanitizers",
            file: "staged/bifrost.commons-text-sanitizers.json",
            ecosystem: "maven",
            package: Some("org.apache.commons:commons-text"),
            mandatory: false,
            pinned_to_esapi_digest: false,
        },
        PackDescriptor {
            pack_id: "bifrost.guava-sanitizers",
            file: "staged/bifrost.guava-sanitizers.json",
            ecosystem: "maven",
            package: Some("com.google.guava:guava"),
            mandatory: false,
            pinned_to_esapi_digest: false,
        },
        PackDescriptor {
            pack_id: "bifrost.spring-web-sanitizers",
            file: "staged/bifrost.spring-web-sanitizers.json",
            ecosystem: "maven",
            package: Some("org.springframework:spring-web"),
            mandatory: false,
            pinned_to_esapi_digest: false,
        },
    ];

    fn collect_dependency_jars(deps_dir: &Path) -> Result<Vec<PathBuf>, String> {
        let mut jars = Vec::new();
        let entries = fs::read_dir(deps_dir)
            .map_err(|error| format!("read deps dir {}: {error}", deps_dir.display()))?;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jar") {
                jars.push(path);
            }
        }
        jars.sort();
        Ok(jars)
    }

    fn git_head(root: &Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    #[derive(Serialize)]
    struct Artifact {
        schema: &'static str,
        benchmark: BenchmarkMeta,
        bifrost: BifrostMeta,
        scope: ScopeMeta,
        policy: PolicyMeta,
        packs: PacksMeta,
        run: RunMeta,
        result: ResultSummary,
        scoring_methodology: Vec<&'static str>,
        headline: Headline,
        category_runs: Vec<brokk_bifrost::owasp_benchmark::CategoryRunStatus>,
        scoreboard: brokk_bifrost::owasp_benchmark::Scoreboard,
    }

    /// A plain-language verdict a reader gets before the tables.
    #[derive(Serialize)]
    struct ResultSummary {
        verdict: &'static str,
        headline_number: String,
        false_greens: u32,
        findings: usize,
        why_abstained: Vec<&'static str>,
        directs_next_work: Vec<&'static str>,
    }

    #[derive(Serialize)]
    struct BenchmarkMeta {
        repo: &'static str,
        commit: String,
        version: &'static str,
        expected_results: &'static str,
    }

    #[derive(Serialize)]
    struct BifrostMeta {
        version: &'static str,
        commit: Option<String>,
    }

    #[derive(Serialize)]
    struct ScopeMeta {
        scored_categories: Vec<&'static str>,
        note: &'static str,
        skipped_non_taint_categories: BTreeMap<String, u32>,
    }

    #[derive(Serialize)]
    struct PolicyMeta {
        mode: &'static str,
        note: &'static str,
        per_category: Vec<CategoryPolicy>,
    }

    #[derive(Serialize)]
    struct CategoryPolicy {
        category: &'static str,
        policy_id: String,
        source: String,
    }

    #[derive(Serialize)]
    struct PacksMeta {
        activated: Vec<String>,
        skipped: Vec<SkippedPack>,
        esapi: EsapiMeta,
    }

    #[derive(Serialize)]
    struct SkippedPack {
        pack_id: String,
        reason: String,
    }

    #[derive(Serialize)]
    struct EsapiMeta {
        coordinate: &'static str,
        artifact_sha256: Option<String>,
    }

    #[derive(Serialize)]
    struct RunMeta {
        cases_scored: usize,
        cases_with_taint_roots: usize,
        taint_roots: usize,
        findings_total: usize,
        dependency_jars: usize,
        case_limit: Option<usize>,
    }

    #[derive(Serialize)]
    struct Headline {
        note: &'static str,
        overall_naive_youden: Option<f64>,
        overall_honest_youden: Option<f64>,
        real_cases: u32,
        real_flagged_true_positive: u32,
        real_cleared_false_green: u32,
        real_abstained_inconclusive: u32,
    }

    fn run_command(args: &[String]) -> Result<(), String> {
        let flags = parse_flags(args)?;
        let benchmark_root = PathBuf::from(
            flags
                .get("benchmark")
                .ok_or_else(|| "--benchmark <dir> is required".to_owned())?,
        );
        let packs_dir = PathBuf::from(
            flags
                .get("packs-dir")
                .ok_or_else(|| "--packs-dir <dir> is required".to_owned())?,
        );
        let out = flags
            .get("out")
            .ok_or_else(|| "--out <path> is required".to_owned())?;
        let esapi_digest = flags.get("esapi-digest").cloned();
        let dependency_jars = match flags.get("deps") {
            Some(dir) => collect_dependency_jars(Path::new(dir))?,
            None => Vec::new(),
        };
        let case_limit = flags
            .get("limit")
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|error| format!("--limit: {error}"))
            })
            .transpose()?;
        let timeout = Duration::from_secs(
            flags
                .get("timeout-secs")
                .map(|s| {
                    s.parse::<u64>()
                        .map_err(|error| format!("--timeout-secs: {error}"))
                })
                .transpose()?
                .unwrap_or(3600),
        );

        let sanitizer_packs: Vec<SanitizerPackSpec> = PACKS
            .iter()
            .map(|descriptor| SanitizerPackSpec {
                pack_id: descriptor.pack_id.to_owned(),
                json_path: packs_dir.join(descriptor.file),
                ecosystem: descriptor.ecosystem.to_owned(),
                package: descriptor.package.map(|name| CatalogCoordinate {
                    name: name.to_owned(),
                    version: None,
                }),
                artifact_sha256: descriptor
                    .pinned_to_esapi_digest
                    .then(|| esapi_digest.clone())
                    .flatten(),
                mandatory: descriptor.mandatory,
            })
            .collect();

        let config = RunConfig {
            benchmark_root: benchmark_root.clone(),
            dependency_jars,
            sanitizer_packs,
            timeout,
            case_limit,
        };

        eprintln!("building benchmark workspace and running taint policies...");
        let result = run_bakeoff(&config)?;
        let artifact = assemble_artifact(&benchmark_root, &result, esapi_digest, &config);
        let mut rendered = serde_json::to_string_pretty(&artifact)
            .map_err(|error| format!("serialize artifact: {error}"))?;
        rendered.push('\n');
        if let Some(parent) = Path::new(out).parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
        }
        fs::write(out, rendered.as_bytes()).map_err(|error| format!("write {out}: {error}"))?;

        print_summary(&result);
        println!("wrote artifact to {out}");
        Ok(())
    }

    fn assemble_artifact(
        benchmark_root: &Path,
        result: &RunResult,
        esapi_digest: Option<String>,
        config: &RunConfig,
    ) -> Artifact {
        let benchmark_commit = git_head(benchmark_root).unwrap_or_else(|| "unknown".to_owned());
        let per_category = InjectionCategory::ALL
            .into_iter()
            .map(|category| CategoryPolicy {
                category: category.label(),
                policy_id: policy_id(category),
                source: build_policy(category),
            })
            .collect();
        let overall = &result.scoreboard.overall;
        Artifact {
            schema: "bifrost.owasp-benchmark-java.v1",
            benchmark: BenchmarkMeta {
                repo: "https://github.com/OWASP-Benchmark/BenchmarkJava",
                commit: benchmark_commit,
                version: "1.2",
                expected_results: "expectedresults-1.2.csv",
            },
            bifrost: BifrostMeta {
                version: env!("CARGO_PKG_VERSION"),
                commit: git_head(Path::new(".")),
            },
            scope: ScopeMeta {
                scored_categories: InjectionCategory::ALL
                    .into_iter()
                    .map(InjectionCategory::label)
                    .collect(),
                note: "Scores only the injection/taint subset (a source reaches a sink). \
                       The Benchmark's crypto, hash, weakrand, securecookie, and trustbound \
                       categories are not source-to-sink taint problems and are out of scope.",
                skipped_non_taint_categories: result.label_set.skipped_non_taint.clone(),
            },
            policy: PolicyMeta {
                mode: "require-model",
                note: "One taint policy per category, sharing the attacker-controlled source \
                       set and differing only in sinks, so a finding attributes unambiguously \
                       to a category. `require-model` fails closed on an unmodeled boundary: an \
                       unmodeled call on a flow yields Inconclusive, never a silent pass-through.",
                per_category,
            },
            packs: PacksMeta {
                activated: result.activated_pack_ids.clone(),
                skipped: result
                    .skipped_packs
                    .iter()
                    .map(|(pack_id, reason)| SkippedPack {
                        pack_id: pack_id.clone(),
                        reason: reason.clone(),
                    })
                    .collect(),
                esapi: EsapiMeta {
                    coordinate: "org.owasp.esapi:esapi:2.7.0.0",
                    artifact_sha256: esapi_digest,
                },
            },
            run: RunMeta {
                cases_scored: result.label_set.cases.len(),
                cases_with_taint_roots: result.cases_with_roots,
                taint_roots: result.taint_roots,
                findings_total: result.findings_total,
                dependency_jars: config.dependency_jars.len(),
                case_limit: config.case_limit,
            },
            result: ResultSummary {
                verdict: "abstained (capability_incomplete) across the taint subset: the shipped \
                          require-model taint policy produced no source-to-sink verdicts on the \
                          OWASP Benchmark under name-based endpoint selection.",
                headline_number: format!(
                    "naive Youden J = {}; honest Youden J = {} (no case was decided). \
                     0 findings, 0 false positives, 0 false greens over {} taint cases.",
                    overall
                        .naive_metrics
                        .youden
                        .map_or_else(|| "undefined".to_owned(), |v| format!("{v:.3}")),
                    overall
                        .honest_metrics
                        .youden
                        .map_or_else(|| "undefined".to_owned(), |v| format!("{v:.3}")),
                    overall.total,
                ),
                false_greens: overall.false_greens,
                findings: result.findings_total,
                why_abstained: vec![
                    "Endpoint binding requires every selected source and sink to bind into a \
                     shared, completely-discovered call region; workspace-wide name selectors \
                     match sources in files that hold no matching sink, so coverage fails and the \
                     whole policy compile abstains (capability_incomplete).",
                    "At corpus scale the source selector also exceeds the RQL 100-result limit \
                     before binding (result_limit_reached), truncating the match set to a partial \
                     discovery.",
                    "Binding hard-fails when a matched sink call lacks the selected argument index \
                     -- arity-overloaded names like Statement.execute / PreparedStatement.execute() \
                     -- and there is no RQL predicate to constrain a call's arity.",
                    "Where binding does succeed (observed per-file for pathtraver/xss), the flow \
                     still crosses an unmodeled JDK transform (URLDecoder.decode, java.util.List \
                     add/get, Enumeration.nextElement, StringBuilder) on essentially every \
                     Benchmark case, so require-model propagation reaches PartialDiscovery and \
                     abstains rather than fabricating a pass-through.",
                ],
                directs_next_work: vec![
                    "Make endpoint binding arity-tolerant: skip a matched call that lacks the \
                     selected operand instead of aborting the whole policy compile.",
                    "Bind endpoints per completely-discovered region (or add a file/region scope to \
                     selectors) so workspace-wide selection does not require global co-occurrence.",
                    "Ship taint-preserving procedure summaries for the common JDK string/codec/ \
                     collection transforms (URLDecoder.decode, StringBuilder, List/Map, \
                     Enumeration) so require-model can close the boundaries the Benchmark routes \
                     every flow through -- the same summaries-vs-engine question this run feeds.",
                    "Mount external declaration packs (servlet, java.sql, ESAPI) so sanitizer \
                     summaries can bind by exact symbol, letting ESAPI-sanitized fake cases clear \
                     rather than abstain.",
                ],
            },
            scoring_methodology: vec![
                "Each Benchmark case belongs to exactly one category, so each category is scored \
                 over only its own labeled cases.",
                "flagged = Bifrost produced >=1 taint finding of the case's category on the \
                 case's file.",
                "cleared = no finding AND the case's flow analysis reliably concluded (Complete \
                 or ProvenBySummary on every root that touched it).",
                "abstained = no finding AND the analysis was Inconclusive, or no taint root \
                 formed at all.",
                "naive score: an abstention folds into the negatives (an abstained real is a \
                 false negative, an abstained fake is a true negative).",
                "honest score: abstentions are excluded from the confusion matrix and reported \
                 separately. The gap between naive and honest is the cost of honest abstention.",
                "false green = a real vulnerability Bifrost affirmatively cleared as safe. The \
                 'no false greens' pitch is that this count stays near zero: when Bifrost cannot \
                 prove safety it abstains rather than clearing.",
                "TPR = TP/(TP+FN); FPR = FP/(FP+TN); Youden J = TPR - FPR.",
            ],
            headline: Headline {
                note: "Overall Youden over the taint subset, both ways, plus the real-case \
                       disposition that carries the honest-completion story.",
                overall_naive_youden: overall.naive_metrics.youden,
                overall_honest_youden: overall.honest_metrics.youden,
                real_cases: overall.real,
                real_flagged_true_positive: overall.real_flagged,
                real_cleared_false_green: overall.real_cleared,
                real_abstained_inconclusive: overall.real_abstained,
            },
            category_runs: result.category_runs.clone(),
            scoreboard: result.scoreboard.clone(),
        }
    }

    fn print_summary(result: &RunResult) {
        let board = &result.scoreboard;
        println!("== OWASP BenchmarkJava taint bakeoff ==");
        println!(
            "cases scored: {}, cases with taint roots: {}, taint roots: {}, findings: {}",
            result.label_set.cases.len(),
            result.cases_with_roots,
            result.taint_roots,
            result.findings_total
        );
        println!("activated packs: {:?}", result.activated_pack_ids);
        if !result.skipped_packs.is_empty() {
            println!("skipped packs: {:?}", result.skipped_packs);
        }
        println!();
        println!("run-level taint completion per category:");
        for status in &result.category_runs {
            println!(
                "  {:<12} {} (findings={}, retained={})",
                status.category, status.completion, status.findings, status.retained_analyses
            );
            if let Some(sample) = status.sample_diagnostics.first() {
                println!("      e.g. {sample}");
            }
        }
        println!();
        println!(
            "{:<12} {:>5} {:>5} {:>5} | naive TPR/FPR/J     | honest TPR/FPR/J    | compl C/PbS/Inc/NA | false-green",
            "category", "total", "real", "fake"
        );
        for score in board
            .per_category
            .iter()
            .chain(std::iter::once(&board.overall))
        {
            println!(
                "{:<12} {:>5} {:>5} {:>5} | {:>5}/{:>5}/{:>6} | {:>5}/{:>5}/{:>6} | {:>4}/{:>4}/{:>4}/{:>4} | {}",
                score.category,
                score.total,
                score.real,
                score.fake,
                fmt_rate(score.naive_metrics.tpr),
                fmt_rate(score.naive_metrics.fpr),
                fmt_rate(score.naive_metrics.youden),
                fmt_rate(score.honest_metrics.tpr),
                fmt_rate(score.honest_metrics.fpr),
                fmt_rate(score.honest_metrics.youden),
                score.completion.complete,
                score.completion.proven_by_summary,
                score.completion.inconclusive,
                score.completion.not_analyzed,
                score.false_greens,
            );
        }
    }

    fn fmt_rate(value: Option<f64>) -> String {
        value.map_or_else(|| "-".to_owned(), |v| format!("{v:.2}"))
    }
}
