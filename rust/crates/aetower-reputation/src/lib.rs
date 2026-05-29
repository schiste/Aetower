//! Opt-in, consent-gated binary reputation via VirusTotal.
//!
//! Privacy contract: this crate only ever sends a file's **SHA-256** to
//! VirusTotal (lookup-only, `GET /api/v3/files/{sha256}`). It never uploads the
//! file, and never transmits the path, hostname, or any other context. Callers
//! are responsible for the consent gate (feature enabled + API key present) and
//! for restricting *which* binaries are hashed (the engine only looks up
//! unsigned/ad-hoc binaries with active network connections).

use std::fs::File;
use std::io::Read;
use std::time::Duration;

use aetower_model::{BinaryReputation, ReputationVerdict};
use reqwest::blocking::Client;

/// Don't hash files larger than this — bounds the cost of the SHA-256 pass and
/// avoids stalling a refresh tick on a pathologically large binary.
const MAX_HASHED_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Read size for the streaming hash.
const HASH_CHUNK_BYTES: usize = 1024 * 1024;

/// VirusTotal free tier allows 4 requests/minute. Stay comfortably under it.
pub const DEFAULT_MIN_REQUEST_INTERVAL_MILLIS: u64 = 16_000;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

/// The outcome of a single reputation lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum ReputationOutcome {
    /// The hash was found in VirusTotal; carries the parsed reputation.
    Found(BinaryReputation),
    /// VirusTotal has no record of this hash (HTTP 404). Not an error — the
    /// binary simply isn't in the corpus.
    Unknown,
    /// VirusTotal rate-limited us (HTTP 429). Back off; do not cache.
    RateLimited,
    /// Any other failure (network, auth, parse). Carries a short reason.
    Error(String),
}

/// Streaming SHA-256 of a file, returned as a lowercase hex string. Returns
/// `None` if the file can't be opened/read or exceeds [`MAX_HASHED_FILE_BYTES`].
pub fn hash_file(path: &str) -> Option<String> {
    let mut file = File::open(path).ok()?;
    if let Ok(metadata) = file.metadata()
        && metadata.len() > MAX_HASHED_FILE_BYTES
    {
        return None;
    }
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_CHUNK_BYTES];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(_) => return None,
        }
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Derive a single safety verdict from VirusTotal's per-engine counts. Malicious
/// dominates suspicious, which dominates a clean result.
pub fn derive_verdict(malicious: u32, suspicious: u32) -> ReputationVerdict {
    if malicious > 0 {
        ReputationVerdict::Malicious
    } else if suspicious > 0 {
        ReputationVerdict::Suspicious
    } else {
        ReputationVerdict::Clean
    }
}

/// Build the public VirusTotal GUI permalink for a file hash.
pub fn permalink_for(sha256: &str) -> String {
    format!("https://www.virustotal.com/gui/file/{sha256}")
}

/// Interpret an HTTP status + body into a [`ReputationOutcome`]. Pure (no I/O)
/// so the status→outcome mapping and JSON parsing are unit-testable.
pub fn interpret_response(
    status: u16,
    body: &str,
    sha256: &str,
    checked_at_millis: u64,
) -> ReputationOutcome {
    match status {
        200 => match parse_reputation(body, sha256, checked_at_millis) {
            Some(reputation) => ReputationOutcome::Found(reputation),
            None => ReputationOutcome::Error("unparseable VirusTotal response".to_owned()),
        },
        404 => ReputationOutcome::Unknown,
        429 => ReputationOutcome::RateLimited,
        other => ReputationOutcome::Error(format!("VirusTotal HTTP {other}")),
    }
}

/// Parse a VirusTotal v3 `/files/{id}` success body into a [`BinaryReputation`].
fn parse_reputation(body: &str, sha256: &str, checked_at_millis: u64) -> Option<BinaryReputation> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let stats = value
        .get("data")?
        .get("attributes")?
        .get("last_analysis_stats")?;
    let count = |key: &str| -> u32 {
        stats
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32
    };
    let malicious = count("malicious");
    let suspicious = count("suspicious");
    let harmless = count("harmless");
    let undetected = count("undetected");
    Some(BinaryReputation {
        sha256: sha256.to_owned(),
        malicious,
        suspicious,
        harmless,
        undetected,
        total_engines: malicious
            .saturating_add(suspicious)
            .saturating_add(harmless)
            .saturating_add(undetected),
        verdict: derive_verdict(malicious, suspicious),
        permalink: permalink_for(sha256),
        checked_at_millis,
    })
}

