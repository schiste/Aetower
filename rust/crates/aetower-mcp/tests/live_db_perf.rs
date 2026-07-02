//! Temporary live-DB repro harness (ignored by default).
//!
//! Usage:
//!   AETOWER_REPRO_HOME=/tmp/aetower-repro/home \
//!   AETOWER_REPRO_ROOT_HOME=$HOME \
//!   cargo test -p aetower-mcp --release --test live_db_perf -- --ignored --nocapture

use std::time::Instant;

fn repro_roots(real_home: &str) -> Vec<String> {
    let rels = [
        "Repositories",
        "Documents",
        "Desktop",
        "Downloads",
        "Developer",
        "Projects",
        "Applications",
        "Library",
        "Library/Application Support",
        "Library/Containers",
        ".claude",
        ".codex",
        ".cursor",
        ".aider",
        ".cache",
        ".docker",
        ".npm",
        ".pnpm-store",
        ".cargo",
        "Library/Developer/Xcode/DerivedData",
        "Library/Developer/Xcode/Archives",
        "Library/Developer/CoreSimulator",
        "Library/Application Support/MobileSync/Backup",
        "Library/Mail",
        "Library/Messages/Attachments",
        "Library/Containers/com.docker.docker",
        "Library/Group Containers/group.com.docker",
        "Library/Caches/org.swift.swiftpm",
        "Library/Caches/com.apple.dt.Xcode",
        "Library/CloudStorage",
        "Library/Mobile Documents",
        "Dropbox",
        "OneDrive",
        "Google Drive",
    ];
    let mut roots: Vec<String> = rels
        .iter()
        .map(|rel| format!("{real_home}/{rel}"))
        .collect();
    roots.extend(
        ["/Applications", "/Library", "/Users/Shared"]
            .into_iter()
            .map(String::from),
    );
    roots
}

#[test]
#[ignore = "manual live-DB repro harness"]
fn live_db_indexed_and_overview_timing() {
    let repro_home = std::env::var("AETOWER_REPRO_HOME").expect("set AETOWER_REPRO_HOME");
    let real_home = std::env::var("AETOWER_REPRO_ROOT_HOME").expect("set AETOWER_REPRO_ROOT_HOME");
    // Redirect dirs::data_local_dir() to the fake home holding the DB copy.
    unsafe { std::env::set_var("HOME", &repro_home) };
    let roots = repro_roots(&real_home);

    for round in 1..=2 {
        let started = Instant::now();
        let indexed = aetower_mcp::storage_hygiene_indexed_json(roots.clone(), 6, 200);
        println!(
            "round {round} indexed: {:?} ok={} len={}",
            started.elapsed(),
            indexed.is_ok(),
            indexed.as_ref().map(String::len).unwrap_or_default()
        );
        if let Err(error) = &indexed {
            println!("indexed error: {error}");
        }

        let started = Instant::now();
        let overview =
            aetower_mcp::storage_hygiene_overview_json(roots.clone(), 6, "instant_cached");
        println!(
            "round {round} overview: {:?} ok={} len={}",
            started.elapsed(),
            overview.is_ok(),
            overview.as_ref().map(String::len).unwrap_or_default()
        );
        if let Err(error) = &overview {
            println!("overview error: {error}");
        }
    }
}
