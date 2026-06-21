use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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

pub fn load_dir(path: &Path) -> Result<Vec<Pack>> {
    if !path.is_dir() {
        bail!("pack directory does not exist: {}", path.display());
    }
    let mut packs = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() || p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&p).map_err(|e| anyhow!("reading {}: {e}", p.display()))?;
        let pack: Pack =
            serde_json::from_str(&raw).map_err(|e| anyhow!("parsing {}: {e}", p.display()))?;
        validate(&pack).map_err(|e| anyhow!("invalid pack {}: {e}", p.display()))?;
        packs.push(pack);
    }
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(packs)
}

pub fn validate(pack: &Pack) -> Result<()> {
    if pack.id.is_empty() {
        bail!("pack id is empty");
    }
    if !pack
        .id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!("pack id {:?} must match ^[a-z0-9-]+$", pack.id);
    }
    if pack.entries.is_empty() {
        bail!("pack {} has no entries", pack.id);
    }
    let mut seen_ids = std::collections::HashSet::new();
    for entry in &pack.entries {
        if !seen_ids.insert(entry.id.as_str()) {
            bail!("duplicate entry id {:?} in pack {}", entry.id, pack.id);
        }
        if entry.syntax.is_empty() {
            bail!("entry {} has empty syntax", entry.id);
        }
        if matches!(entry.source, EntrySource::Personal) {
            bail!(
                "entry {} uses source:\"personal\" which is reserved for v1.1",
                entry.id
            );
        }
    }
    Ok(())
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
        let packs = load_dir(&fixture_dir()).expect("load");
        let git = packs
            .iter()
            .find(|p| p.id == "git")
            .expect("git pack present");
        validate(git).expect("git pack should validate");
        assert_eq!(git.entries.len(), 15);
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
        let packs = load_dir(&fixture_dir()).expect("load");
        assert!(
            packs.iter().any(|p| p.id == "git"),
            "git pack should be present at packs/curate root"
        );
        assert!(
            packs.iter().any(|p| p.id == "docker"),
            "docker pack should be present at packs/curate root"
        );
        assert!(
            packs.iter().any(|p| p.id == "kubectl"),
            "kubectl pack should be present at packs/curate root"
        );
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
}
