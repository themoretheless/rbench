mod artifacts;
mod experiment_report;
mod forma;
mod git_run;
mod matrix;
mod privacy;
mod project;
mod revisions;
mod runner;
mod sessions;
mod web_ui;
use clap::{Parser, Subcommand};
use rbench::{analysis, report, *};
use std::{
    fs::OpenOptions,
    io::{IsTerminal, Write},
    path::PathBuf,
    process::Command,
};
#[derive(Parser)]
#[command(
    name = "cargo rbench",
    version,
    about = "Reproducible local benchmarks, explicit metrics, offline A/B reports"
)]
struct Cli {
    #[command(subcommand)]
    command: Action,
    /// Storage for named baselines.
    #[arg(long, global = true, default_value = ".rbench")]
    store: PathBuf,
}
#[derive(Subcommand)]
enum Action {
    /// Browse saved experiments in a local web interface.
    Serve {
        #[arg(default_value = ".rbench")]
        root: PathBuf,
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// Execute a Cartesian matrix of explicit worker CLI arguments, sequentially.
    Matrix {
        #[arg(long)]
        plan: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Search a bounded first-parent history; audit every commit to detect nonmonotonic regressions.
    Bisect {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        good: String,
        #[arg(long)]
        bad: String,
        #[arg(long)]
        target: String,
        #[arg(long, default_value = "Cargo.toml")]
        manifest_path: PathBuf,
        #[arg(long, default_value_t = 12)]
        repetitions: u32,
        #[arg(long, default_value_t = 16)]
        max_commits: usize,
        #[arg(long)]
        offline: bool,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Continue an interrupted allowlisted experiment as a new linked session.
    Resume {
        run: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Replay one recorded case under an explicit profiler executable.
    Profile {
        run: PathBuf,
        #[arg(long)]
        case: String,
        #[arg(long)]
        profiler: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Inventory storage; --apply moves eligible owned runs into reversible quarantine.
    Retention {
        #[arg(long, default_value_t = 20)]
        keep: usize,
        #[arg(long)]
        apply: bool,
    },
    /// Diagnose between-process spread, chronological drift and AB/BA effects.
    Diagnose { run: PathBuf },
    /// Plan a NEW confirmation experiment; the pilot is never pooled automatically.
    Pilot {
        run: PathBuf,
        #[arg(long, default_value_t = 5.0)]
        precision: f64,
        #[arg(long, default_value_t = 200)]
        max_processes: usize,
    },
    /// Raw frame deadline exceedance; does not infer compositor-dropped frames.
    Deadlines {
        run: PathBuf,
        #[arg(long)]
        case: String,
        #[arg(long)]
        metric: String,
        #[arg(long, value_delimiter = ',', default_value = "60,120,144")]
        hz: Vec<f64>,
    },
    /// Compare all variants against one reference with family-wise correction.
    Multi {
        run: PathBuf,
        #[arg(long, default_value = "baseline")]
        reference: String,
        #[arg(long, default_value_t = 5.0)]
        threshold: f64,
        #[arg(long, default_value_t = 0.05)]
        alpha: f64,
    },
    /// Prepare a PR comment from a completed base/head run; never publishes.
    PrReport {
        run: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Preview the exact categories and sizes included in a bundle.
    ExportPreview { run: PathBuf },

    /// Show the effort and limitations of named worker sampling profiles.
    Profiles,
    /// List recorded runs chronologically; last resolves the latest complete run.
    History {
        #[arg(long)]
        json: bool,
    },
    /// Show exact environment, provenance and workload-contract differences.
    Context {
        baseline: PathBuf,
        candidate: PathBuf,
    },
    /// Descriptive performance history of an exact case/metric.
    Trend {
        #[arg(long)]
        case: String,
        #[arg(long)]
        metric: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Export raw observations, descriptors and availability.
    Export {
        run: PathBuf,
        #[arg(long)]
        format: String,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Produce a portable JSON bundle of run, report and notes (no binaries/logs).
    Bundle {
        run: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Verify and unpack a bundle into a new directory.
    Unpack {
        bundle: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Append a note without altering immutable run data.
    Note { run: PathBuf, text: String },
    /// Read the notes attached to an unchanged run.
    Notes { run: PathBuf },
    /// Check binaries, fixtures, cwd, deadlines and output without executing workers.
    Preflight {
        #[arg(long)]
        plan: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Compare local committed Git snapshots; current checkout remains untouched.
    GitCompare {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        baseline: String,
        #[arg(long)]
        candidate: String,
        #[arg(long)]
        target: String,
        #[arg(long, default_value = "Cargo.toml")]
        manifest_path: PathBuf,
        #[arg(long, default_value_t = 12)]
        repetitions: u32,
        #[arg(long)]
        offline: bool,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Write a manually triggered GitHub Actions smoke workflow, never overwrite.
    Ci {
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Scaffold a checked parameterized benchmark in an existing Cargo package.
    Init {
        #[arg(long, default_value = "Cargo.toml")]
        manifest_path: PathBuf,
        #[arg(long)]
        library_path: Option<PathBuf>,
    },
    /// Discover Cargo workspace benchmark targets without executing them.
    Discover {
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        json: bool,
    },
    /// Build all registered workspace targets first, then measure them sequentially.
    Bench {
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        #[arg(long)]
        offline: bool,
        /// Select exact package/target (may repeat); only registered targets are runnable.
        #[arg(long)]
        target: Vec<String>,
        #[arg(long, default_value_t = 12)]
        repetitions: u32,
        #[arg(long, default_value_t = 60000)]
        timeout_ms: u64,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Register or list immutable named baseline references.
    Baseline {
        #[command(subcommand)]
        action: BaselineAction,
    },
    /// Evaluate exact-case absolute and relative budgets from JSON.
    Gate {
        run: PathBuf,
        #[arg(long, default_value = "rbench.json")]
        config: PathBuf,
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long, value_enum, default_value = "fail")]
        uncertainty: Uncertainty,
    },
    /// Execute a JSON plan or one program; output directory must not exist.
    Run {
        #[arg(long)]
        plan: Option<PathBuf>,
        #[arg(long)]
        program: Option<PathBuf>,
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long, default_value_t = 12)]
        repetitions: u32,
        #[arg(long, default_value_t = 60000)]
        timeout_ms: u64,
        #[arg(long)]
        protocol: bool,
        #[arg(long)]
        dry_run: bool,
        /// Start and open the live interface even without an interactive terminal.
        #[arg(long, conflicts_with = "no_ui")]
        ui: bool,
        /// Disable the automatic interface (CI/headless runs).
        #[arg(long)]
        no_ui: bool,
        /// Print the interface URL without launching a browser.
        #[arg(long)]
        no_open: bool,
        /// One diagnostic memory-profile run; requires an instrumented Rust worker.
        #[arg(long)]
        memory: bool,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Compare paired run, or two independent runs. Nonzero exit with --check on uncertain results.
    Compare {
        baseline: PathBuf,
        candidate: Option<PathBuf>,
        #[arg(long, default_value_t = 5.0)]
        threshold: f64,
        #[arg(long, default_value_t = 0.05)]
        alpha: f64,
        #[arg(long)]
        json: bool,
        #[arg(long, value_enum, default_value = "fail")]
        uncertainty: Uncertainty,
        #[arg(long)]
        check: bool,
        #[arg(long, default_value = "")]
        filter: String,
        /// Exact case-id match (mutually exclusive with --glob).
        #[arg(long)]
        exact: bool,
        /// Glob case-id match (*, ?); mutually exclusive with --exact.
        #[arg(long)]
        glob: bool,
        /// Glob patterns to exclude (repeatable).
        #[arg(long)]
        exclude: Vec<String>,
        /// Require contract tag.NAME=true (repeatable; AND).
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long, default_value = "")]
        metric: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check every available observation against an absolute maximum and optional minimum (no statistical inference).
    Check {
        run: PathBuf,
        #[arg(long)]
        metric: String,
        #[arg(long)]
        max: Option<f64>,
        /// Absolute lower bound (useful for Higher metrics such as throughput).
        #[arg(long)]
        min: Option<f64>,
    },
    /// Import completed legacy Forma offscreen or paired directories.
    ImportForma {
        source: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Build an offline report for one run or an experiment tree; .html/.json/.md select format.
    Report {
        run: PathBuf,
        /// Baseline run or collection matched by relative target paths.
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long, default_value = "Benchmark experiment")]
        title: String,
        #[arg(long, default_value_t = 5.0)]
        threshold: f64,
        #[arg(long, default_value_t = 0.05)]
        alpha: f64,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List cases already recorded in a run, without executing workloads.
    List {
        run: PathBuf,
        #[arg(long, default_value = "")]
        filter: String,
        #[arg(long)]
        exact: bool,
        #[arg(long)]
        glob: bool,
        #[arg(long)]
        exclude: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
    },
    /// Build benchmark executables without measuring (for project harness=false benches).
    Build {
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        #[arg(long)]
        offline: bool,
    },
    /// Run synthetic statistical acceptance (coverage + A/A FPR); offline, no host workload.
    Accept {
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Also run live Instant A/A on this host (noisy; not a CI gate).
        #[arg(long)]
        hardware: bool,
        #[arg(long, default_value_t = 8)]
        pairs: usize,
        #[arg(long, default_value_t = 20)]
        trials: usize,
        #[arg(long, default_value_t = 50_000)]
        batch_iters: u64,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Inspect an interrupted run directory without mutating it.
    Recover {
        run: PathBuf,
    },
    /// Print available host capabilities; does not change system settings.
    Doctor,
    /// Print an honest capability scorecard vs common Rust timing tools.
    Compete {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
#[derive(Clone, Copy, clap::ValueEnum)]
enum Uncertainty {
    Fail,
    Warn,
    Record,
}

#[derive(Subcommand)]
enum BaselineAction {
    Save { name: String, run: PathBuf },
    List,
}
fn compete_scorecard() -> serde_json::Value {
    let perf = rbench::perf::probe();
    let snap = rbench::isolate::snapshot();
    let os = rbench::process::sample()
        .map(|s| {
            serde_json::json!({
                "available": true,
                "rss_bytes": s.rss_bytes,
                "user_cpu_ns": s.user_cpu_ns,
            })
        })
        .unwrap_or_else(|e| serde_json::json!({"available": false, "error": e.to_string()}));
    serde_json::json!({
        "schema": 1,
        "product": "rbench",
        "version": env!("CARGO_PKG_VERSION"),
        "thesis": "Win on measurement validity and multi-metric contracts, not on microbench ergonomics marketing.",
        "host": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "process_metrics": os,
            "perf_event": {
                "availability": format!("{:?}", perf.availability),
                "note": perf.note,
            },
            "isolation": snap,
            "noise_warnings": rbench::isolate::noise_warnings(&snap),
        },
        "dimensions": [
            {
                "id": "process_isolation",
                "rbench": "first-class: AB/BA independent processes, leases, hashes, incomplete states",
                "criterion": "in-process by default; Criterion groups share address space",
                "divan": "in-process; designed for hot-loop throughput",
                "iai": "Cachegrind/Callgrind instrumentation; strong for deterministic CI counts",
                "hyperfine": "process wall time only; excellent CLI A/B for commands",
                "verdict": "rbench leads for confirmatory product/scenario gates that need process units"
            },
            {
                "id": "multi_metric_contracts",
                "rbench": "wall + throughput + OS RSS/CPU + optional perf.instructions/cycles + scenario GPU/alloc with Availability",
                "criterion": "primarily wall/throughput plots; custom measurements possible but not a typed availability model",
                "divan": "wall/throughput focused",
                "iai": "instructions/cycles/bytes via Valgrind; not wall-time product gates",
                "hyperfine": "wall (+ optional shell); no memory/perf contracts",
                "verdict": "rbench leads when one run must carry typed multi-metric evidence"
            },
            {
                "id": "inconclusive_gates",
                "rbench": "Inconclusive is a first-class CI outcome; never silently treated as pass/equiv",
                "criterion": "CI usually threshold scripts on estimates; inconclusive semantics are DIY",
                "divan": "not a confirmatory gate product",
                "iai": "exact counters; different failure mode (tooling/env)",
                "hyperfine": "statistical summary; gating is DIY",
                "verdict": "rbench leads for honest regression CI"
            },
            {
                "id": "hot_loop_ergonomics",
                "rbench": "Suite builder + harness=false; overhead documented via examples/overhead.rs — not zero",
                "criterion": "mature macros, plots, html; ecosystem default",
                "divan": "very low overhead / ergonomic benches",
                "iai": "N/A for microbench UX",
                "hyperfine": "N/A (external commands)",
                "verdict": "Criterion/Divan still win day-to-day microbench UX; do not claim otherwise"
            },
            {
                "id": "deterministic_ci_counters",
                "rbench": "Linux perf_event when permitted; PermissionDenied recorded — Callgrind adapter still open",
                "criterion": "wall noise on shared runners",
                "divan": "wall noise on shared runners",
                "iai": "leads for Valgrind-backed instruction counts in CI",
                "hyperfine": "wall noise",
                "verdict": "iai still leads for Valgrind-deterministic CI; rbench perf is host-capability gated"
            },
            {
                "id": "command_wall_benchmarks",
                "rbench": "run --program measures process wall with protocol option; heavier than hyperfine for simple cmds",
                "criterion": "N/A",
                "divan": "N/A",
                "iai": "N/A",
                "hyperfine": "leads for shell command A/B",
                "verdict": "hyperfine wins simple command timing; rbench wins when you need contracts+gates"
            }
        ],
        "claims_forbidden": [
            "beats Criterion/Divan on microbench ergonomics or absolute hot-loop overhead",
            "beats iai on Valgrind-deterministic CI without a Callgrind adapter",
            "hosted GitHub Actions is a controlled performance acceptance environment",
            "CPU pin or loadavg snapshot equals BenchExec-grade isolation"
        ],
        "how_to_reproduce": {
            "doctor": "cargo rbench doctor",
            "synthetic_accept": "cargo rbench accept --seed 42",
            "hardware_aa": "RBENCH_PIN_CPU=0 cargo rbench accept --hardware --pairs 8 --trials 20",
            "multi_metric_demo": "cargo run --release --example compete --offline",
            "docs": "docs/COMPETE.md"
        }
    })
}

fn output(text: &str, path: Option<PathBuf>) -> Result<()> {
    if let Some(p) = path {
        let mut f = OpenOptions::new().create_new(true).write(true).open(p)?;
        f.write_all(text.as_bytes())?;
    } else {
        println!("{text}");
    }
    Ok(())
}

fn select_cases(
    run: &mut Run,
    filter: &str,
    exact: bool,
    glob_mode: bool,
    exclude: &[String],
    tags: &[String],
) -> Result<()> {
    let selection = Selection {
        pattern: filter.into(),
        exact,
        glob: glob_mode,
        exclude: exclude.to_vec(),
        tags: tags.to_vec(),
    };
    selection.validate()?;
    run.cases.retain(|c| selection.matches(c));
    run.observations
        .retain(|o| run.cases.iter().any(|c| c.id == o.case));
    Ok(())
}

fn execute() -> Result<i32> {
    let mut args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|x| x == "rbench") {
        args.remove(1);
    }
    let cli = Cli::parse_from(args);
    let load = |p: PathBuf| Run::load(project::resolve(&cli.store, &p)?);
    match cli.command {
        Action::Matrix{plan,output}=>matrix::run(serde_json::from_slice(&std::fs::read(plan)?)?,&output)?,
        Action::Bisect{repo,good,bad,target,manifest_path,repetitions,max_commits,offline,output,args}=>revisions::run(revisions::Search{repo:&repo,good:&good,bad:&bad,target:&target,manifest:&manifest_path,out:&output,repetitions,max_commits,offline,args})?,
        Action::Resume{run,output}=>sessions::resume(&project::resolve(&cli.store,&run)?,&output)?,
        Action::Profile{run,case,profiler,output,args}=>sessions::profile(&project::resolve(&cli.store,&run)?,&case,&profiler,args,&output)?,
        Action::Retention{keep,apply}=>println!("{}",serde_json::to_string_pretty(&sessions::retention(&cli.store,keep,apply)?)?),
        Action::Diagnose{run} => {let r=load(run)?;println!("{}",serde_json::to_string_pretty(&serde_json::json!({"spread_and_drift":diagnostics::diagnose(&r)?,"order":diagnostics::order_effects(&r)?,"status":r.status,"unavailable_observations":r.observations.iter().filter(|o|o.availability!=Availability::Available).count(),"interpretation":"Descriptive thresholds; missing observations excluded and counted. No causal or significance inference."}))?);},
        Action::Pilot{run,precision,max_processes}=>println!("{}",serde_json::to_string_pretty(&diagnostics::pilot(&load(run)?,precision,max_processes)?)?),
        Action::Deadlines{run,case,metric,hz}=>println!("{}",serde_json::to_string_pretty(&diagnostics::deadlines(&load(run)?,&case,&metric,&hz)?)?),
        Action::Multi{run,reference,threshold,alpha}=>println!("{}",serde_json::to_string_pretty(&analysis::compare_multi(&load(run)?,&reference,threshold,alpha)?)?),
        Action::PrReport{run,output:path}=>{
            let r=load(run)?;
            let base=r.provenance.get("user.git.baseline").ok_or_else(||error("base revision absent; use git-compare"))?;
            let head=r.provenance.get("user.git.candidate").ok_or_else(||error("head revision absent; use git-compare"))?;
            let rows=analysis::compare(&r,None,5.,0.05)?;
            output(&format!("## Benchmark comparison\n\nBase: `{}`\nHead: `{}`\n\n{}\n\nFixed process-pair design, 5% practical margin, 95% family confidence. Inconclusive means insufficient evidence, not equivalence. Workload contracts and metric scopes are part of the attached run artifact.\n",report::escape(base),report::escape(head),report::comparison(&rows)),Some(path))?;
        },
        Action::ExportPreview{run}=>{
            let resolved=project::resolve(&cli.store,&run)?;let r=Run::load(&resolved)?;let notes=artifacts::notes(&cli.store,&resolved)?;
            println!("{}",serde_json::to_string_pretty(&serde_json::json!({"file_bytes":{"run.json":std::fs::metadata(if resolved.is_dir(){resolved.join("run.json")}else{resolved.clone()})?.len(),"report.html":report::html_run(&r)?.len(),"notes.json":serde_json::to_string_pretty(&notes)?.len()},"run_id":r.id,"cases":r.cases.len(),"observations":r.observations.len(),"environment_keys":r.environment.keys().collect::<Vec<_>>(),"provenance_keys":r.provenance.keys().collect::<Vec<_>>(),"notes":notes.len(),"included":["run.json: all contracts, environment, provenance, observations and notes","report.html: derived report","notes.json: sidecar user notes"],"excluded":["worker logs","binaries","fixtures","plan"],"review":"Inspect run and notes before sharing. Preview does not certify absence of unknown or transformed secrets."}))?);
        },

        Action::Profiles => println!("quick: 8 samples × 1 ms + 10 ms warmup/case; smoke only\nnormal: 30 × 5 ms + 50 ms warmup/case\nthorough: 100 × 10 ms + 200 ms warmup/case\nUse worker --profile NAME after --. CLI --repetitions controls independent processes separately. Profiles do not guarantee confidence/precision."),
        Action::History {json} => {let rows=artifacts::history(&cli.store)?;if json{println!("{}",serde_json::to_string_pretty(&rows)?);}else{for r in rows{println!("{} {} {} {}",r.id,r.status,r.path.display(),r.error.unwrap_or_default());}}},
        Action::Context {baseline,candidate} => println!("{}",artifacts::context(&load(baseline)?,&load(candidate)?)),
        Action::Trend {case,metric,output:path} => {
            let text=artifacts::trend(&cli.store,&case,&metric)?;
            let text=if path.as_ref().is_some_and(|p|p.extension().is_some_and(|x|x=="html")){artifacts::trend_html(&text)}else{text};output(&text,path)?;
        },
        Action::Export {run,format,output:path} => output(&artifacts::export(&load(run)?,&format)?,Some(path))?,
        Action::Bundle {run,output} => artifacts::bundle(&cli.store,&project::resolve(&cli.store,&run)?,&output)?,
        Action::Unpack {bundle,output} => artifacts::unpack(&bundle,&output)?,
        Action::Note {run,text} => artifacts::note(&cli.store,&project::resolve(&cli.store,&run)?,&text)?,
        Action::Notes {run} => {for n in artifacts::notes(&cli.store,&project::resolve(&cli.store,&run)?)?{println!("{}: {}",n.created_ns,n.text);}},
        Action::Preflight {plan,output} => println!("{}",serde_json::to_string_pretty(&runner::preflight(serde_json::from_slice(&std::fs::read(plan)?)?,&output)?)?),
        Action::GitCompare {repo,baseline,candidate,target,manifest_path,repetitions,offline,output,args} => git_run::run(git_run::Request{repo:&repo,base:&baseline,head:&candidate,target:&target,manifest:&manifest_path,output:&output,repetitions,offline,args})?,
        Action::Ci {output:path} => output(include_str!("ci-template.yml"),Some(path))?,
        Action::Init {
            manifest_path,
            library_path,
        } => project::init(&manifest_path, library_path.as_deref())?,
        Action::Discover {
            manifest_path,
            offline,
            json,
        } => {
            let targets = project::discover(manifest_path.as_deref(), offline)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&targets)?);
            } else {
                for t in targets {
                    println!(
                        "{}/{} [{}]",
                        t.package,
                        t.name,
                        if t.registered {
                            "rbench"
                        } else {
                            "unregistered; not auto-run"
                        }
                    );
                }
            }
        }
        Action::Bench {
            manifest_path,
            offline,
            target,
            repetitions,
            timeout_ms,
            output: out,
            mut args,
        } => {
            if out.exists() {
                return Err(error("output directory already exists"));
            }
            if repetitions == 0 || repetitions > 10000 || timeout_ms == 0 {
                return Err(error("invalid repetitions/timeout"));
            }
            let mut targets = project::discover(manifest_path.as_deref(), offline)?;
            for requested in &target {
                if !targets
                    .iter()
                    .any(|t| t.registered && format!("{}/{}", t.package, t.name) == *requested)
                {
                    return Err(error(format!("registered target not found: {requested}")));
                }
            }
            targets.retain(|t| {
                t.registered
                    && (target.is_empty() || target.contains(&format!("{}/{}", t.package, t.name)))
            });
            if targets.is_empty() {
                return Err(error(
                    "no registered targets; use init or package.metadata.rbench.targets",
                ));
            }
            // Complete all compilation before taking any measurements.
            let builds = targets
                .iter()
                .map(|t| project::build(t, offline))
                .collect::<Result<Vec<_>>>()?;
            if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::create_dir(&out)?;
            model::write_new(&out.join("targets.json"), &targets)?;
            if !args.iter().any(|s| s == "--json") {
                args.push("--json".into());
            }
            for (i, (t, path)) in targets.iter().zip(builds).enumerate() {
                let directory = out.join(format!("{i}-{}-{}", t.package, t.name));
                eprintln!(
                    "rbench: target {}/{} {}/{}",
                    i + 1,
                    targets.len(),
                    t.package,
                    t.name
                );
                let run = runner::run(
                    runner::Plan {
                        privacy: None,
                        variants: Default::default(),
                        start_pair:0,
                        candidate: runner::Program {
                            path,
                            args: args.clone(),
                            env: Default::default(),
                            cwd: t.manifest.parent().map(|p| p.to_path_buf()),
                        },
                        baseline: None,
                        repetitions,
                        timeout_ms,
                        protocol: true,
                        fixtures: vec![],
                        contract: Default::default(),
                        provenance: Default::default(),
                    },
                    &directory,
                )?;
                println!("{}\nSaved {}", report::markdown(&run)?, directory.display());
            }
        }
        Action::Baseline { action } => match action {
            BaselineAction::Save { name, run } => {
                project::save_baseline(&cli.store, &name, &project::resolve(&cli.store, &run)?)?
            }
            BaselineAction::List => project::baselines(&cli.store)?,
        },
        Action::Gate {
            run,
            config,
            baseline,
            json,
            uncertainty,
        } => {
            let config = serde_json::from_slice(&std::fs::read(config)?)?;
            let run = load(run)?;
            let baseline = baseline.map(load).transpose()?;
            let rows = budget::evaluate(&config, &run, baseline.as_ref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for row in &rows {
                    println!(
                        "{} / {}: {} — {}",
                        row.case, row.metric, row.decision, row.detail
                    );
                }
            }
            let code=budget::exit_code(&rows);
            if code==2 && rows.iter().all(|r|r.decision=="passed"||r.decision=="inconclusive") && !matches!(uncertainty,Uncertainty::Fail) {
                if matches!(uncertainty,Uncertainty::Warn){eprintln!("rbench: warning: inconclusive budgets allowed by policy; no proof of equivalence");}return Ok(0);
            }
            return Ok(code);
        }
        Action::Run {
            plan,
            program,
            baseline,
            repetitions,
            timeout_ms,
            protocol,
            dry_run,
            ui,
            no_ui,
            no_open,
            memory,
            output: out,
            args,
        } => {
            if plan.is_some()
                && (program.is_some()
                    || baseline.is_some()
                    || !args.is_empty()
                    || protocol
                    || repetitions != 12
                    || timeout_ms != 60000)
            {
                return Err(error("--plan cannot be combined with command overrides"));
            }
            let mut plan = if let Some(p) = plan {
                serde_json::from_reader(std::fs::File::open(p)?)?
            } else {
                let p = program.ok_or_else(|| error("provide --plan or --program"))?;
                let program = |p| runner::Program {
                    path: p,
                    args: args.clone(),
                    env: Default::default(),
                    cwd: None,
                };
                runner::Plan {
                        privacy: None,
                        variants: Default::default(),
                        start_pair:0,
                    candidate: program(p),
                    baseline: baseline.map(program),
                    repetitions,
                    timeout_ms,
                    protocol,
                    fixtures: vec![],
                    contract: Default::default(),
                        provenance: Default::default(),
                }
            };
            if memory {
                if plan.baseline.is_some() || !plan.variants.is_empty() {return Err(error("memory profiling requires a single candidate"));}
                plan.repetitions=1;
                plan.candidate.env.insert("RBENCH_MEMORY_OUTPUT".into(), std::env::current_dir()?.join(&out).join("memory.json").to_string_lossy().into());
                plan.provenance.insert("session.policy".into(), "memory profiler; diagnostic only".into());
            }
            if dry_run {println!("{}",serde_json::to_string_pretty(&runner::preflight(plan,&out)?)?);return Ok(0);}
            runner::preflight(plan.clone(), &out)?;
            let live = !no_ui && (ui || std::io::stdout().is_terminal());
            if live {
                let url = web_ui::start_live(&out, &cli.store)?;
                println!("Live benchmark: {url}");
                if !no_open { web_ui::open_browser(&url); }
            }
            let result = runner::run(plan, &out);
            if out.join("run.json").is_file() {
                let doc = experiment_report::build(experiment_report::Options {source:&out,baseline:None,store:&cli.store,title:"Benchmark results",threshold:5.0,alpha:0.05})?;
                output(&doc.html()?, Some(out.join("report.html")))?;
                output(&serde_json::to_string_pretty(&doc)?, Some(out.join("report.json")))?;
                output(&doc.markdown()?, Some(out.join("report.md")))?;
                println!("Saved results and reports: {}", out.display());
            }
            if live && !runner::cancelled() && out.join("status-final.json").is_file() {
                println!("Interface remains available. Ctrl+C to close; results are already saved.");
                while !runner::cancelled() { std::thread::sleep(std::time::Duration::from_millis(100)); }
            }
            result?;
            if memory { rbench::memory::Profile::load(&out.join("memory.json"))?; }
        }
        Action::ImportForma {
            source,
            output: out,
        } => {
            let run = forma::import(&source)?;
            run.save_new(&out)?;
            println!(
                "Imported {} cases / {} observations → {}",
                run.cases.len(),
                run.observations.len(),
                out.display()
            );
        }
        Action::Serve {root,port} => web_ui::serve(&root, &cli.store, port)?,
        Action::Report {run,baseline,title,threshold,alpha,output:path} => {
            let doc=experiment_report::build(experiment_report::Options{source:&run,baseline:baseline.as_deref(),store:&cli.store,title:&title,threshold,alpha})?;
            let text=match path.as_ref().and_then(|p|p.extension()).and_then(|e|e.to_str()){
                Some("html")=>doc.html()?,Some("json")=>serde_json::to_string_pretty(&doc)?,Some("md")|None=>doc.markdown()?,_=>return Err(error("report output extension must be .html, .json or .md"))
            };output(&text,path)?;
        }
        Action::List { run, filter, exact, glob, exclude, tag } => {
            let mut r = load(run)?;
            select_cases(&mut r, &filter, exact, glob, &exclude, &tag)?;
            for c in r.cases {
                println!("{}", c.id);
            }
        }
        Action::Compare {
            baseline,
            candidate,
            threshold,
            alpha,
            json,
            uncertainty,
            check,
            filter,
            exact,
            glob,
            exclude,
            tag,
            metric,
            output: path,
        } => {
            let select = |mut r: Run| -> Result<Run> {
                select_cases(&mut r, &filter, exact, glob, &exclude, &tag)?;
                for c in &mut r.cases {
                    c.metrics.retain(|m| m.id.contains(&metric));
                }
                r.cases.retain(|c| !c.metrics.is_empty());
                r.observations.retain(|o| {
                    r.cases
                        .iter()
                        .any(|c| c.id == o.case && c.metrics.iter().any(|m| m.id == o.metric))
                });
                if r.cases.is_empty() {
                    return Err(error("filter matched no cases/metrics"));
                }
                r.validate()?;
                Ok(r)
            };
            let a = select(load(baseline)?)?;
            let b = candidate.map(|p| select(load(p)?)).transpose()?;
            let rows = analysis::compare(&a, b.as_ref(), threshold, alpha)?;
            let text = if json {
                serde_json::to_string_pretty(&rows)?
            } else {
                report::comparison(&rows)
            };
            output(&text, path)?;
            if check {
                if rows
                    .iter()
                    .any(|r| r.decision == analysis::Decision::Regression)
                {
                    return Ok(1);
                }
                if rows.iter().any(|r|r.decision==analysis::Decision::Unavailable){return Ok(2);}
                if rows.iter().any(|r|r.decision==analysis::Decision::Inconclusive){
                    if matches!(uncertainty,Uncertainty::Fail){return Ok(2);}
                    if matches!(uncertainty,Uncertainty::Warn){eprintln!("rbench: warning: inconclusive comparison allowed by explicit policy; no proof of equivalence");}
                }
            }
        }
        Action::Build {
            manifest_path,
            offline,
        } => {
            let mut c = Command::new("cargo");
            c.args(["bench", "--no-run"]);
            if let Some(p) = manifest_path {
                c.arg("--manifest-path").arg(p);
            }
            if offline {
                c.arg("--offline");
            }
            if !c.status()?.success() {
                return Err(error("Cargo benchmark build failed"));
            }
        }
        Action::Check { run, metric, max, min } => {
            if max.is_none() && min.is_none() {
                return Err(error("check requires --max and/or --min"));
            }
            if max.is_some_and(|v| !v.is_finite() || v < 0.0) || min.is_some_and(|v| !v.is_finite() || v < 0.0) {
                return Err(error("--max/--min must be finite and nonnegative"));
            }
            if let (Some(lo), Some(hi)) = (min, max) {
                if lo > hi {
                    return Err(error("--min cannot exceed --max"));
                }
            }
            let run = load(run)?;
            if run.status != Status::Complete {
                return Err(error("check requires complete run"));
            }
            let mut count = 0;
            let mut failed = false;
            for o in run.observations.iter().filter(|o| o.metric == metric) {
                count += 1;
                let mut value = o
                    .number()?
                    .ok_or_else(|| error("required metric unavailable"))?;
                let m = run
                    .cases
                    .iter()
                    .find(|c| c.id == o.case)
                    .and_then(|c| c.metrics.iter().find(|m| m.id == metric))
                    .ok_or_else(|| error("metric descriptor missing for observation"))?;
                if m.statistic == "batch_total" {
                    value /= o.operations as f64;
                }
                if max.is_some_and(|hi| value > hi) {
                    eprintln!(
                        "{} {} process {}: {value} {} > max {}",
                        o.case, o.variant, o.process, m.unit, max.unwrap()
                    );
                    failed = true;
                }
                if min.is_some_and(|lo| value < lo) {
                    eprintln!(
                        "{} {} process {}: {value} {} < min {}",
                        o.case, o.variant, o.process, m.unit, min.unwrap()
                    );
                    failed = true;
                }
            }
            if count == 0 {
                return Err(error("metric not found"));
            }
            println!(
                "Checked {count} observations; absolute bounds max={max:?} min={min:?}. This is a budget check, not a statistical comparison."
            );
            if failed {
                return Ok(1);
            }
        }
        Action::Accept {
            seed,
            hardware,
            pairs,
            trials,
            batch_iters,
            output,
        } => {
            let mut report = rbench::acceptance::battery(seed)?;
            if hardware {
                let aa = rbench::acceptance::hardware_aa(pairs, trials, 5.0, 0.05, batch_iters)?;
                if let Some(obj) = report.as_object_mut() {
                    obj.insert(
                        "hardware_aa".into(),
                        serde_json::to_value(aa)?,
                    );
                }
            }
            let text = serde_json::to_string_pretty(&report)?;
            if let Some(path) = output {
                rbench::publish::write_new_atomic(&path, &report)?;
                println!("wrote {}", path.display());
            } else {
                println!("{text}");
            }
        }
        Action::Recover { run } => {
            let report = rbench::publish::recover_report(&run)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Action::Doctor => {
            let os_metrics = match rbench::process::sample() {
                Ok(s) => format!("available (rss_bytes={:?} user_cpu_ns={:?})", s.rss_bytes, s.user_cpu_ns),
                Err(e) => format!("unsupported ({e})"),
            };
            let perf = rbench::perf::probe();
            let snap = rbench::isolate::snapshot();
            let warnings = rbench::isolate::noise_warnings(&snap);
            let pin = std::env::var("RBENCH_PIN_CPU").unwrap_or_else(|_| "(unset)".into());
            println!(
                "rbench {}\nOS: {}\nArch: {}\nClock: std::time::Instant\nProcess tree cleanup: {}\nOS RSS/CPU providers: {}\nperf_event: {:?} — {}\nIsolation snapshot: loadavg_1={:?} governor={:?} freq_khz={:?} pinned={:?}\nNoise warnings: {}\nRBENCH_PIN_CPU: {}\nGPU: supplied by scenario (not probed)\nWindow: supplied by scenario (not probed)\nIsolation: local runner lease + optional CPU pin (not BenchExec)\nStatistics: independent process units required\nAtomic publish: rename+fsync staging\nAccept: cargo rbench accept --seed N [--hardware]\nCompete: cargo rbench compete",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
                if cfg!(unix) {
                    "Unix process groups"
                } else {
                    "direct child only; descendants unsupported"
                },
                os_metrics,
                perf.availability,
                perf.note,
                snap.loadavg_1,
                snap.cpu_governor,
                snap.cpu_freq_khz,
                snap.pinned_cpu,
                if warnings.is_empty() {
                    "none".into()
                } else {
                    warnings.join("; ")
                },
                pin
            );
        }
        Action::Compete { output } => {
            let scorecard = compete_scorecard();
            let text = serde_json::to_string_pretty(&scorecard)?;
            if let Some(path) = output {
                rbench::publish::write_new_atomic(&path, &scorecard)?;
                println!("wrote {}", path.display());
            } else {
                println!("{text}");
            }
        }
    }
    Ok(0)
}
fn main() {
    runner::install_cancel_handler();
    match execute() {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("rbench: {e}");
            std::process::exit(2);
        }
    }
}
