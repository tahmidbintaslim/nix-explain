//! Parses Nix evaluation traces and separates the user's frames from nixpkgs internals.

use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

static ANSI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());
// `at /path/file.nix:12:5:` — the location line under each trace frame and under the final error.
static AT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*at (.+?):(\d+):(\d+):\s*$").unwrap());
// Code excerpt lines: `   12| foo = bar;` and the caret line `     |   ^`.
static SNIPPET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\d*\|").unwrap());
// A flake copied into the store: /nix/store/<hash>-source/<relative path>.
static STORE_SOURCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/nix/store/[a-z0-9]{32}-source/(.+)$").unwrap());

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// What Nix was doing, e.g. "while evaluating the attribute 'config'".
    pub desc: String,
    /// Path as shown to the user: workspace-relative when it maps to a local file.
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub snippet: Vec<String>,
    /// True if the frame points at the user's own code rather than nixpkgs/store internals.
    pub local: bool,
}

pub fn strip_ansi(s: &str) -> String {
    ANSI.replace_all(s, "").into_owned()
}

/// The root error: the first non-empty `error: <text>` line. Eval traces open with a bare
/// `error:` and carry the message at the bottom; build logs put the root cause first and
/// follow it with cascades like "Build failed due to failed dependency".
pub fn headline(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("error:"))
        .map(str::trim)
        .find(|m| !m.is_empty())
        .map(|m| format!("error: {m}"))
}

/// Extracts every located frame, in the order Nix printed them (outermost first).
pub fn frames(text: &str, workspace: &Path) -> Vec<Frame> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(c) = AT.captures(line) else { continue };
        // The description is the nearest `…` / `error:` line above, skipping blanks.
        let desc = lines[..i]
            .iter()
            .rev()
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            // The final message is already the headline; just mark where it happened.
            .map(|l| {
                if l.starts_with("error:") {
                    "← error is here".into()
                } else {
                    l.trim_start_matches('…').trim().to_string()
                }
            })
            .unwrap_or_default();
        let snippet = lines[i + 1..]
            .iter()
            .take_while(|l| SNIPPET.is_match(l))
            .map(|l| l.trim_end().to_string())
            .collect();
        let (file, local) = classify(&c[1], workspace);
        out.push(Frame {
            desc,
            file,
            line: c[2].parse().unwrap_or(0),
            col: c[3].parse().unwrap_or(0),
            snippet,
            local,
        });
    }
    out
}

/// Decides whether a path is the user's code, and rewrites it relative to the workspace.
fn classify(path: &str, workspace: &Path) -> (String, bool) {
    // A flake's own files show up as /nix/store/<hash>-source/<rel>; it's the user's
    // file if <rel> exists in the workspace (nixpkgs' lib/modules.nix won't).
    if let Some(c) = STORE_SOURCE.captures(path) {
        let rel = &c[1];
        return if workspace.join(rel).is_file() {
            (rel.to_string(), true)
        } else {
            (path.to_string(), false)
        };
    }
    // «github:...», «string», «none» and other store paths are never user code.
    if path.starts_with('«') || path.starts_with("/nix/store/") {
        return (path.to_string(), false);
    }
    match Path::new(path).strip_prefix(workspace) {
        Ok(rel) => (rel.display().to_string(), true),
        Err(_) => (path.to_string(), true), // outside the workspace but not in the store: still user-owned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = "error:
       … while evaluating the attribute 'config'
         at /nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-source/lib/modules.nix:320:9:
          319|         options = checked options;
          320|         config = checked (removeAttrs config [ \"_module\" ]);
             |         ^

       … from call site
         at /home/me/proj/flake.nix:12:5:
           12|     port = config.services.web.prot;
             |     ^

       error: attribute 'prot' missing";

    #[test]
    fn keeps_local_frames_and_relativizes() {
        let f = frames(TRACE, Path::new("/home/me/proj"));
        assert_eq!(f.len(), 2);
        assert!(!f[0].local);
        assert_eq!(f[0].desc, "while evaluating the attribute 'config'");
        assert!(f[1].local);
        assert_eq!(
            (f[1].file.as_str(), f[1].line, f[1].col),
            ("flake.nix", 12, 5)
        );
        assert_eq!(f[1].snippet.len(), 2);
    }

    #[test]
    fn headline_skips_bare_error_line() {
        assert_eq!(headline(TRACE).unwrap(), "error: attribute 'prot' missing");
    }

    #[test]
    fn store_source_maps_to_workspace_file() {
        let ws = std::env::temp_dir().join("nix-explain-classify");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("flake.nix"), "{}").unwrap();
        let p = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-source/flake.nix";
        assert_eq!(classify(p, &ws), ("flake.nix".into(), true));
        let lib = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-source/lib/modules.nix";
        assert!(!classify(lib, &ws).1);
    }

    #[test]
    fn strips_color_codes() {
        assert_eq!(strip_ansi("\x1b[31;1merror:\x1b[0m boom"), "error: boom");
    }
}
