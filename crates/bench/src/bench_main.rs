//! orbok-bench entry point.
//! Build and run on a machine with sufficient RAM (release binary).
//!
//! Usage: orbok-bench [N_DOCS] [OUTPUT_DIR]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("warn").init();

    let args: Vec<String> = std::env::args().collect();
    let n_docs: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let out_dir = std::path::PathBuf::from(
        args.get(2)
            .map(|s| s.as_str())
            .unwrap_or("target/orbok-bench/results"),
    );
    std::fs::create_dir_all(&out_dir)?;

    eprintln!("orbok-bench: generating and indexing {n_docs} synthetic documents...");
    let work_dir = tempfile::tempdir()?;
    let result = orbok_bench_lib::run_bench(n_docs, work_dir.path())?;

    result.write_json(&out_dir.join("orbok-bench-results.json"))?;
    result.write_markdown(&out_dir.join("orbok-bench-report.md"))?;
    eprintln!("orbok-bench: results written to {}", out_dir.display());
    result.print_summary();

    Ok(())
}
