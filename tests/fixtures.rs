//! Runs every log in tests/fixtures/ through the analyzer and checks the rule, the fix,
//! and that the first user frame points at the right file.

use nix_explain::analyze;
use std::path::Path;

/// (fixture, expected rule, substring of the fix, expected first local frame or None)
const CASES: &[(&str, &str, &str, Option<&str>)] = &[
    (
        "infinite-recursion",
        "infinite-recursion",
        "self-reference",
        Some("config.nix:4"),
    ),
    (
        "attribute-missing",
        "attribute-missing",
        "`colour` → `color`",
        Some("config.nix:4"),
    ),
    (
        "git-untracked",
        "git-untracked",
        "git add module.nix",
        Some("flake.nix:4"),
    ),
    (
        "git-untracked-old",
        "git-untracked",
        "git add hosts/laptop/hardware.nix",
        None,
    ),
    (
        "module-system",
        "attribute-missing",
        "`prot` → `port`",
        Some("configuration.nix:3"),
    ),
    ("unfree", "unfree", "[ \"vscode\" ]", None),
    (
        "cargo-hash",
        "rust-cargo-hash",
        "cargoHash = \"sha256-3mH2kV0nqfY1bQyN9u7pZ2sR8xW5tE4cL6aJ1dG0hKo=\";",
        None,
    ),
    (
        "go-vendor-hash",
        "go-vendor-hash",
        "vendorHash = \"sha256-Xw2Q9f0bR7pL3nV6hY1tK8mE4sD5cA2uJ7gZ0oI9qWe=\";",
        None,
    ),
    (
        "rust-openssl",
        "rust-openssl",
        "buildInputs = [ openssl ];",
        None,
    ),
    ("go-version", "go-version", "go = go_1_24", None),
    (
        "python-module",
        "python-module-not-found",
        "[ pyyaml ]",
        None,
    ),
    (
        "python-backend",
        "python-build-backend",
        "python3Packages.hatchling",
        None,
    ),
    (
        "python-c-header",
        "c-header-missing",
        "buildInputs = [ zlib ];",
        None,
    ),
];

#[test]
fn fixtures() {
    let ws = Path::new("/home/me/proj");
    for (name, rule, fix, frame) in CASES {
        let log = std::fs::read_to_string(format!("tests/fixtures/{name}.log")).unwrap();
        let r = analyze(&log, ws);
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
    let log = std::fs::read_to_string("tests/fixtures/module-system.log").unwrap();
    let r = analyze(&log, Path::new("/home/me/proj"));
    assert_eq!(r.hidden, 2); // attrsets.nix + modules.nix
    assert_eq!(
        r.frames.len(),
        1,
        "call site and error site share a position"
    );
    assert!(r.frames.iter().all(|f| !f.file.starts_with("/nix/store")));
    assert_eq!(
        r.headline.as_deref(),
        Some("error: attribute 'prot' missing")
    );
}
