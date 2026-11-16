# nix-explain

Nix errors are hard to read. Evaluation is lazy, so a typo in your `flake.nix` often fails
deep inside nixpkgs, and you get pages of `/nix/store/.../lib/modules.nix` frames.

`nix-explain` cleans that up. It:

1. **Hides internal frames.** Frames inside nixpkgs or the store are dropped. Only frames in *your* files are shown, with paths relative to your project.
2. **Explains the error.** You get one sentence on the cause and a fix you can copy and paste.

## Before / after

```text
$ nix-explain -- nixos-rebuild build --flake .
error: attribute 'prot' missing

  → configuration.nix:3:30 from call site
       2|   options.services.web.port = lib.mkOption { type = lib.types.int; };
       3|   config.services.web.port = config.services.web.prot;
        |                              ^

  2 internal nixpkgs/store frame(s) hidden — use --raw to see everything

Cause: You accessed `prot` on an attribute set that doesn't contain it.
Fix:
  Probably a typo: `prot` → `port`.
```

## Install

```bash
nix run github:tahmidbintaslim/nix-explain -- nix build
```

```bash
nix profile install github:tahmidbintaslim/nix-explain
```

Or build it from source with `cargo install --path .`.

## Usage

Wrap a command. The exit code is passed through, and successful runs print unchanged:

```bash
nix-explain -- nix build .#default
```

Or pipe output into it:

```bash
nix build 2>&1 | nix-explain
```

| Flag | Effect |
|------|--------|
| `--json` | Machine-readable report (`error`, `frames`, `hidden_frames`, `diagnosis`) for CI |
| `--raw`  | Also print the full, unedited Nix output |

When an error is found in piped mode, `nix-explain` exits with code 1.

## What it recognises

| Rule | Triggers on | Suggested fix |
|------|-------------|---------------|
| `git-untracked` | `is not tracked by Git` / `path '/nix/store/…-source/x' does not exist` | `git add x` |
| `unfree` | `has an unfree license` (old and nixpkgs 26.x wording) | `allowUnfreePredicate` for that package |
| `hash-mismatch` | fixed-output hash mismatch | swaps the `specified` hash for the `got` hash |
| `rust-cargo-hash` | mismatch in a `-vendor` derivation | `cargoHash = "sha256-…";` |
| `go-vendor-hash` | mismatch in a `-go-modules` derivation | `vendorHash = "sha256-…";` |
| `imports-recursion` | infinite recursion from using `config` in `imports` | names the module and the `mkIf` pattern |
| `infinite-recursion` | `infinite recursion encountered` | common causes, with the line shown |
| `attribute-missing` | `attribute 'x' missing` | the typo fix, using Nix's "Did you mean" hint |
| `rust-openssl` | `openssl-sys` build failure | `pkg-config` + `openssl` inputs |
| `rust-system-lib` | `The system library … was not found` / no `pkg-config` | `pkg-config` + the library |
| `go-version` | `go.mod requires go >= X` | `buildGoModule.override { go = go_X; }` |
| `python-build-backend` | missing setuptools/hatchling/poetry/… | `pyproject = true; build-system = [ … ];` |
| `python-module-not-found` | `ModuleNotFoundError` | adds the package to `dependencies` (knows that `yaml` is `pyyaml`, and similar) |
| `c-header-missing` | `fatal error: x.h: No such file` | the package that provides the header |

To see how a frame is classified: if a store path like `/nix/store/<hash>-source/<rel>` matches a file
`<rel>` in your current directory, it counts as yours. That covers flakes that get copied to the store.

## Development

```bash
nix develop
```

```bash
cargo test
```

Each `tests/fixtures/*.log` file is a real Nix error log, captured from Nix 2.34 and nixpkgs 26.11pre
(except `git-untracked-old.log`, which reproduces the message from older Nix versions). `tests/fixtures.rs` checks the rule, fix, and
user frame for each one. To add a rule, add an entry in `src/rules.rs`, then add a fixture and a test case.

## License

MIT © tahmidbintaslim
