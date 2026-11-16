//! Error rules: each one recognises a known failure and explains it with a cause and a fix.
//! Rules are tried in order and the first match wins, so specific rules come before generic ones.

use regex::{Captures, Regex};
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnosis {
    pub rule: &'static str,
    pub cause: String,
    pub fix: String,
}

struct Rule {
    id: &'static str,
    re: Regex,
    explain: fn(&Captures, &Path) -> (String, String),
}

fn rule(id: &'static str, re: &str, explain: fn(&Captures, &Path) -> (String, String)) -> Rule {
    Rule {
        id,
        re: Regex::new(re).unwrap(),
        explain,
    }
}

/// Capture group `i`, or "" if it didn't participate.
fn g<'a>(c: &'a Captures, i: usize) -> &'a str {
    c.get(i).map_or("", |m| m.as_str())
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        // ── Language-specific fixed-output hashes (before the generic hash rule) ──
        rule(
            "rust-cargo-hash",
            r"(?s)hash mismatch in fixed-output derivation '[^']*(?:-vendor|cargo-deps)[^']*'.*?got:\s*(\S+)",
            |c, _| {
                (
                "The Cargo dependency hash (`cargoHash`) in your `buildRustPackage` is stale: Cargo.lock changed or the hash was a placeholder.".into(),
                format!("cargoHash = \"{}\";", g(c, 1)),
            )
            },
        ),
        rule(
            "go-vendor-hash",
            r"(?s)hash mismatch in fixed-output derivation '[^']*-go-modules[^']*'.*?got:\s*(\S+)",
            |c, _| {
                (
                "The Go module hash (`vendorHash`) in your `buildGoModule` is stale: go.mod/go.sum changed or the hash was a placeholder.".into(),
                format!("vendorHash = \"{}\";", g(c, 1)),
            )
            },
        ),
        rule(
            "hash-mismatch",
            r"(?s)hash mismatch in fixed-output derivation '([^']+)'.*?specified:\s*(\S+).*?got:\s*(\S+)",
            |c, _| {
                (
                format!("A fetcher's pinned hash doesn't match what was downloaded for `{}` (the source changed, or the hash is a placeholder).", g(c, 1)),
                format!("Replace\n  hash = \"{}\";\nwith\n  hash = \"{}\";", g(c, 2), g(c, 3)),
            )
            },
        ),
        // ── Flakes & git ──
        rule(
            "git-untracked",
            // Nix ≥2.20 says so directly; older Nix claims the store copy "does not exist".
            r#"(?:Path '([^']+)' in the repository "[^"]*" is not tracked by Git|'/nix/store/[a-z0-9]{32}-source/([^']+)'(?: does not exist|: No such file or directory))"#,
            |c, ws| {
                let rel = if c.get(1).is_some() { g(c, 1) } else { g(c, 2) };
                let cause = if c.get(1).is_some() || ws.join(rel).exists() {
                    format!("`{rel}` exists on disk but isn't tracked by git. Flakes only see files git knows about.")
                } else {
                    format!("`{rel}` is referenced but missing from the flake source. Check the path, and if the file is new, that it's added to git.")
                };
                (cause, format!("git add {rel}"))
            },
        ),
        rule(
            "unfree",
            r"Package [‘']([^’']+)[’'].*?has an unfree license",
            |c, _| {
                let name = pname(g(c, 1));
                (
                    format!("`{name}` has a non-free license and nixpkgs refuses to evaluate it unless you opt in."),
                    format!(
                        "NixOS / home-manager config:\n  nixpkgs.config.allowUnfreePredicate = pkg: builtins.elem (lib.getName pkg) [ \"{name}\" ];\n\
                         One-off CLI build:\n  NIXPKGS_ALLOW_UNFREE=1 nix build --impure ..."
                    ),
                )
            },
        ),
        // ── Evaluator errors ──
        rule(
            "infinite-recursion",
            r"infinite recursion encountered",
            |_, _| {
                (
                "A value depends on itself. Common culprits: a `let`/`rec` binding that refers to itself, using `config` or `pkgs` to compute `imports`, or an overlay reading `final` where it should read `prev`.".into(),
                "Look at the highlighted line: break the self-reference (rename the binding, use `prev.<pkg>` in overlays, or move `config`-dependent logic out of `imports` into `lib.mkIf`).".into(),
            )
            },
        ),
        rule(
            "attribute-missing",
            r"(?s)attribute '([^']+)' missing(?:.*?Did you mean (?:one of )?([^?\n]+)\?)?",
            |c, _| {
                let fix = match c.get(2) {
                    Some(s) => format!("Probably a typo: `{}` → `{}`.", g(c, 1), s.as_str().trim()),
                    None => format!("Check the spelling of `{}`, or that the option/package exists in your nixpkgs version (search.nixos.org).", g(c, 1)),
                };
                (
                    format!(
                        "You accessed `{}` on an attribute set that doesn't contain it.",
                        g(c, 1)
                    ),
                    fix,
                )
            },
        ),
        // ── Rust builds ──
        rule(
            "rust-openssl",
            r"Could not find (?:directory of OpenSSL installation|openssl via pkg-config)|failed to run custom build command for .openssl-sys",
            |_, _| {
                (
                "The `openssl-sys` crate can't find OpenSSL: the Nix build sandbox has no system libraries unless you list them.".into(),
                "nativeBuildInputs = [ pkg-config ];\nbuildInputs = [ openssl ];".into(),
            )
            },
        ),
        rule(
            "rust-system-lib",
            r"The system library .([\w.+-]+). required by crate .([\w-]+). was not found|(The pkg-config command could not be found)",
            |c, _| {
                match c.get(1) {
                Some(lib) => (
                    format!("Crate `{}` needs the system library `{}`, which isn't in the build inputs.", g(c, 2), lib.as_str()),
                    format!("nativeBuildInputs = [ pkg-config ];\nbuildInputs = [ /* package providing {} */ ];  # find it: nix-locate {}.pc", lib.as_str(), lib.as_str()),
                ),
                None => (
                    "A build script called `pkg-config`, but it isn't available in the build sandbox.".into(),
                    "nativeBuildInputs = [ pkg-config ];".into(),
                ),
            }
            },
        ),
        // ── Go builds ──
        rule(
            "go-version",
            r"go\.mod requires go >= (\d+)\.(\d+)[\d.]* \(running go ([\d.]+)",
            |c, _| {
                (
                format!("go.mod requires Go ≥ {}.{}, but nixpkgs' default `go` is {}.", g(c, 1), g(c, 2), g(c, 3)),
                format!("(buildGoModule.override {{ go = go_{}_{}; }}) {{ ... }}\n# or lower the `go` directive in go.mod", g(c, 1), g(c, 2)),
            )
            },
        ),
        // ── Python builds (build-backend before generic ModuleNotFoundError) ──
        rule(
            "python-build-backend",
            r"No module named '(setuptools|wheel|hatchling|poetry|flit_core|pdm|setuptools_scm)'|Backend '([\w]+)[^']*' is not available",
            |c, _| {
                let m = if c.get(1).is_some() { g(c, 1) } else { g(c, 2) };
                let pkg = match m {
                    "poetry" => "poetry-core",
                    "flit_core" => "flit-core",
                    "pdm" => "pdm-backend",
                    "setuptools_scm" => "setuptools-scm",
                    x => x,
                };
                (
                    format!("The Python build backend `{m}` (from pyproject.toml's [build-system]) isn't available to the build."),
                    format!("pyproject = true;\nbuild-system = [ python3Packages.{pkg} ];"),
                )
            },
        ),
        rule(
            "python-module-not-found",
            r"ModuleNotFoundError: No module named '([\w]+)",
            |c, _| {
                let m = g(c, 1);
                // Import names that differ from their nixpkgs attribute.
                let pkg = match m {
                    "yaml" => "pyyaml",
                    "PIL" => "pillow",
                    "cv2" => "opencv4",
                    "sklearn" => "scikit-learn",
                    "bs4" => "beautifulsoup4",
                    "dateutil" => "python-dateutil",
                    "dotenv" => "python-dotenv",
                    "jwt" => "pyjwt",
                    "attr" => "attrs",
                    "google" => "protobuf",
                    x => x,
                };
                (
                    format!("Python can't import `{m}`: it isn't a runtime dependency of the package."),
                    format!("dependencies = with python3Packages; [ {pkg} ];  # `propagatedBuildInputs` on older nixpkgs"),
                )
            },
        ),
        rule(
            "c-header-missing",
            r"fatal error: ([\w./+-]+\.h): No such file or directory",
            |c, _| {
                let h = g(c, 1);
                let pkg = match h.split('/').next().unwrap_or(h) {
                    "Python.h" => "python3",
                    "openssl" => "openssl",
                    "zlib.h" => "zlib",
                    "ffi.h" => "libffi",
                    "sqlite3.h" => "sqlite",
                    "libxml" => "libxml2",
                    "yaml.h" => "libyaml",
                    "curl" => "curl",
                    _ => return (
                        format!("A C/C++ extension includes `{h}`, but no build input provides that header."),
                        format!("Find the package: nix-locate --top-level include/{h}\nthen add it: buildInputs = [ <package> ];"),
                    ),
                };
                (
                    format!(
                        "A C/C++ extension includes `{h}`, but `{pkg}` isn't in the build inputs."
                    ),
                    format!("buildInputs = [ {pkg} ];"),
                )
            },
        ),
    ]
});

