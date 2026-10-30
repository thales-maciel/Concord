# Contributing

## Prerequisites

- Rust stable with `rustfmt` and `clippy`.
- Windows: MSVC and Visual Studio C++ Build Tools with the Windows SDK.
- macOS: Apple Silicon, macOS 13+, and Xcode Command Line Tools.

## Checks

On Windows or macOS:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --lib --bins --tests
cargo test --locked --workspace --lib --bins --tests
cargo build --locked --workspace
```

Linux currently supports only the portable subset; see [CI](.github/workflows/ci.yml)
for its commands. Explicit targets exclude platform-specific developer examples.

Internet/router and real-media tests are ignored by default. Historical migration tests
skip without old executables; real Keychain checks may skip without a usable Keychain.
The Windows GUI/media gate is separate: see `tools/gate.ps1` for prerequisites and options.

## Version tags

`vX.Y.Z` tags (optionally with a prerelease suffix) must match the workspace version and
reference a commit on `main`. After all checks pass, tag CI publishes a Windows x64 ZIP
and SHA-256 checksums to GitHub Releases. Tags with a prerelease suffix are marked as
prereleases. The Windows ZIP is also uploaded as a CI artifact as soon as its job succeeds.
