use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const GIT_COMMAND_TIMEOUT: Duration = Duration::from_millis(900);
const DEFAULT_SCORECARD_TIMEOUT_SECONDS: u64 = 30;
const MAX_SCORECARD_TIMEOUT_SECONDS: u64 = 120;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RepositoryScorecardReport {
    repo_root: String,
    remote_url: String,
    provider: String,
    owner: String,
    repo: String,
    mode: String,
    requested_mode: String,
    status: String,
    score: Option<f64>,
    checks: Vec<RepositoryScorecardCheck>,
    failed_checks: Vec<RepositoryScorecardCheck>,
    unavailable_checks: Vec<RepositoryScorecardCheck>,
    recommendations: Vec<RepositoryScorecardRecommendation>,
    commit_sha: Option<String>,
    scorecard_version: Option<String>,
    scorecard_commit: Option<String>,
    source_timestamp: Option<String>,
    cache_status: String,
    cache_key: Option<String>,
    cache_hit: bool,
    cached_at_millis: Option<u64>,
    captured_at_millis: u64,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RepositoryScorecardCheck {
    name: String,
    score: Option<f64>,
    outcome: String,
    reason: String,
    details: Vec<String>,
    documentation_url: Option<String>,
    documentation_short: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct RepositoryScorecardRecommendation {
    category: String,
    check_name: String,
    severity: String,
    actionability: String,
    title: String,
    detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitHubRemote {
    owner: String,
    repo: String,
}

trait ScorecardSource {
    fn fetch_public_api(&self, remote: &GitHubRemote, timeout: Duration) -> Result<String, String>;

    fn run_live_cli(&self, remote: &GitHubRemote, timeout: Duration) -> Result<String, String>;
}

struct CommandScorecardSource;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RepositoryScorecardCacheRecord {
    schema_version: u64,
    saved_at_millis: u64,
    repo_root: String,
    remote_url: String,
    commit_sha: Option<String>,
    requested_mode: String,
    effective_mode: String,
    scorecard_version: Option<String>,
    source_timestamp: Option<String>,
    report: RepositoryScorecardReport,
}

#[derive(Clone, Debug, Deserialize)]
struct RawScorecardReport {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    scorecard: Option<RawScorecardBuild>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    checks: Vec<RawScorecardCheck>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawScorecardBuild {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    commit: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawScorecardCheck {
    #[serde(default)]
    name: String,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    details: Vec<String>,
    #[serde(default)]
    documentation: Option<RawScorecardDocumentation>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawScorecardDocumentation {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    short: Option<String>,
}

pub fn repository_scorecard_json(repo_root: String, mode: &str) -> Result<String, String> {
    repository_scorecard_json_with_timeout(repo_root, mode, DEFAULT_SCORECARD_TIMEOUT_SECONDS)
}

pub fn repository_scorecard_json_with_timeout(
    repo_root: String,
    mode: &str,
    timeout_seconds: u64,
) -> Result<String, String> {
    repository_scorecard_json_cached(repo_root, mode, timeout_seconds, false)
}

pub fn repository_scorecard_json_cached(
    repo_root: String,
    mode: &str,
    timeout_seconds: u64,
    refresh: bool,
) -> Result<String, String> {
    let source = CommandScorecardSource;
    let cache_dir = default_scorecard_cache_dir();
    serde_json::to_string(&build_repository_scorecard_report_with_cache(
        repo_root,
        mode,
        timeout_from_seconds(timeout_seconds),
        refresh,
        cache_dir.as_deref(),
        &source,
    ))
    .map_err(|error| error.to_string())
}

pub fn repository_scorecard_json_from_scorecard_json(
    repo_root: String,
    remote_url: String,
    mode: &str,
    raw_json: &str,
) -> Result<String, String> {
    let report =
        repository_scorecard_report_from_scorecard_json(repo_root, remote_url, mode, raw_json)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_repository_scorecard_report(
    repo_root: String,
    mode: &str,
) -> RepositoryScorecardReport {
    let captured_at_millis = current_millis();
    let mut warnings = Vec::new();
    let path = Path::new(&repo_root);
    let normalized_mode = normalize_mode(mode);

    if !path.exists() {
        warnings.push("repo_root does not exist.".to_owned());
        return empty_report(
            repo_root,
            normalized_mode,
            "invalid_repo",
            captured_at_millis,
            warnings,
        );
    }
    if !path.is_dir() {
        warnings.push("repo_root is not a directory.".to_owned());
        return empty_report(
            repo_root,
            normalized_mode,
            "invalid_repo",
            captured_at_millis,
            warnings,
        );
    }

    let remote_url = match git_remote_origin(path) {
        Ok(remote) => remote,
        Err(error) => {
            warnings.push(error);
            String::new()
        }
    };
    let commit_sha = match git_commit_sha(path) {
        Ok(sha) => Some(sha),
        Err(error) => {
            warnings.push(error);
            None
        }
    };

    let github = parse_github_remote(&remote_url);
    let (provider, owner, repo) = if let Some(remote) = github {
        ("github".to_owned(), remote.owner, remote.repo)
    } else {
        if !remote_url.is_empty() {
            warnings.push("origin remote is not a supported github.com repository.".to_owned());
        }
        (String::new(), String::new(), String::new())
    };
    let status = if provider == "github" {
        "not_scanned"
    } else {
        "unsupported"
    };

    RepositoryScorecardReport {
        repo_root,
        remote_url,
        provider,
        owner,
        repo,
        mode: normalized_mode.clone(),
        requested_mode: normalized_mode,
        status: status.to_owned(),
        score: None,
        checks: Vec::new(),
        failed_checks: Vec::new(),
        unavailable_checks: Vec::new(),
        recommendations: Vec::new(),
        commit_sha,
        scorecard_version: None,
        scorecard_commit: None,
        source_timestamp: None,
        cache_status: "not_used".to_owned(),
        cache_key: None,
        cache_hit: false,
        cached_at_millis: None,
        captured_at_millis,
        warnings,
    }
}

fn repository_scorecard_report_from_scorecard_json(
    repo_root: String,
    remote_url: String,
    mode: &str,
    raw_json: &str,
) -> Result<RepositoryScorecardReport, String> {
    let raw: RawScorecardReport = serde_json::from_str(raw_json)
        .map_err(|error| format!("scorecard_json_failed: {error}"))?;
    let RawScorecardReport {
        date,
        scorecard,
        score,
        checks: raw_checks,
    } = raw;
    let github = parse_github_remote(&remote_url);
    let (provider, owner, repo) = if let Some(remote) = github {
        ("github".to_owned(), remote.owner, remote.repo)
    } else {
        (String::new(), String::new(), String::new())
    };
    let checks = raw_checks
        .into_iter()
        .filter(|check| !check.name.trim().is_empty())
        .map(normalize_scorecard_check)
        .collect::<Vec<_>>();
    let failed_checks = checks
        .iter()
        .filter(|check| check.outcome == "failed")
        .cloned()
        .collect::<Vec<_>>();
    let unavailable_checks = checks
        .iter()
        .filter(|check| check.outcome == "unavailable")
        .cloned()
        .collect::<Vec<_>>();
    let recommendations = recommendations_for_checks(&failed_checks);

    Ok(RepositoryScorecardReport {
        repo_root,
        remote_url,
        provider,
        owner,
        repo,
        mode: normalize_mode(mode),
        requested_mode: normalize_mode(mode),
        status: "ok".to_owned(),
        score,
        checks,
        failed_checks,
        unavailable_checks,
        recommendations,
        commit_sha: None,
        scorecard_version: scorecard.as_ref().and_then(|value| value.version.clone()),
        scorecard_commit: scorecard.and_then(|value| value.commit),
        source_timestamp: date,
        cache_status: "not_used".to_owned(),
        cache_key: None,
        cache_hit: false,
        cached_at_millis: None,
        captured_at_millis: current_millis(),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
fn build_repository_scorecard_report_with_source(
    repo_root: String,
    mode: &str,
    timeout: Duration,
    source: &dyn ScorecardSource,
) -> RepositoryScorecardReport {
    build_repository_scorecard_report_with_cache(repo_root, mode, timeout, true, None, source)
}

fn build_repository_scorecard_report_with_cache(
    repo_root: String,
    mode: &str,
    timeout: Duration,
    refresh: bool,
    cache_dir: Option<&Path>,
    source: &dyn ScorecardSource,
) -> RepositoryScorecardReport {
    let normalized_mode = normalize_mode(mode);
    let mut base = build_repository_scorecard_report(repo_root, &normalized_mode);
    if base.provider != "github" || base.owner.is_empty() || base.repo.is_empty() {
        return base;
    }

    let lookup_key = scorecard_cache_lookup_key(&base, &normalized_mode);
    if let Some(cache_dir) = cache_dir {
        base.cache_key = Some(lookup_key.clone());
        if !refresh {
            match read_scorecard_cache(cache_dir, &lookup_key) {
                Some(mut cached) => {
                    cached.cache_status = "hit".to_owned();
                    cached.cache_hit = true;
                    return cached;
                }
                None => {
                    base.cache_status = "miss".to_owned();
                }
            }
        } else {
            base.cache_status = "refresh".to_owned();
        }
    }

    let remote = GitHubRemote {
        owner: base.owner.clone(),
        repo: base.repo.clone(),
    };

    let mut report = match normalized_mode.as_str() {
        "public_api" => match source.fetch_public_api(&remote, timeout) {
            Ok(raw_json) => merge_source_report(&base, "public_api", &raw_json),
            Err(error) => {
                base.warnings
                    .push(format!("public_api source failed: {error}"));
                base.status = source_error_status(&error);
                base
            }
        },
        "live_cli" => match source.run_live_cli(&remote, timeout) {
            Ok(raw_json) => merge_source_report(&base, "live_cli", &raw_json),
            Err(error) => {
                base.warnings
                    .push(format!("live_cli source failed: {error}"));
                base.status = source_error_status(&error);
                base
            }
        },
        _ => match source.fetch_public_api(&remote, timeout) {
            Ok(raw_json) => merge_source_report(&base, "public_api", &raw_json),
            Err(api_error) => {
                base.warnings
                    .push(format!("public_api source failed: {api_error}"));
                match source.run_live_cli(&remote, timeout) {
                    Ok(raw_json) => merge_source_report(&base, "live_cli", &raw_json),
                    Err(cli_error) => {
                        base.warnings
                            .push(format!("live_cli source failed: {cli_error}"));
                        base.status = source_error_status(&cli_error);
                        base
                    }
                }
            }
        },
    };

    report.requested_mode = normalized_mode;
    if report.cache_key.is_none() {
        report.cache_key = Some(result_cache_key(&report));
    }
    if let Some(cache_dir) = cache_dir
        && report.status == "ok"
    {
        let saved_at_millis = current_millis();
        report.cached_at_millis = Some(saved_at_millis);
        report.cache_status = "saved".to_owned();
        report.cache_status =
            match write_scorecard_cache(cache_dir, &lookup_key, saved_at_millis, &report) {
                Ok(()) => "saved".to_owned(),
                Err(error) => {
                    report.warnings.push(format!("cache write failed: {error}"));
                    "write_failed".to_owned()
                }
            };
    }
    report
}

fn merge_source_report(
    base: &RepositoryScorecardReport,
    mode: &str,
    raw_json: &str,
) -> RepositoryScorecardReport {
    match repository_scorecard_report_from_scorecard_json(
        base.repo_root.clone(),
        base.remote_url.clone(),
        mode,
        raw_json,
    ) {
        Ok(mut report) => {
            report.warnings = base.warnings.clone();
            report.requested_mode = base.requested_mode.clone();
            report.commit_sha = base.commit_sha.clone();
            report.cache_status = base.cache_status.clone();
            report.cache_key = Some(result_cache_key(&report));
            report
        }
        Err(error) => {
            let mut report = base.clone();
            report.mode = mode.to_owned();
            report.status = "parse_error".to_owned();
            report
                .warnings
                .push(format!("{mode} response parse failed: {error}"));
            report
        }
    }
}

fn empty_report(
    repo_root: String,
    mode: String,
    status: &str,
    captured_at_millis: u64,
    warnings: Vec<String>,
) -> RepositoryScorecardReport {
    RepositoryScorecardReport {
        repo_root,
        remote_url: String::new(),
        provider: String::new(),
        owner: String::new(),
        repo: String::new(),
        mode: mode.clone(),
        requested_mode: mode,
        status: status.to_owned(),
        score: None,
        checks: Vec::new(),
        failed_checks: Vec::new(),
        unavailable_checks: Vec::new(),
        recommendations: Vec::new(),
        commit_sha: None,
        scorecard_version: None,
        scorecard_commit: None,
        source_timestamp: None,
        cache_status: "not_used".to_owned(),
        cache_key: None,
        cache_hit: false,
        cached_at_millis: None,
        captured_at_millis,
        warnings,
    }
}

fn normalize_scorecard_check(raw: RawScorecardCheck) -> RepositoryScorecardCheck {
    let score = raw.score;
    let outcome = match score {
        Some(value) if value < 0.0 => "unavailable",
        Some(value) if value >= 10.0 => "passed",
        Some(_) => "failed",
        None => "unavailable",
    }
    .to_owned();
    let documentation = raw.documentation;

    RepositoryScorecardCheck {
        name: raw.name,
        score,
        outcome,
        reason: raw.reason.unwrap_or_default(),
        details: raw.details,
        documentation_url: documentation.as_ref().and_then(|value| value.url.clone()),
        documentation_short: documentation.and_then(|value| value.short),
    }
}

fn recommendations_for_checks(
    failed_checks: &[RepositoryScorecardCheck],
) -> Vec<RepositoryScorecardRecommendation> {
    failed_checks
        .iter()
        .map(|check| recommendation_for_check(&check.name))
        .collect()
}

fn recommendation_for_check(check_name: &str) -> RepositoryScorecardRecommendation {
    let normalized = normalize_check_name(check_name);
    match normalized.as_str() {
        "security-policy" => recommendation(
            "Security-Policy",
            check_name,
            "medium",
            "local",
            "Add or update the security policy",
            "Provide a tracked SECURITY.md with vulnerability reporting instructions.",
        ),
        "token-permissions" => recommendation(
            "Token-Permissions",
            check_name,
            "high",
            "mixed",
            "Restrict automation token permissions",
            "Set least-privilege GitHub Actions permissions in workflow files and review repository defaults.",
        ),
        "branch-protection" => recommendation(
            "Branch-Protection",
            check_name,
            "medium",
            "remote",
            "Require protected default-branch workflows",
            "Configure branch protection or repository rules for review, status checks, and force-push restrictions.",
        ),
        "code-review" => recommendation(
            "Code-Review",
            check_name,
            "medium",
            "remote",
            "Require review before merging",
            "Configure pull request review requirements for sensitive or default-branch changes.",
        ),
        "pinned-dependencies" => recommendation(
            "Pinned-Dependencies",
            check_name,
            "medium",
            "local",
            "Pin automation dependencies",
            "Pin GitHub Actions and other automation dependencies to immutable versions where practical.",
        ),
        "dangerous-workflow" => recommendation(
            "Dangerous-Workflow",
            check_name,
            "high",
            "local",
            "Remove dangerous workflow patterns",
            "Audit GitHub Actions for unsafe pull_request_target, untrusted checkout, or shell interpolation patterns.",
        ),
        "vulnerabilities" => recommendation(
            "Vulnerabilities",
            check_name,
            "high",
            "local",
            "Resolve known dependency vulnerabilities",
            "Run the repository's dependency audit tooling and update vulnerable packages.",
        ),
        "sast" => recommendation(
            "SAST",
            check_name,
            "medium",
            "local",
            "Enable static analysis",
            "Add or document static analysis in CI so security findings are checked continuously.",
        ),
        "fuzzing" => recommendation(
            "Fuzzing",
            check_name,
            "low",
            "local",
            "Document fuzzing coverage",
            "Add fuzz targets for parser or boundary-heavy code, or document why fuzzing is not applicable.",
        ),
        "maintained" => recommendation(
            "Maintained",
            check_name,
            "medium",
            "mixed",
            "Refresh repository maintenance signals",
            "Review recent releases, issue handling, and default branch activity.",
        ),
        _ => recommendation(
            "Scorecard",
            check_name,
            "low",
            "mixed",
            "Review the failed Scorecard check",
            "Inspect the Scorecard finding and decide whether it requires local edits or repository settings.",
        ),
    }
}

fn recommendation(
    category: &str,
    check_name: &str,
    severity: &str,
    actionability: &str,
    title: &str,
    detail: &str,
) -> RepositoryScorecardRecommendation {
    RepositoryScorecardRecommendation {
        category: category.to_owned(),
        check_name: check_name.to_owned(),
        severity: severity.to_owned(),
        actionability: actionability.to_owned(),
        title: title.to_owned(),
        detail: detail.to_owned(),
    }
}

fn normalize_check_name(check_name: &str) -> String {
    check_name
        .trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-")
}

fn normalize_mode(mode: &str) -> String {
    match mode.trim().to_ascii_lowercase().as_str() {
        "public_api" | "public-api" => "public_api",
        "live_cli" | "live-cli" => "live_cli",
        "auto" => "auto",
        _ => "auto",
    }
    .to_owned()
}

fn git_remote_origin(repo_root: &Path) -> Result<String, String> {
    run_git(repo_root, &["remote", "get-url", "origin"])
        .map(|output| normalize_remote_url(output.trim()))
        .and_then(|output| {
            if output.is_empty() {
                Err("origin remote is empty.".to_owned())
            } else {
                Ok(output)
            }
        })
}

fn git_commit_sha(repo_root: &Path) -> Result<String, String> {
    run_git(repo_root, &["rev-parse", "HEAD"])
        .map(|output| output.trim().to_owned())
        .and_then(|output| {
            if output.is_empty() {
                Err("git HEAD commit is empty.".to_owned())
            } else {
                Ok(output)
            }
        })
}

fn default_scorecard_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|path| path.join("Aetower").join("repository-scorecard").join("v1"))
}

fn read_scorecard_cache(cache_dir: &Path, lookup_key: &str) -> Option<RepositoryScorecardReport> {
    let path = scorecard_cache_file(cache_dir, lookup_key);
    let content = fs::read_to_string(path).ok()?;
    let record: RepositoryScorecardCacheRecord = serde_json::from_str(&content).ok()?;
    if record.schema_version != 1 {
        return None;
    }
    let mut report = record.report;
    if report.cached_at_millis.is_none() {
        report.cached_at_millis = Some(record.saved_at_millis);
    }
    Some(report)
}

fn write_scorecard_cache(
    cache_dir: &Path,
    lookup_key: &str,
    saved_at_millis: u64,
    report: &RepositoryScorecardReport,
) -> Result<(), String> {
    fs::create_dir_all(cache_dir).map_err(|error| error.to_string())?;
    let record = RepositoryScorecardCacheRecord {
        schema_version: 1,
        saved_at_millis,
        repo_root: report.repo_root.clone(),
        remote_url: report.remote_url.clone(),
        commit_sha: report.commit_sha.clone(),
        requested_mode: report.requested_mode.clone(),
        effective_mode: report.mode.clone(),
        scorecard_version: report.scorecard_version.clone(),
        source_timestamp: report.source_timestamp.clone(),
        report: report.clone(),
    };
    let json = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
    fs::write(scorecard_cache_file(cache_dir, lookup_key), json).map_err(|error| error.to_string())
}

fn scorecard_cache_file(cache_dir: &Path, lookup_key: &str) -> PathBuf {
    cache_dir.join(format!("{lookup_key}.json"))
}

fn scorecard_cache_lookup_key(report: &RepositoryScorecardReport, requested_mode: &str) -> String {
    stable_hash_hex(&[
        "repository-scorecard-v1",
        &report.repo_root,
        &report.remote_url,
        report.commit_sha.as_deref().unwrap_or("no-commit"),
        requested_mode,
    ])
}

fn result_cache_key(report: &RepositoryScorecardReport) -> String {
    stable_hash_hex(&[
        "repository-scorecard-result-v1",
        &report.repo_root,
        &report.remote_url,
        report.commit_sha.as_deref().unwrap_or("no-commit"),
        &report.mode,
        report
            .scorecard_version
            .as_deref()
            .unwrap_or("no-scorecard-version"),
        report
            .source_timestamp
            .as_deref()
            .unwrap_or("no-source-timestamp"),
    ])
}

fn stable_hash_hex(parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn source_error_status(error: &str) -> String {
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("timed out") || lowered.contains("timeout") {
        "timeout"
    } else if lowered.contains("401")
        || lowered.contains("403")
        || lowered.contains("authentication")
        || lowered.contains("authorization")
        || lowered.contains("credential")
        || lowered.contains("bad credentials")
    {
        "auth_required"
    } else if lowered.contains("404") || lowered.contains("not found") {
        "unsupported"
    } else if lowered.contains("429") || lowered.contains("rate limit") {
        "rate_limited"
    } else {
        "source_unavailable"
    }
    .to_owned()
}

fn normalize_remote_url(remote: &str) -> String {
    let trimmed = remote.trim();
    if let Some(rest) = trimmed.strip_prefix("https://")
        && let Some((_, after_credentials)) = rest.split_once('@')
        && after_credentials.starts_with("github.com/")
    {
        return format!("https://{after_credentials}");
    }
    if let Some(rest) = trimmed.strip_prefix("http://")
        && let Some((_, after_credentials)) = rest.split_once('@')
        && after_credentials.starts_with("github.com/")
    {
        return format!("http://{after_credentials}");
    }
    trimmed.to_owned()
}

impl ScorecardSource for CommandScorecardSource {
    fn fetch_public_api(&self, remote: &GitHubRemote, timeout: Duration) -> Result<String, String> {
        let timeout_secs = command_timeout_seconds(timeout).to_string();
        let connect_timeout_secs = command_timeout_seconds(timeout.min(Duration::from_secs(10)))
            .max(1)
            .to_string();
        run_command_stdout(
            "/usr/bin/curl",
            &[
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--max-time",
                timeout_secs.as_str(),
                "--connect-timeout",
                connect_timeout_secs.as_str(),
                "--user-agent",
                "Aetower-Repository-Scorecard/1",
                scorecard_api_project_url(remote).as_str(),
            ],
            timeout,
            "scorecard public API",
        )
    }

    fn run_live_cli(&self, remote: &GitHubRemote, timeout: Duration) -> Result<String, String> {
        let repo = format!("github.com/{}/{}", remote.owner, remote.repo);
        let repo_arg = format!("--repo={repo}");
        run_command_stdout(
            "scorecard",
            &[repo_arg.as_str(), "--show-details", "--format=json"],
            timeout,
            "scorecard CLI",
        )
    }
}

fn scorecard_api_project_url(remote: &GitHubRemote) -> String {
    format!(
        "https://api.scorecard.dev/projects/github.com/{}/{}",
        remote.owner, remote.repo
    )
}

fn timeout_from_seconds(timeout_seconds: u64) -> Duration {
    Duration::from_secs(timeout_seconds.clamp(1, MAX_SCORECARD_TIMEOUT_SECONDS))
}

fn command_timeout_seconds(timeout: Duration) -> u64 {
    timeout.as_secs().clamp(1, MAX_SCORECARD_TIMEOUT_SECONDS)
}

fn run_command_stdout(
    program: &str,
    args: &[&str],
    timeout: Duration,
    context: &str,
) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{context} unavailable: {error}"))?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|error| format!("{context} output unavailable: {error}"))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if status.success() {
                    if stdout.trim().is_empty() {
                        return Err(format!("{context} returned empty output"));
                    }
                    return Ok(stdout);
                }
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                let detail = if stderr.is_empty() {
                    truncate_detail(stdout.trim())
                } else {
                    truncate_detail(&stderr)
                };
                return Err(if detail.is_empty() {
                    format!("{context} exited with status {status}")
                } else {
                    format!("{context} exited with status {status}: {detail}")
                });
            }
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(40));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{context} timed out after {}s",
                    command_timeout_seconds(timeout)
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{context} failed: {error}"));
            }
        }
    }
}

fn truncate_detail(value: &str) -> String {
    const MAX_DETAIL_CHARS: usize = 700;
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_DETAIL_CHARS {
        trimmed.to_owned()
    } else {
        let prefix = trimmed.chars().take(MAX_DETAIL_CHARS).collect::<String>();
        format!("{prefix}...")
    }
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<String, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("git command unavailable: {error}"))?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|error| format!("git output unavailable: {error}"))?;
                if status.success() {
                    return Ok(String::from_utf8_lossy(&output.stdout).to_string());
                }
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                return Err(if stderr.is_empty() {
                    "git remote lookup failed.".to_owned()
                } else {
                    format!("git remote lookup failed: {stderr}")
                });
            }
            Ok(None) if started.elapsed() < GIT_COMMAND_TIMEOUT => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git remote lookup timed out.".to_owned());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("git remote lookup failed: {error}"));
            }
        }
    }
}

