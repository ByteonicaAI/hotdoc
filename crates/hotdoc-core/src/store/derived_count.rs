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
    fn derived_entry_count_matches_real_packs() {
        use std::path::PathBuf;
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("packs")
            .join("curate");
        let packs = crate::pack::load_dir(&dir).expect("load real packs").loaded;
        let expected: usize = packs.iter().map(|p| p.entries.len()).sum();
        let idxdir = tempfile::tempdir().expect("tempdir");
        let idx = HotdocIndex::build(&packs, idxdir.path()).expect("build");
        assert_eq!(
            idx.entry_count(),
            expected,
            "entry_count must equal sum over REAL packs"
        );
    }

    #[test]
    fn build_rejects_cross_pack_duplicate_ids() {
        // ponytail: this test used to prove that two packs sharing an
        // entry id both counted in entry_count() (a flat Vec sum, not a
        // deduped HashMap len). That premise is now obsolete: the 4c
        // cross-pack id uniqueness gate (index.rs) rejects this state at
        // build() time, because a live collision would make the
        // doc→entry lookup in search() silently keep the last writer and
        // render the WRONG pack's entry for a retrieved doc. So the
        // correct assertion is now "build() fails", not "the count is
        // right anyway".
        let mut packs = small_packs();
        // Force a cross-pack duplicate id.
        packs[1].entries[0].id = packs[0].entries[0].id.clone();
        let dir = tempfile::tempdir().expect("tempdir");
        let res = HotdocIndex::build(&packs, dir.path());
        assert!(
            res.is_err(),
            "cross-pack duplicate entry id must fail build(), not silently succeed"
        );
    }
}