/// `vscode-1.85.1` → `vscode`.
fn pname(name: &str) -> &str {
    name.match_indices('-')
        .find(|(i, _)| name[i + 1..].starts_with(|c: char| c.is_ascii_digit()))
        .map_or(name, |(i, _)| &name[..i])
}

pub fn diagnose(text: &str, workspace: &Path) -> Option<Diagnosis> {
    RULES.iter().find_map(|r| {
        r.re.captures(text).map(|c| {
            let (cause, fix) = (r.explain)(&c, workspace);
            Diagnosis {
                rule: r.id,
                cause,
                fix,
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Diagnosis {
        diagnose(text, Path::new("/nonexistent")).expect("no rule matched")
    }

    #[test]
    fn cargo_hash() {
        let x = d("error: hash mismatch in fixed-output derivation '/nix/store/abc-myapp-0.1.0-vendor.drv':\n  specified: sha256-AAAA=\n     got:    sha256-REAL=");
        assert_eq!(
            (x.rule, x.fix.as_str()),
            ("rust-cargo-hash", "cargoHash = \"sha256-REAL=\";")
        );
    }

    #[test]
    fn go_vendor_hash() {
        let x = d("error: hash mismatch in fixed-output derivation '/nix/store/abc-myapp-1.0-go-modules.drv':\n  specified: sha256-AAAA=\n     got:    sha256-GO=");
        assert_eq!(
            (x.rule, x.fix.as_str()),
            ("go-vendor-hash", "vendorHash = \"sha256-GO=\";")
        );
    }

    #[test]
    fn generic_hash() {
        let x = d("error: hash mismatch in fixed-output derivation '/nix/store/abc-source.drv':\n         specified: sha256-OLD=\n            got:    sha256-NEW=");
        assert_eq!(x.rule, "hash-mismatch");
        assert!(x.fix.contains("sha256-OLD=") && x.fix.contains("sha256-NEW="));
    }

    #[test]
    fn untracked_new_and_old_nix() {
        let x =
            d("error: Path 'module.nix' in the repository \"/home/me/p\" is not tracked by Git.");
        assert_eq!(
            (x.rule, x.fix.as_str()),
            ("git-untracked", "git add module.nix")
        );
        let y = d("error: path '/nix/store/0123456789abcdfghijklmnpqrsvwxyz-source/hosts/laptop.nix' does not exist");
        assert_eq!(
            (y.rule, y.fix.as_str()),
            ("git-untracked", "git add hosts/laptop.nix")
        );
    }

    #[test]
    fn unfree() {
        let x = d("error: Package ‘vscode-1.85.1’ in /nix/store/x/pkgs/vscode.nix:63 has an unfree license (‘unfree’), refusing to evaluate.");
        assert_eq!(x.rule, "unfree");
        assert!(x.fix.contains("[ \"vscode\" ]"), "{}", x.fix);
    }

    #[test]
    fn infinite_recursion_and_attr_missing() {
        assert_eq!(
            d("error: infinite recursion encountered").rule,
            "infinite-recursion"
        );
        let x = d("error: attribute 'colour' missing\n  at f.nix:1:1:\n  Did you mean color?");
        assert_eq!(x.rule, "attribute-missing");
        assert!(x.fix.contains("`colour` → `color`"));
    }

    #[test]
    fn rust_native_deps() {
        assert_eq!(d("Could not find directory of OpenSSL installation, and this `-sys` crate cannot proceed").rule, "rust-openssl");
        let x = d("The system library `libudev` required by crate `libudev-sys` was not found.");
        assert!(x.cause.contains("libudev-sys"));
        assert_eq!(
            d("The pkg-config command could not be found.").rule,
            "rust-system-lib"
        );
    }

    #[test]
    fn go_version() {
        let x = d("go: go.mod requires go >= 1.23.0 (running go 1.22.7; GOTOOLCHAIN=local)");
        assert!(x.fix.contains("go = go_1_23"), "{}", x.fix);
    }

    #[test]
    fn python_rules() {
        assert!(d("ModuleNotFoundError: No module named 'yaml'")
            .fix
            .starts_with("dependencies = with python3Packages; [ pyyaml ];"));
        let x = d("ModuleNotFoundError: No module named 'setuptools'");
        assert_eq!(x.rule, "python-build-backend");
        assert_eq!(
            d("ERROR Backend 'hatchling.build' is not available.").fix,
            "pyproject = true;\nbuild-system = [ python3Packages.hatchling ];"
        );
        assert_eq!(
            d("src/ext.c:1:10: fatal error: Python.h: No such file or directory").fix,
            "buildInputs = [ python3 ];"
        );
        assert!(d("fatal error: gmp.h: No such file or directory")
            .fix
            .contains("nix-locate"));
    }

    #[test]
    fn pname_strips_version() {
        assert_eq!(pname("vscode-1.85.1"), "vscode");
        assert_eq!(pname("google-chrome-120.0"), "google-chrome");
        assert_eq!(pname("steam"), "steam");
    }

    #[test]
    fn no_match() {
        assert!(diagnose("all good", Path::new("/")).is_none());
    }
}