fn parse_github_remote(remote_url: &str) -> Option<GitHubRemote> {
    let mut remote = remote_url.trim();
    if remote.is_empty() {
        return None;
    }
    if let Some((prefix, _)) = remote.split_once('?') {
        remote = prefix;
    }
    if let Some((prefix, _)) = remote.split_once('#') {
        remote = prefix;
    }

    let path = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("http://github.com/"))
        .or_else(|| remote.strip_prefix("git://github.com/"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("git@github.com:"))
        .or_else(|| remote.strip_prefix("https://www.github.com/"))
        .or_else(|| remote.strip_prefix("http://www.github.com/"))?;
    github_owner_repo_from_path(path)
}

fn github_owner_repo_from_path(path: &str) -> Option<GitHubRemote> {
    let trimmed = path.trim_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let mut parts = without_git.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(GitHubRemote {
        owner: owner.to_owned(),
        repo: repo.to_owned(),
    })
}

fn current_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        env, fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct FakeScorecardSource {
        public_api_result: Result<String, String>,
        live_cli_result: Result<String, String>,
        calls: RefCell<Vec<&'static str>>,
    }

    impl FakeScorecardSource {
        fn new(public_api_result: Result<&str, &str>, live_cli_result: Result<&str, &str>) -> Self {
            Self {
                public_api_result: public_api_result.map(str::to_owned).map_err(str::to_owned),
                live_cli_result: live_cli_result.map(str::to_owned).map_err(str::to_owned),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl ScorecardSource for FakeScorecardSource {
        fn fetch_public_api(
            &self,
            _remote: &GitHubRemote,
            _timeout: Duration,
        ) -> Result<String, String> {
            self.calls.borrow_mut().push("public_api");
            self.public_api_result.clone()
        }

        fn run_live_cli(
            &self,
            _remote: &GitHubRemote,
            _timeout: Duration,
        ) -> Result<String, String> {
            self.calls.borrow_mut().push("live_cli");
            self.live_cli_result.clone()
        }
    }

    #[test]
    fn parses_common_github_remotes() {
        let cases = [
            ("https://github.com/owner/repo.git", "owner", "repo"),
            ("https://github.com/owner/repo", "owner", "repo"),
            ("git@github.com:owner/repo.git", "owner", "repo"),
            ("ssh://git@github.com/owner/repo.git", "owner", "repo"),
            ("git://github.com/owner/repo", "owner", "repo"),
        ];

        for (remote, owner, repo) in cases {
            let parsed = match parse_github_remote(remote) {
                Some(parsed) => parsed,
                None => panic!("remote should parse: {remote}"),
            };
            assert_eq!(parsed.owner, owner);
            assert_eq!(parsed.repo, repo);
        }
    }

    #[test]
    fn rejects_non_github_remotes() {
        assert!(parse_github_remote("https://gitlab.com/owner/repo.git").is_none());
        assert!(parse_github_remote("git@example.com:owner/repo.git").is_none());
        assert!(parse_github_remote("https://github.com/owner/repo/extra").is_none());
    }

    #[test]
    fn invalid_repo_returns_clean_status() {
        let root = unique_test_dir("repository-scorecard-missing");
        let report = build_repository_scorecard_report(root.display().to_string(), "auto");

        assert_eq!(report.status, "invalid_repo");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("does not exist"))
        );
    }

    #[test]
    fn unsupported_remote_returns_clean_status() {
        let root = test_git_repo_with_remote("https://gitlab.com/example/widgets.git");
        let report = build_repository_scorecard_report(root.display().to_string(), "auto");

        assert_eq!(report.status, "unsupported");
        assert_eq!(report.remote_url, "https://gitlab.com/example/widgets.git");
        assert!(report.owner.is_empty());
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("not a supported github.com repository"))
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn builds_scorecard_api_project_url() {
        let remote = GitHubRemote {
            owner: "ossf".to_owned(),
            repo: "scorecard".to_owned(),
        };
        assert_eq!(
            scorecard_api_project_url(&remote),
            "https://api.scorecard.dev/projects/github.com/ossf/scorecard"
        );
    }

    #[test]
    fn normalizes_scorecard_json_into_report() {
        let raw = r#"{
          "date": "2026-06-29",
          "scorecard": {
            "version": "v5.0.0",
            "commit": "abc123"
          },
          "score": 6.4,
          "checks": [
            {
              "name": "Token-Permissions",
              "score": 0,
              "reason": "workflow tokens are overly broad",
              "details": ["permissions are not set"],
              "documentation": {
                "url": "https://example.com/token",
                "short": "Token permissions"
              }
            },
            {
              "name": "Maintained",
              "score": -1,
              "reason": "not enough activity"
            },
            {
              "name": "Security-Policy",
              "score": 10,
              "reason": "policy found"
            }
          ]
        }"#;

        let report = match repository_scorecard_report_from_scorecard_json(
            "/tmp/repo".to_owned(),
            "git@github.com:ossf/scorecard.git".to_owned(),
            "public_api",
            raw,
        ) {
            Ok(report) => report,
            Err(error) => panic!("report parses: {error}"),
        };

        assert_eq!(report.provider, "github");
        assert_eq!(report.owner, "ossf");
        assert_eq!(report.repo, "scorecard");
        assert_eq!(report.mode, "public_api");
        assert_eq!(report.status, "ok");
        assert_eq!(report.score, Some(6.4));
        assert_eq!(report.scorecard_version.as_deref(), Some("v5.0.0"));
        assert_eq!(report.scorecard_commit.as_deref(), Some("abc123"));
        assert_eq!(report.source_timestamp.as_deref(), Some("2026-06-29"));
        assert_eq!(report.checks.len(), 3);
        assert_eq!(report.failed_checks.len(), 1);
        assert_eq!(report.failed_checks[0].name, "Token-Permissions");
        assert_eq!(report.unavailable_checks.len(), 1);
        assert_eq!(report.recommendations.len(), 1);
        assert_eq!(report.recommendations[0].category, "Token-Permissions");
        assert_eq!(report.recommendations[0].severity, "high");
    }

    #[test]
    fn builds_repository_shell_from_git_remote() {
        let root = test_git_repo_with_remote("https://github.com/example/widgets.git");

        let report = build_repository_scorecard_report(root.display().to_string(), "live-cli");
        assert_eq!(report.provider, "github");
        assert_eq!(report.owner, "example");
        assert_eq!(report.repo, "widgets");
        assert_eq!(report.mode, "live_cli");
        assert!(report.score.is_none());
        assert!(report.checks.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn public_api_mode_uses_public_api_source_only() {
        let root = test_git_repo_with_remote("https://github.com/example/widgets.git");
        let source = FakeScorecardSource::new(
            Ok(token_permissions_scorecard_json()),
            Err("cli should not be called"),
        );

        let report = build_repository_scorecard_report_with_source(
            root.display().to_string(),
            "public_api",
            Duration::from_secs(5),
            &source,
        );

        assert_eq!(*source.calls.borrow(), vec!["public_api"]);
        assert_eq!(report.mode, "public_api");
        assert_eq!(report.status, "ok");
        assert_eq!(report.score, Some(6.4));
        assert_eq!(report.failed_checks.len(), 1);
        assert_eq!(report.failed_checks[0].name, "Token-Permissions");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn live_cli_mode_uses_cli_source_only() {
        let root = test_git_repo_with_remote("git@github.com:example/widgets.git");
        let source =
            FakeScorecardSource::new(Ok("api should not be called"), Ok(sast_scorecard_json()));

        let report = build_repository_scorecard_report_with_source(
            root.display().to_string(),
            "live_cli",
            Duration::from_secs(5),
            &source,
        );

        assert_eq!(*source.calls.borrow(), vec!["live_cli"]);
        assert_eq!(report.mode, "live_cli");
        assert_eq!(report.failed_checks.len(), 1);
        assert_eq!(report.recommendations[0].category, "SAST");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parses_live_cli_json_into_report() {
        let report = match repository_scorecard_report_from_scorecard_json(
            "/tmp/repo".to_owned(),
            "ssh://git@github.com/example/widgets.git".to_owned(),
            "live_cli",
            sast_scorecard_json(),
        ) {
            Ok(report) => report,
            Err(error) => panic!("cli report parses: {error}"),
        };

        assert_eq!(report.provider, "github");
        assert_eq!(report.owner, "example");
        assert_eq!(report.repo, "widgets");
        assert_eq!(report.mode, "live_cli");
        assert_eq!(report.status, "ok");
        assert_eq!(report.failed_checks.len(), 1);
        assert_eq!(report.failed_checks[0].name, "SAST");
        assert_eq!(report.recommendations[0].category, "SAST");
    }

    #[test]
    fn auto_mode_falls_back_to_live_cli_when_public_api_fails() {
        let root = test_git_repo_with_remote("https://github.com/example/widgets.git");
        let source = FakeScorecardSource::new(Err("api unavailable"), Ok(sast_scorecard_json()));

        let report = build_repository_scorecard_report_with_source(
            root.display().to_string(),
            "auto",
            Duration::from_secs(5),
            &source,
        );

        assert_eq!(*source.calls.borrow(), vec!["public_api", "live_cli"]);
        assert_eq!(report.mode, "live_cli");
        assert_eq!(report.failed_checks[0].name, "SAST");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("public_api source failed"))
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn timeout_errors_return_timeout_status() {
        let root = test_git_repo_with_remote("https://github.com/example/widgets.git");
        let source = FakeScorecardSource::new(Err("operation timed out"), Err("not called"));

        let report = build_repository_scorecard_report_with_source(
            root.display().to_string(),
            "public_api",
            Duration::from_millis(1),
            &source,
        );

        assert_eq!(*source.calls.borrow(), vec!["public_api"]);
        assert_eq!(report.status, "timeout");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("public_api source failed"))
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scorecard_cache_reuses_successful_result_until_refresh() {
        let root = test_git_repo_with_remote("https://github.com/example/widgets.git");
        commit_test_file(&root);
        let cache_dir = unique_test_dir("repository-scorecard-cache");
        let source = FakeScorecardSource::new(
            Ok(token_permissions_scorecard_json()),
            Err("cli should not be called"),
        );

        let first = build_repository_scorecard_report_with_cache(
            root.display().to_string(),
            "public_api",
            Duration::from_secs(5),
            false,
            Some(&cache_dir),
            &source,
        );

        assert_eq!(*source.calls.borrow(), vec!["public_api"]);
        assert_eq!(first.status, "ok");
        assert_eq!(first.cache_status, "saved");
        assert!(!first.cache_hit);
        assert!(first.cache_key.is_some());
        assert!(first.commit_sha.is_some());
        assert_eq!(first.scorecard_version.as_deref(), Some("v5.0.0"));
        assert_eq!(first.source_timestamp.as_deref(), Some("2026-06-29"));

        let cached_source =
            FakeScorecardSource::new(Err("api should not be called"), Err("no cli"));
        let second = build_repository_scorecard_report_with_cache(
            root.display().to_string(),
            "public_api",
            Duration::from_secs(5),
            false,
            Some(&cache_dir),
            &cached_source,
        );

        assert!(cached_source.calls.borrow().is_empty());
        assert_eq!(second.cache_status, "hit");
        assert!(second.cache_hit);
        assert_eq!(second.score, Some(6.4));

        let refreshed_source =
            FakeScorecardSource::new(Ok(sast_scorecard_json()), Err("cli should not be called"));
        let refreshed = build_repository_scorecard_report_with_cache(
            root.display().to_string(),
            "public_api",
            Duration::from_secs(5),
            true,
            Some(&cache_dir),
            &refreshed_source,
        );

        assert_eq!(*refreshed_source.calls.borrow(), vec!["public_api"]);
        assert_eq!(refreshed.cache_status, "saved");
        assert_eq!(refreshed.failed_checks[0].name, "SAST");

        let _ = fs::remove_dir_all(&cache_dir);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cache_lookup_key_changes_by_remote_commit_and_requested_mode() {
        let mut report = empty_report(
            "/tmp/repo".to_owned(),
            "public_api".to_owned(),
            "ok",
            123,
            Vec::new(),
        );
        report.remote_url = "https://github.com/example/widgets.git".to_owned();
        report.commit_sha = Some("commit-a".to_owned());

        let original = scorecard_cache_lookup_key(&report, "public_api");
        let mode_changed = scorecard_cache_lookup_key(&report, "live_cli");
        report.commit_sha = Some("commit-b".to_owned());
        let commit_changed = scorecard_cache_lookup_key(&report, "public_api");
        report.commit_sha = Some("commit-a".to_owned());
        report.remote_url = "https://github.com/example/other.git".to_owned();
        let remote_changed = scorecard_cache_lookup_key(&report, "public_api");

        assert_ne!(original, mode_changed);
        assert_ne!(original, commit_changed);
        assert_ne!(original, remote_changed);
    }

    #[test]
    fn result_cache_key_changes_by_scorecard_version_and_source_timestamp() {
        let mut report = empty_report(
            "/tmp/repo".to_owned(),
            "public_api".to_owned(),
            "ok",
            123,
            Vec::new(),
        );
        report.remote_url = "https://github.com/example/widgets.git".to_owned();
        report.commit_sha = Some("commit-a".to_owned());
        report.scorecard_version = Some("v5.0.0".to_owned());
        report.source_timestamp = Some("2026-06-29".to_owned());

        let original = result_cache_key(&report);
        report.scorecard_version = Some("v5.1.0".to_owned());
        let version_changed = result_cache_key(&report);
        report.scorecard_version = Some("v5.0.0".to_owned());
        report.source_timestamp = Some("2026-06-30".to_owned());
        let timestamp_changed = result_cache_key(&report);

        assert_ne!(original, version_changed);
        assert_ne!(original, timestamp_changed);
    }

    #[test]
    fn recommendation_mapping_covers_known_scorecard_checks() {
        let cases = [
            ("Security-Policy", "Security-Policy", "medium", "local"),
            ("Token-Permissions", "Token-Permissions", "high", "mixed"),
            ("Branch-Protection", "Branch-Protection", "medium", "remote"),
            ("Code-Review", "Code-Review", "medium", "remote"),
            (
                "Pinned-Dependencies",
                "Pinned-Dependencies",
                "medium",
                "local",
            ),
            ("Dangerous-Workflow", "Dangerous-Workflow", "high", "local"),
            ("Vulnerabilities", "Vulnerabilities", "high", "local"),
            ("SAST", "SAST", "medium", "local"),
            ("Fuzzing", "Fuzzing", "low", "local"),
            ("Maintained", "Maintained", "medium", "mixed"),
        ];

        for (check_name, category, severity, actionability) in cases {
            let recommendation = recommendation_for_check(check_name);
            assert_eq!(recommendation.category, category);
            assert_eq!(recommendation.check_name, check_name);
            assert_eq!(recommendation.severity, severity);
            assert_eq!(recommendation.actionability, actionability);
        }
    }

    fn token_permissions_scorecard_json() -> &'static str {
        r#"{
          "date": "2026-06-29",
          "scorecard": {
            "version": "v5.0.0",
            "commit": "abc123"
          },
          "score": 6.4,
          "checks": [
            {
              "name": "Token-Permissions",
              "score": 0,
              "reason": "workflow tokens are overly broad",
              "details": ["permissions are not set"],
              "documentation": {
                "url": "https://example.com/token",
                "short": "Token permissions"
              }
            }
          ]
        }"#
    }

    fn sast_scorecard_json() -> &'static str {
        r#"{
          "date": "2026-06-29",
          "scorecard": {
            "version": "v5.0.0",
            "commit": "def456"
          },
          "score": 7.2,
          "checks": [
            {
              "name": "SAST",
              "score": 0,
              "reason": "no static analysis detected",
              "details": []
            }
          ]
        }"#
    }

    fn test_git_repo_with_remote(remote_url: &str) -> std::path::PathBuf {
        let root = unique_test_dir("repository-scorecard");
        if let Err(error) = fs::create_dir_all(&root) {
            panic!("create test dir: {error}");
        }

        if Command::new("git").arg("--version").output().is_err() {
            return root;
        }

        let init_status = match Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("init")
            .arg("-q")
            .status()
        {
            Ok(status) => status,
            Err(error) => panic!("git init runs: {error}"),
        };
        assert!(init_status.success());
        let remote_status = match Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("remote")
            .arg("add")
            .arg("origin")
            .arg(remote_url)
            .status()
        {
            Ok(status) => status,
            Err(error) => panic!("git remote add runs: {error}"),
        };
        assert!(remote_status.success());
        root
    }

    fn commit_test_file(root: &Path) {
        if let Err(error) = fs::write(root.join("README.md"), "test repo\n") {
            panic!("write test file: {error}");
        }
        let add_status = match Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("add")
            .arg("README.md")
            .status()
        {
            Ok(status) => status,
            Err(error) => panic!("git add runs: {error}"),
        };
        assert!(add_status.success());
        let commit_status = match Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("-c")
            .arg("user.name=Aetower Tests")
            .arg("-c")
            .arg("user.email=aetower-tests@example.invalid")
            .arg("commit")
            .arg("-q")
            .arg("-m")
            .arg("initial commit")
            .status()
        {
            Ok(status) => status,
            Err(error) => panic!("git commit runs: {error}"),
        };
        assert!(commit_status.success());
    }

    fn unique_test_dir(prefix: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "{prefix}-{}-{nonce}-{sequence}",
            std::process::id()
        ))
    }
}
