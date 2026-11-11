//! The ten bestiary placeholder rigs match their committed parts files and
//! game files. This replaces the CI art job for them (they have no oil atlas).

use crabrig::bestiary;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: String) -> String {
    std::fs::read_to_string(root().join(&path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn placeholder_rigs_and_ink_exports_match_the_committed_files() {
    for name in bestiary::NAMES {
        let committed = read(format!("art/creatures/{name}/{name}-parts.svg"));
        let svg = bestiary::parts_svg(name).expect("a bestiary name has a rig");
        assert_eq!(svg, committed, "{name} parts svg drifted");
        let export = crabrig::export::export(
            &svg,
            &crabrig::export::Options {
                zoom: 1.0,
                source_name: format!("art/creatures/{name}/{name}-parts.svg"),
                name: name.to_string(),
            },
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(export.parts, 3, "{name}");
        assert_eq!(
            export.json,
            read(format!("assets/creatures/{name}-parts.json")),
            "{name} json drifted"
        );
        let atlas = std::fs::read(root().join(format!("assets/creatures/{name}-atlas.png")))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(export.atlas_png, atlas, "{name} atlas drifted");
    }
}

#[test]
fn only_the_ten_names_have_rigs() {
    assert_eq!(bestiary::NAMES.len(), 10);
    assert!(bestiary::parts_svg("snapping-shrimp").is_none());
}
