use nix_explain::{analyze, Report};
use serde_json::json;
use std::io::{self, IsTerminal, Read, Write};
use std::process::{Command, ExitCode, Stdio};

const USAGE: &str = "\
nix-explain — plain-English Nix errors

USAGE:
    nix-explain [--json] [--raw] -- <command> [args...]
    <command> 2>&1 | nix-explain [--json] [--raw]

OPTIONS:
    --json      Machine-readable output for CI
    --raw       Also print the full, unedited Nix output
    -h, --help  Show this help
    -V, --version";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut json, mut raw, mut cmd) = (false, false, Vec::new());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--json" => json = true,
            "--raw" => raw = true,
            "-h" | "--help" => return done(USAGE, 0),
            "-V" | "--version" => {
                return done(concat!("nix-explain ", env!("CARGO_PKG_VERSION")), 0)
            }
            "--" => cmd.extend(args.by_ref()),
            _ if a.starts_with('-') => return done(&format!("unknown option: {a}\n\n{USAGE}"), 2),
            // Allow `nix-explain nix build` without the `--`.
            _ => cmd.extend(std::iter::once(a).chain(args.by_ref())),
        }
    }

    let (input, status) = if cmd.is_empty() {
        if io::stdin().is_terminal() {
            return done(USAGE, 2);
        }
        let mut s = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut s) {
            return done(&format!("nix-explain: reading stdin: {e}"), 2);
        }
        (s, None)
    } else {
        // shortcut: stderr is buffered, so Nix's live progress shows only at the end; stream it if users miss it.
        match Command::new(&cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .output()
        {
            Ok(out) => (
                String::from_utf8_lossy(&out.stderr).into_owned(),
                Some(out.status.code().unwrap_or(1)),
            ),
            Err(e) => {
                return done(
                    &format!("nix-explain: failed to run `{}`: {e}", cmd[0]),
                    127,
                )
            }
        }
    };

    let workspace = std::env::current_dir().unwrap_or_default();
    let report = analyze(&input, &workspace);

    // Successful command (or clean piped log): pass the output through untouched.
    if status == Some(0) || (status.is_none() && !report.is_error()) {
        eprint!("{input}");
        return ExitCode::SUCCESS;
    }

    if raw && !json {
        eprintln!("{input}\n──── nix-explain ────");
    }
    let out = if json {
        to_json(&report, raw.then_some(&input))
    } else {
        render(&report, io::stdout().is_terminal())
    };
    let _ = writeln!(io::stdout(), "{out}");
    // Wrapped: mirror the command's exit code. Piped: 1 because an error was found.
    ExitCode::from(status.unwrap_or(1).clamp(1, 255) as u8)
}

fn done(msg: &str, code: u8) -> ExitCode {
    if code == 0 {
        println!("{msg}")
    } else {
        eprintln!("{msg}")
    }
    ExitCode::from(code)
}

fn render(r: &Report, color: bool) -> String {
    let paint = |code: &str, s: &str| {
        if color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    };
    let mut o = String::new();
    o += &paint(
        "1;31",
        r.headline
            .as_deref()
            .unwrap_or("error: (no Nix error message found)"),
    );
    o += "\n";
    for f in &r.frames {
        o += &format!(
            "\n  {} {}\n",
            paint("1;36", &format!("→ {}:{}:{}", f.file, f.line, f.col)),
            paint("2", &f.desc)
        );
        for l in &f.snippet {
            o += &format!("    {l}\n");
        }
    }
    if r.frames.is_empty() {
        o += "\n  (no frame points into your files)\n";
    }
    if r.hidden > 0 {
        o += &paint(
            "2",
            &format!(
                "\n  {} internal nixpkgs/store frame(s) hidden — use --raw to see everything\n",
                r.hidden
            ),
        );
    }
    match &r.diagnosis {
        Some(d) => {
            o += &format!(
                "\n{} {}\n{}\n",
                paint("1;33", "Cause:"),
                d.cause,
                paint("1;32", "Fix:")
            );
            for l in d.fix.lines() {
                o += &format!("  {l}\n");
            }
        }
        None => o +=
            "\nNo known rule matched. Please open an issue with this log so a rule can be added.\n",
    }
    o.trim_end().to_string()
}

fn to_json(r: &Report, raw: Option<&String>) -> String {
    let frames: Vec<_> = r
        .frames
        .iter()
        .map(|f| json!({ "file": f.file, "line": f.line, "column": f.col, "description": f.desc, "snippet": f.snippet }))
        .collect();
    let mut v = json!({
        "error": r.headline,
        "frames": frames,
        "hidden_frames": r.hidden,
        "diagnosis": r.diagnosis.as_ref().map(|d| json!({ "rule": d.rule, "cause": d.cause, "fix": d.fix })),
    });
    if let Some(raw) = raw {
        v["raw"] = json!(raw);
    }
    serde_json::to_string_pretty(&v).unwrap()
}
