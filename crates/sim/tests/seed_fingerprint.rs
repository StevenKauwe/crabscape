//! The settled seed's fingerprint counts code, not comments, whitespace or
//! test-only items (`crates/sim/build_support/fingerprint.rs`).
#[path = "../build_support/fingerprint.rs"]
mod fingerprint;

use fingerprint::normalise;

const BASE: &str = r##"
//! module doc
/// doc
fn step(x: f32) -> f32 {
    let s = "a // not a comment /* nor this */";
    let r = r#"raw "quoted" // still text"#;
    let c = '/';
    let l: &'static str = "x";
    x * 2.0 + s.len() as f32
}
#[cfg(test)]
mod tests {
    #[test]
    fn t() { assert!(true); }
}
"##;

#[test]
fn comments_whitespace_and_test_items_do_not_change_it() {
    // Spacing inside a line still counts as a token change only where tokens
    // differ; compare against the same tokens spaced the same way.
    let same_tokens = BASE
        .replace("//! module doc", "// other words")
        .replace("/// doc", "")
        .replace(
            "    #[test]\n    fn t() { assert!(true); }",
            "    #[test]\n    fn renamed() { assert_eq!(1, 1); }",
        )
        .replace("    let c", "\n\n    // a note\n    let c");
    assert_eq!(normalise(BASE), normalise(&same_tokens));
    assert_eq!(normalise(BASE), normalise(&BASE.replace('\n', "\r\n")));
}

#[test]
fn a_code_change_alters_it() {
    for changed in [
        BASE.replace("x * 2.0", "x * 2.5"),
        BASE.replace("// not a comment", "// not a remark"),
        BASE.replace("'/'", "'\\\\'"),
        BASE.replace("fn step", "pub fn step"),
    ] {
        assert_ne!(normalise(BASE), normalise(&changed), "{changed}");
    }
}

#[test]
fn the_real_sources_normalise_without_losing_their_code() {
    let src = include_str!("../src/level/coral_rise.rs");
    let n = normalise(src);
    assert!(!n.is_empty());
    assert!(!String::from_utf8_lossy(&n).contains("#[cfg(test)]"));
}