/// A VirusTotal lookup client. Holds its own blocking HTTP client (same
/// TLS/timeout profile as the telemetry exporter) and the API key.
pub struct VtClient {
    client: Client,
    api_key: String,
}

impl VtClient {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client, api_key }
    }

    /// Look up a single SHA-256. Sends only the hash + API key — never the file.
    pub fn lookup(&self, sha256: &str, checked_at_millis: u64) -> ReputationOutcome {
        let url = format!("https://www.virustotal.com/api/v3/files/{sha256}");
        let response = self
            .client
            .get(&url)
            .header("x-apikey", &self.api_key)
            .send();
        match response {
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.text().unwrap_or_default();
                interpret_response(status, &body, sha256, checked_at_millis)
            }
            Err(error) => ReputationOutcome::Error(format!("VirusTotal request failed: {error}")),
        }
    }
}

/// Minimum-interval gate so we never exceed VirusTotal's free-tier rate. State
/// lives in the caller (e.g. the engine's adapter state); this is pure logic.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    min_interval_millis: u64,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            min_interval_millis: DEFAULT_MIN_REQUEST_INTERVAL_MILLIS,
        }
    }
}

impl RateLimiter {
    pub fn new(min_interval_millis: u64) -> Self {
        Self {
            min_interval_millis,
        }
    }

    /// True when enough time has elapsed since `last_request_millis` to issue
    /// another request. A `last_request_millis` of 0 always allows (cold start).
    pub fn ready(&self, last_request_millis: u64, now_millis: u64) -> bool {
        if last_request_millis == 0 {
            return true;
        }
        now_millis.saturating_sub(last_request_millis) >= self.min_interval_millis
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_priority() {
        assert_eq!(derive_verdict(3, 1), ReputationVerdict::Malicious);
        assert_eq!(derive_verdict(0, 2), ReputationVerdict::Suspicious);
        assert_eq!(derive_verdict(0, 0), ReputationVerdict::Clean);
    }

    #[test]
    fn parses_success_body() {
        let body = r#"{
            "data": {
                "id": "abc",
                "attributes": {
                    "last_analysis_stats": {
                        "malicious": 4,
                        "suspicious": 1,
                        "harmless": 60,
                        "undetected": 5,
                        "timeout": 0
                    }
                }
            }
        }"#;
        let outcome = interpret_response(200, body, "abc", 1234);
        let ReputationOutcome::Found(reputation) = outcome else {
            panic!("expected Found, got {outcome:?}");
        };
        assert_eq!(reputation.malicious, 4);
        assert_eq!(reputation.suspicious, 1);
        assert_eq!(reputation.harmless, 60);
        assert_eq!(reputation.undetected, 5);
        assert_eq!(reputation.total_engines, 70);
        assert_eq!(reputation.verdict, ReputationVerdict::Malicious);
        assert_eq!(reputation.sha256, "abc");
        assert_eq!(reputation.checked_at_millis, 1234);
        assert!(reputation.permalink.contains("abc"));
    }

    #[test]
    fn maps_status_codes() {
        assert_eq!(
            interpret_response(404, "", "h", 0),
            ReputationOutcome::Unknown
        );
        assert_eq!(
            interpret_response(429, "", "h", 0),
            ReputationOutcome::RateLimited
        );
        assert!(matches!(
            interpret_response(500, "", "h", 0),
            ReputationOutcome::Error(_)
        ));
    }

    #[test]
    fn malformed_success_body_is_error() {
        assert!(matches!(
            interpret_response(200, "not json", "h", 0),
            ReputationOutcome::Error(_)
        ));
    }

    #[test]
    fn rate_limiter_gates_on_interval() {
        let limiter = RateLimiter::new(16_000);
        // Cold start (never requested) always allows.
        assert!(limiter.ready(0, 1_000));
        // Too soon after the last request.
        assert!(!limiter.ready(10_000, 20_000));
        // Enough time elapsed.
        assert!(limiter.ready(10_000, 26_000));
    }
}
