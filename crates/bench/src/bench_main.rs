//! orbok-bench entry point.
//! Build and run on a machine with sufficient RAM (release binary).
//!
//! Usage:
//!   orbok-bench [N_DOCS] [OUTPUT_DIR]
//!   orbok-bench [N_DOCS] [OUTPUT_DIR] --model-dir <DIR>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("warn").init();

    let args = Args::parse(std::env::args().skip(1))?;
    let out_dir = args.out_dir;
    std::fs::create_dir_all(&out_dir)?;

    eprintln!(
        "orbok-bench: generating and indexing {} synthetic documents...",
        args.n_docs
    );
    let work_dir = tempfile::tempdir()?;
    let result = orbok_bench_lib::run_bench_with_options(
        args.n_docs,
        work_dir.path(),
        orbok_bench_lib::BenchmarkOptions {
            model_dir: args.model_dir,
        },
    )?;

    result.write_json(&out_dir.join("orbok-bench-results.json"))?;
    result.write_markdown(&out_dir.join("orbok-bench-report.md"))?;
    eprintln!("orbok-bench: results written to {}", out_dir.display());
    result.print_summary();

    Ok(())
}

struct Args {
    n_docs: usize,
    out_dir: std::path::PathBuf,
    model_dir: Option<std::path::PathBuf>,
}

impl Args {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut positional = Vec::new();
        let mut model_dir = None;
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--model-dir" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--model-dir requires a path".to_string())?;
                    model_dir = Some(std::path::PathBuf::from(value));
                }
                "--help" | "-h" => return Err(USAGE.to_string()),
                _ if arg.starts_with("--") => return Err(format!("unknown option: {arg}")),
                _ => positional.push(arg),
            }
        }

        let n_docs = positional
            .first()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);
        let out_dir = std::path::PathBuf::from(
            positional
                .get(1)
                .map(|s| s.as_str())
                .unwrap_or("target/orbok-bench/results"),
        );
        Ok(Self {
            n_docs,
            out_dir,
            model_dir,
        })
    }
}

const USAGE: &str = "Usage: orbok-bench [N_DOCS] [OUTPUT_DIR] [--model-dir DIR]";
