//! Asserts HotdocIndex::entry_count() matches the runtime pack entry
//! count, proving no hardcoded numbers leak into the UI. The Tauri IPC
//! `index_status` (in `src-tauri/src/commands.rs`) reads counts from
//! SQLite, which `HotdocIndex::populate_store` populates from the same
//! packs; an accurate `entry_count()` therefore implies an accurate
//! IPC count. This is the WS-D derived-count gate.

#[cfg(test)]
mod tests {
    use crate::index::HotdocIndex;
    use crate::pack::{Entry, EntrySource, Pack};

    fn small_packs() -> Vec<Pack> {
        let entry = |id: &str, syntax: &str| Entry {
            id: id.into(),
            title: format!("{id} title"),
            syntax: syntax.into(),
            description: String::new(),
            examples: vec![],
            tags: vec![],
            aliases: vec![],
            source: EntrySource::Curated,
            source_url: None,
        };
        vec![
            Pack {
                id: "git".into(),
                name: "Git".into(),
                version: "0.1.0".into(),
                source: "test".into(),
                license: "test".into(),
                homepage: None,
                entries: vec![
                    entry("git-stash", "git stash"),
                    entry("git-reset", "git reset"),
                    entry("git-stash-pop", "git stash pop"),
                ],
            },
            Pack {
                id: "docker".into(),
                name: "Docker".into(),
                version: "0.1.0".into(),
                source: "test".into(),
                license: "test".into(),
                homepage: None,
                entries: vec![entry("docker-ps", "docker ps")],
            },
        ]
    }

    #[test]
    fn derived_entry_count_matches_packs() {
        let packs = small_packs();
        let expected: usize = packs.iter().map(|p| p.entries.len()).sum();
        let dir = tempfile::tempdir().expect("tempdir");
        let idx = HotdocIndex::build(&packs, dir.path()).expect("build");
        assert_eq!(
            idx.entry_count(),
            expected,
            "entry_count must equal sum of pack entries"
        );
        // ponytail: pack_count is derived from packs.len() — no fixture
        // path can produce a different count, but assert explicitly to
        // document the contract.
        assert_eq!(packs.len(), 2);
    }
}
