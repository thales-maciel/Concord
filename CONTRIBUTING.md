# Contributing

## Prerequisites

- Rust stable with `rustfmt` and `clippy`.
- Windows: MSVC and Visual Studio C++ Build Tools with the Windows SDK.
- macOS: Apple Silicon, macOS 13+, and Xcode Command Line Tools.

## Checks

On Windows or macOS:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-features --lib --bins --tests
cargo test --locked --profile testing --workspace --all-features --lib --bins --tests
cargo build --locked --profile testing -p voxhub-app
```

Linux currently supports only the portable subset; see [CI](.github/workflows/ci.yml)
for its commands. Explicit targets exclude platform-specific developer examples.

Internet/router and real-media tests are ignored by default. Historical migration tests
skip without old executables; real Keychain checks may skip without a usable Keychain.
The Windows GUI/media gate is separate: see `tools/gate.ps1` for prerequisites and options.

## Faster desktop iteration

`--profile testing` uses optimized code (`opt-level=2`), no LTO, 16 code-generation units,
and unwinding for **both** tests and the app. Audio/inference dependencies remain optimized.
This avoids the release-test/unwind versus release-app/abort rebuild, without changing the
shipping `release` profile. It is not a performance-equivalent release: validate real audio/video
before distributing a shipping build. Omit `--profile testing` for normal debug iteration.

Full app builds include **Maximum AI** noise suppression by default. For a deliberately
lightweight build without DeepFilterNet/tract:

```sh
cargo build --locked --profile testing -p voxhub-app --no-default-features --timings
```

The UI omits Maximum AI and explains the limitation. An existing Maximum AI preference is
retained in settings but runs Light AI in this build, so returning to a full build restores it.
Select `-p voxhub-app`, not a default-feature workspace build that could re-enable client-core
features through Cargo feature unification.

GUI scenario drivers are excluded from normal builds. `--autotest` fails explicitly unless
built with `--features autotest`. To run `tools/gate.ps1`, `tools/shots.ps1`, or `tools/perf.ps1`,
keep their shipping-profile comparison and build a dedicated automation executable:

```sh
cargo build --locked --release --workspace --all-features
```

The CLI and its Clap dependency are also opt-in:

```sh
cargo run --locked -p vox-client-core --features cli --bin vox-cli -- --help
```

CI checks/tests the full feature set, checks the lightweight app, and tests its denoiser fallback.
The full workspace test suite and shipping Windows release build still gate tagged releases.

## Windows testing packages and build measurements

The **Windows testing build** workflow defaults to `main`; its `ref` input also accepts a commit
or version tag **containing the testing profile**. Old release tags cannot gain a new Cargo
profile from the workflow. Version tags are checked against the workspace version. ZIP names
include version, commit SHA, feature variant, and `testing`; they are not shipping releases.
Packaging requires app/core tests to pass. `lightweight` defaults to false.

For a cold/warm comparison on the same ref, dispatch with `cold-cache=true` and
`measure-warm=true`. The workflow skips cache restore/save, builds the app first from an empty
hosted runner, repeats the unchanged app build, then runs the tests. Download the
`cargo-timings-windows-*` artifact: `windows-app.html`, `windows-warm.html`, and
`windows-tests.html` preserve the individual reports. GitHub step durations include test
execution and other overhead; Cargo timing reports describe compilation, not test runtime.
A warm no-change build measures artifact reuse, not the latency of an edited crate.

Both desktop workflows share the `desktop-v2` Rust-cache namespace, including workspace
artifacts. Only main saves it (also on failure); PRs and tags restore main's cache subject to
GitHub's ref restrictions. Cargo still distinguishes profiles, features, and panic strategies.
Main normally warms check/testing artifacts, not shipping release code generation; a warm testing
cache does not eliminate the first release compilation.
Inspect the restore logs for exact/fallback hits, transfer time, and cache size. Cache storage
is bounded and incremental artifacts are not cached; incremental compilation stays disabled in
CI. Timing reports are uploaded even when a later step fails. Compare cold, restored-cache, and
warm timings before changing job counts or adding a compiler cache/persistent runner.

See [the build review](docs/build-performance-review.md) for the original measurements and
implementation follow-up. No Windows speedup is claimed until the revised workflows are run.

## Version tags

`vX.Y.Z` tags (optionally with a prerelease suffix) must match the workspace version and
reference a commit on `main`. After all checks pass, tag CI publishes a Windows x64 ZIP
and SHA-256 checksums to GitHub Releases. Tags with a prerelease suffix are marked as
prereleases. The Windows ZIP is also uploaded as a CI artifact as soon as its job succeeds.
