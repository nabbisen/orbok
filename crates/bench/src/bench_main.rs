//! orbok-bench entry point.
//! Build and run on a machine with sufficient RAM (release binary).
//!
//! Usage:
//!   orbok-bench [N_DOCS] [OUTPUT_DIR]
//!   orbok-bench [N_DOCS] [OUTPUT_DIR] --model-dir <DIR>
//!   orbok-bench [N_DOCS] [OUTPUT_DIR] --expect-mode <keyword-only|hybrid-real-model>
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("warn").init();

    let Args {
        n_docs,
        out_dir,
        model_dir,
        expected_mode,
    } = Args::parse(std::env::args().skip(1))?;
    validate_expected_mode(expected_mode, model_dir.as_ref())?;
    std::fs::create_dir_all(&out_dir)?;

    eprintln!(
        "orbok-bench: generating and indexing {} synthetic documents...",
        n_docs
    );
    let work_dir = tempfile::tempdir()?;
    let result = orbok_bench_lib::run_bench_with_options(
        n_docs,
        work_dir.path(),
        orbok_bench_lib::BenchmarkOptions { model_dir },
    )?;
    if let Some(expected) = expected_mode {
        if result.mode != expected {
            return Err(format!(
                "benchmark mode mismatch: expected {}, got {}",
                expected.label(),
                result.mode.label()
            )
            .into());
        }
    }

    result.write_json(&out_dir.join("orbok-bench-results.json"))?;
    result.write_markdown(&out_dir.join("orbok-bench-report.md"))?;
    eprintln!("orbok-bench: results written to {}", out_dir.display());
    result.print_summary();

    Ok(())
}

#[derive(Debug)]
struct Args {
    n_docs: usize,
    out_dir: std::path::PathBuf,
    model_dir: Option<std::path::PathBuf>,
    expected_mode: Option<orbok_bench_lib::report::BenchmarkMode>,
}

impl Args {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut positional = Vec::new();
        let mut model_dir = None;
        let mut expected_mode = None;
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--model-dir" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--model-dir requires a path".to_string())?;
                    model_dir = Some(std::path::PathBuf::from(value));
                }
                "--expect-mode" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--expect-mode requires a value".to_string())?;
                    expected_mode = Some(parse_expected_mode(&value)?);
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
            expected_mode,
        })
    }
}

fn parse_expected_mode(value: &str) -> Result<orbok_bench_lib::report::BenchmarkMode, String> {
    match value {
        "keyword-only" => Ok(orbok_bench_lib::report::BenchmarkMode::KeywordOnly),
        "hybrid-real-model" => Ok(orbok_bench_lib::report::BenchmarkMode::HybridRealModel),
        _ => Err(format!(
            "unknown --expect-mode value: {value}; expected keyword-only or hybrid-real-model"
        )),
    }
}

fn validate_expected_mode(
    expected_mode: Option<orbok_bench_lib::report::BenchmarkMode>,
    model_dir: Option<&std::path::PathBuf>,
) -> Result<(), String> {
    if expected_mode == Some(orbok_bench_lib::report::BenchmarkMode::HybridRealModel)
        && model_dir.is_none()
    {
        return Err("--expect-mode hybrid-real-model requires --model-dir".to_string());
    }
    Ok(())
}

const USAGE: &str =
    "Usage: orbok-bench [N_DOCS] [OUTPUT_DIR] [--model-dir DIR] [--expect-mode MODE]";

#[cfg(test)]
mod tests {
    use super::*;
    use orbok_bench_lib::report::BenchmarkMode;

    #[test]
    fn parses_expected_mode() {
        let args = Args::parse([
            "1000".to_string(),
            "target/out".to_string(),
            "--expect-mode".to_string(),
            "keyword-only".to_string(),
        ])
        .unwrap();

        assert_eq!(args.n_docs, 1000);
        assert_eq!(args.out_dir, std::path::PathBuf::from("target/out"));
        assert_eq!(args.expected_mode, Some(BenchmarkMode::KeywordOnly));
    }

    #[test]
    fn hybrid_expectation_requires_model_dir() {
        let args =
            Args::parse(["--expect-mode".to_string(), "hybrid-real-model".to_string()]).unwrap();

        let err = validate_expected_mode(args.expected_mode, args.model_dir.as_ref()).unwrap_err();
        assert_eq!(err, "--expect-mode hybrid-real-model requires --model-dir");
    }

    #[test]
    fn rejects_unknown_expected_mode() {
        let err = Args::parse(["--expect-mode".to_string(), "semantic".to_string()]).unwrap_err();

        assert!(err.contains("unknown --expect-mode value: semantic"));
    }
}
