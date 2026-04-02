use aetower_core::{run_benchmark, BenchmarkConfig};

fn main() -> Result<(), String> {
    let config = parse_config(std::env::args().skip(1))?;
    let report = run_benchmark(config);
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("json encode failed: {error}"))?;
    println!("{json}");
    if config.enforce_default_budget && !report.default_budget_result.passed {
        return Err(format!(
            "benchmark exceeded default budget: {}",
            report.default_budget_result.violations.join("; ")
        ));
    }
    Ok(())
}

fn parse_config(args: impl Iterator<Item = String>) -> Result<BenchmarkConfig, String> {
    let mut config = BenchmarkConfig::default();
    let mut args = args.peekable();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--iterations" | "-n" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --iterations".to_owned())?;
                config.iterations = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid iteration count '{value}': {error}"))?;
            }
            "--warmup" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --warmup".to_owned())?;
                config.warmup_iterations = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid warmup count '{value}': {error}"))?;
            }
            "--enforce" | "--enforce-default-budget" => {
                config.enforce_default_budget = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                return Err(format!("unsupported argument: {other}"));
            }
        }
    }

    config.iterations = config.iterations.max(1);
    Ok(config)
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p aetower-bench --release -- [--iterations <count>] [--enforce]\n\
         Optional: [--warmup <count>]\n\
         Runs the core collection pipeline repeatedly, prints a JSON report, and optionally fails when the default performance budget is exceeded."
    );
}
