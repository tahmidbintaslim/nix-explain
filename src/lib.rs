//! nix-explain: turn Nix's internal stack traces into the user's file, a cause, and a fix.

pub mod rules;
pub mod trace;

use std::path::Path;

pub use rules::Diagnosis;
pub use trace::Frame;

#[derive(Debug)]
pub struct Report {
    pub headline: Option<String>,
    /// Frames in the user's own files only.
    pub frames: Vec<Frame>,
    /// Number of nixpkgs / store frames hidden.
    pub hidden: usize,
    pub diagnosis: Option<Diagnosis>,
}

impl Report {
    pub fn is_error(&self) -> bool {
        self.headline.is_some() || self.diagnosis.is_some()
    }
}

pub fn analyze(raw: &str, workspace: &Path) -> Report {
    let text = trace::strip_ansi(raw);
    let (mut frames, internal): (Vec<_>, Vec<_>) = trace::frames(&text, workspace)
        .into_iter()
        .partition(|f| f.local);
    // Nix often prints the call site and the error site at the same position.
    frames.dedup_by(|b, a| (&a.file, a.line, a.col) == (&b.file, b.line, b.col));
    Report {
        headline: trace::headline(&text),
        frames,
        hidden: internal.len(),
        diagnosis: rules::diagnose(&text, workspace),
    }
}
