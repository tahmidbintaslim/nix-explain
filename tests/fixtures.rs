//! Runs every log in tests/fixtures/ through the analyzer and checks the rule, the fix,
//! and that the first user frame points at the right file.
//! All logs except git-untracked-old.log are real output from Nix 2.34 / nixpkgs 26.11pre;
//! only the project path was rewritten to /home/me/proj.

use nix_explain::analyze;
use std::path::Path;

/// (fixture, expected rule, substring of the fix, expected first local frame or None)
#[rustfmt::skip]
const CASES: &[(&str, &str, &str, Option<&str>)] = &[
    ("infinite-recursion", "infinite-recursion", "self-reference", Some("config.nix:4")),
    ("imports-recursion", "imports-recursion", "In recursion.nix: import unconditionally", None),
    ("attribute-missing", "attribute-missing", "`colour` → `color`", Some("config.nix:4")),
    ("git-untracked", "git-untracked", "git add module.nix", Some("flake.nix:4")),
    ("git-untracked-old", "git-untracked", "git add hosts/laptop/hardware.nix", None),
    ("module-system", "attribute-missing", "`prot` → `port`", Some("configuration.nix:3")),
    ("unfree", "unfree", "[ \"vscode\" ]", None),
    ("hash-mismatch", "hash-mismatch", "hash = \"sha256-LJ3jc651pScWN2NQNERaXNOmrjWsbDBtQMDgZ2R4WJc=\";", None),
    ("cargo-hash", "rust-cargo-hash", "cargoHash = \"sha256-nxBmdki1weh8FQd0xPDV7yYJBMCxYfxQ3res3Xj2jic=\";", None),
    ("go-vendor-hash", "go-vendor-hash", "vendorHash = \"sha256-mGKxBRU5TPgdmiSx0DHEd0Ys8gsVD/YdBfbDdSVpC3U=\";", None),
    ("rust-openssl", "rust-openssl", "buildInputs = [ openssl ];", None),
    ("go-version", "go-version", "go = go_1_99", None),
    ("python-module", "python-module-not-found", "[ pyyaml ]", None),
    ("python-backend", "python-build-backend", "python3Packages.hatchling", None),
    ("c-header", "c-header-missing", "nix-locate --top-level include/zstd.h", None),
];

fn load(name: &str) -> nix_explain::Report {
    let log = std::fs::read_to_string(format!("tests/fixtures/{name}.log")).unwrap();
    analyze(&log, Path::new("/home/me/proj"))
}

#[test]
fn fixtures() {
    for (name, rule, fix, frame) in CASES {
        let r = load(name);
        let d = r
            .diagnosis
            .unwrap_or_else(|| panic!("{name}: no diagnosis"));
        assert_eq!(d.rule, *rule, "{name}");
        assert!(d.fix.contains(fix), "{name}: fix was {:?}", d.fix);
        let first = r.frames.first().map(|f| format!("{}:{}", f.file, f.line));
        assert_eq!(first.as_deref(), *frame, "{name}");
    }
}

#[test]
fn hides_nixpkgs_frames() {
    let r = load("module-system");
    assert!(r.hidden >= 1);
    assert_eq!(
        r.frames.len(),
        1,
        "call site and error site share a position"
    );
    assert_eq!(
        r.headline.as_deref(),
        Some("error: attribute 'prot' missing")
    );
}

#[test]
fn headline_is_root_cause_not_cascade() {
    let h = load("cargo-hash").headline.unwrap();
    assert!(h.starts_with("error: hash mismatch"), "{h}");
}
