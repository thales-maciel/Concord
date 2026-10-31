# Build performance review

Initial read-only investigation of the repository and Windows CI runs while preparing the League
screen-share handoff testing build. The findings and timings below describe the original pipeline.
The implementation follow-up records the subsequent fixes; no Windows speedup has been benchmarked
yet.

## Summary

There is avoidable overhead beyond Rust's normal compilation cost. The biggest issues are the
build profiles, repeated compilation, and cache effectiveness—not the League handoff change.

A cold multimedia build has legitimate costs, but the current pipeline stacks expensive
configurations and repeated work. Fixing those should come before broad architectural rewrites.

## Implementation follow-up

The build configuration now addresses the avoidable work identified here:

| Finding | Change |
|---|---|
| Release tests followed by a different panic strategy for the app | Windows testing builds use `--profile testing` for both commands, with unwinding throughout. Packaging still requires tests to pass. |
| Shipping optimization costs imposed on testing | The new profile inherits dev, uses optimization level 2, disables LTO, and uses 16 code-generation units. Audio/inference dependencies stay optimized. The shipping profile is unchanged. |
| Redundant workspace build | Desktop CI tests the full workspace, then builds only the actual app executable; Windows version tags build the shipping app instead of an additional testing app. |
| Unconditional tract inference stack | `maximum-ai` is a default feature in both app and client core. A selected-package `--no-default-features` app build omits DeepFilterNet/tract. The UI explains this; saved Maximum AI preferences are preserved while processing uses Light AI. |
| Separate, poorly observable Windows caches | Desktop workflows share `desktop-v2`, include workspace artifacts, report exact-hit results, and save from main even on failure. Profiles remain distinct; caches are not assumed to be interchangeable or universally available across refs. |
| No compiler attribution | Desktop commands emit Cargo HTML timings, uploaded even on failure. The manual Windows workflow offers cache bypass and an immediate warm repeat, with named reports for app compilation, warm app compilation, and test compilation. |
| Two crypto providers | Client Reqwest uses `rustls-no-provider` with explicit ring initialization. AWS-LC and its native build dependencies disappear from the resolved lockfile. A certificate-verified loopback HTTPS/WSS test guards client initialization independently of the server. |
| GUI automation in normal app builds | `autotest` is opt-in, including its module tree, state, and frame tick. A normal binary rejects `--autotest` before migration/settings access. CI still checks/tests the automation feature; GUI/media tooling documents the dedicated build. |
| CLI-only Clap in app builds | `vox-cli` requires the `cli` feature; Clap is optional. Full-feature CI still checks/tests it. |

The manual workflow now defaults to `main` and accepts a ref, not an obsolete version tag by
default: old tagged manifests do not contain the new profile. Testing ZIPs identify their commit,
feature variant, and non-shipping profile. Full audio features remain the default; lightweight
packages explicitly identify the missing feature.

### Windows console regression and packaging guard

The first testing ZIP (`23e4f6e`) exposed a regression introduced by the new profile: the client
selected the Windows GUI subsystem only when `debug_assertions` was false, but `testing` inherits
dev assertions. Inspection confirmed that the published executable had console subsystem **3**.
That console belongs to the client process; closing it can terminate the client.

The client now selects GUI subsystem **2** on Windows independently of profile, optimizations,
or automation features. Both workflows check the actual linked PE executable and the copied
package executable; a console-subsystem client is a hard packaging failure. The verifier has
host-independent tests, including rejection of console binaries and malformed headers. The
existing Windows LiveKit and PowerShell helper launchers were audited and already set
`CREATE_NO_WINDOW`. The fix does not hide or terminate somebody else's terminal.

See [Contributing](../CONTRIBUTING.md#faster-desktop-iteration) for commands and the cold/warm
measurement procedure. The Windows app is built **before** tests so a cache-bypassed app timing
is truly cold; tests still gate packaging. A warm no-change build demonstrates reuse, not edited
crate compilation speed. Cargo reports must be compared with step durations to separate compiler
work from actual test execution, downloads, and cache transfer overhead.

Remaining work is measurement, not a promised speedup: run the revised Windows workflows, inspect
cache hit/size/transfer logs and critical paths, and validate real audio/video with both testing
and shipping profiles. Built-in hosting, the six-job local memory limit, and the shipping
optimization settings are intentionally retained. Persistent runners/compiler caches and further
dependency or hosting splits should follow measured bottlenecks, not precede them.

### Local validation of the follow-up

- Formatting and Actionlint workflow validation pass.
- Portable Clippy (`-D warnings`) and the portable library/server test suite pass.
- Full-feature and lightweight app/client-core binaries and tests type-check on the Linux host
  with explicit `eframe/x11`, `winit/x11`, and `softbuffer/x11` dependency features. This is a
  compile check, not a claim of Linux desktop product support or native Windows/macOS validation.
- The full client-core unit suite passes serially: 199 passed, 4 ignored. An initial parallel
  run had two timing-sensitive Maximum AI failures (`ai_max_muting_flushes_old_audio` and
  `ai_max_odd_chunk_sizes_are_adapted_with_one_frame_of_delay`); the focused denoiser suite passed
  all 12 tests, and the serial full run passed. These failures are recorded, not filtered out of
  CI or treated as proof of hardware/runtime equivalence.
- Lightweight denoiser tests pass (7), including saved-preference fallback; audio-path allocation
  tests pass in full (3) and lightweight (2) builds. Verified loopback HTTPS/WSS passes.
- The `testing` profile is exercised by the portable protocol suite (74 tests passed).
- Windows and macOS feature graphs retain DeepFilterNet/tract by default, omit them in the
  lightweight app, and omit AWS-LC/Clap in both normal app variants.

Native Windows/macOS builds, packaging, cold/warm timings, and real-media validation remain to
be run on their supported hosts. Local dependency compilation durations are not a Windows
before/after benchmark.

## Observed timings (original pipeline)

Windows CI run: https://github.com/thales-maciel/Concord/actions/runs/40990978353

| Work | Time |
|---|---:|
| Clippy/check compilation | 10 minutes 16 seconds |
| Compiling test executables | 14 minutes 44 seconds |
| Running tests before the unrelated failure | Approximately 1 minute 20 seconds |

The app's 242 tests and client core's 202 tests passed in that run. The workflow subsequently
failed in a standalone-server download-script integration test, preventing packaging.

Windows-only testing run: https://github.com/thales-maciel/Concord/actions/runs/40993953366

Its release-test step took 33 minutes 45 seconds, then started a separate release-app build.
That step includes compilation and test execution; it is not a per-crate compiler measurement.

Compiler log timestamps indicate when individual crates start, not how much exclusive time each
one consumes. Cargo timing reports are needed to attribute the critical path precisely.

## Findings, in priority order

### 1. Expensive compilation is being performed more than once

Relevant files:

- `.github/workflows/ci.yml`
- `.github/workflows/windows-build.yml`
- `Cargo.toml`

The general desktop pipeline runs Clippy, compiles/runs tests, builds the debug workspace, and
then builds the release app on version tags. Some work is reusable, but checking, generating
machine code, and compiling a different profile are not the same workload.

The Windows-only workflow added during this task runs release tests before building the release
app. The workspace release profile specifies:

```toml
panic = "abort"
```

Cargo's test harness requires unwinding. Tests ignore the profile's abort setting and force
their dependencies to build with the unwind strategy. The subsequent release-app build uses
abort. Consequently, using `--release` for both commands does not make their compiled
dependencies fully reusable.

**Responsibility:** this Windows-only workflow choice was introduced by the coding assistant
while trying to obtain a test package. It added avoidable latency; it was not inherited from
the original project.

**Recommendation:** run ordinary correctness tests and build the optimized distributable
separately. Do not apply maximum shipping optimizations to every test executable. For
friend-testing builds, use a consistent, faster optimized profile for tests and the app to
improve dependency reuse. Test executables and the app still require separate compilation and
linking; profile alignment does not eliminate all duplicate work.

Reference: https://doc.rust-lang.org/cargo/reference/profiles.html#panic

### 2. The release profile prioritizes optimization over build speed

Relevant file: `Cargo.toml`, `[profile.release]`.

```toml
opt-level = 3
lto = "thin"
codegen-units = 1
```

These are legitimate shipping tradeoffs, but expensive for iteration:

- One code-generation unit reduces parallel code generation within each crate.
- ThinLTO adds optimization work at link time.
- Release tests produce multiple executables, multiplying some optimization/linking work.

**Recommendation:** add a testing profile with LTO disabled and more code-generation units
(for example, 16), while retaining optimized audio processing. Keep the existing shipping
profile until runtime benchmarks justify changing it. Measure audio and video performance
before assuming a faster build profile is an equivalent release configuration.

### 3. Maximum AI's heavyweight inference dependencies are unconditional

Relevant files:

- `crates/vox-client-core/Cargo.toml`
- `vendor/deep_filter/Cargo.toml`
- `crates/vox-client-core/src/audio/dfn.rs`

The client core depends unconditionally on DeepFilterNet, which depends on the tract inference
stack. Every desktop build therefore includes ONNX loading, graph-processing machinery,
inference kernels, FFT dependencies, and associated dependencies—even when the user never
selects Maximum AI noise suppression.

The model is loaded lazily at runtime, which is useful for runtime resource consumption, but
does not avoid compiling its implementation.

**Recommendation:** make Maximum AI a compile-time feature. Keep it enabled in full releases,
but allow lightweight development/testing builds to omit it. Such a build would intentionally
have fewer features; this is not a transparent optimization.

The existing release overrides that optimize several tract graph/loading crates for size,
while retaining optimized inference kernels, are a sensible targeted measure already present.

### 4. Caching has not been effective across these attempts

Relevant files: both GitHub Actions workflows.

The previous Windows run explicitly logged `No cache found`. Earlier failed runs also did not
save their caches. Saving desktop caches on failure has since been enabled.

Other limitations remain:

- GitHub restricts cache sharing across branches and tags.
- The Windows-only workflow uses a separate cache key.
- The Rust cache action excludes workspace compilation artifacts by default.
- CI disables incremental compilation.
- Debug and release profiles have different artifacts; a warm debug cache does not constitute
  a warm release cache.

**Recommendation:** maintain a consistently warmed Windows cache on `main`, deliberately
organize debug/release cache keys, and inspect hit/miss logs. Consider persistent runners or
compiler caching if frequent builds justify them.

Simply enabling incremental compilation on disposable runners is not automatically a fix.
Cache size, upload/download cost, invalidation, and compiler-cache compatibility with the
chosen profiles must be considered.

Reference: https://github.com/Swatinem/rust-cache#cache-details

### 5. Both major rustls crypto backends are built

Relevant files:

- `crates/vox-client-core/Cargo.toml`, the Reqwest dependency
- `hub/Cargo.toml`, the Reqwest/rustls dependencies

The client core's Reqwest `rustls` feature enables AWS-LC. Other dependencies enable ring.
The Windows feature graph confirms both providers, and CI logs show AWS-LC compilation.

The server manifest already tries to avoid AWS-LC, but the combined desktop dependency graph
brings it back through the client.

**Recommendation:** investigate standardizing on one TLS provider, with deliberate rustls
provider initialization and verification that HTTPS/WebSocket functionality is preserved.
This is a concrete dependency-cleanup opportunity, but its build-time savings are unmeasured.

### 6. GUI automation tooling is part of normal app builds

Relevant files:

- `app/src/main.rs`, unconditional `mod autotest;`
- `app/src/autotest.rs` and its child modules

The source inventory contains approximately 14,000 lines across 22 autotest source files, out
of approximately 61,000 lines across the app's Rust source files. These are source counts,
not measurements of executable size or Windows-only compiled lines.

The automation modules are wired into the normal app module tree rather than being isolated
behind a feature. Their runtime `--autotest` switch does not remove the compilation work.

**Recommendation:** put GUI automation behind a compile-time feature and retain a dedicated
automation build. Avoid removing the useful automation coverage merely to reduce build time.

## Additional observations

- The desktop app directly includes the hub/server library for built-in hosting. Building just
  `voxhub-app` still compiles that hosting implementation and dependencies such as SQLite and
  Axum. This supports a real product feature, not an inherently mistaken dependency. A
  lightweight client-only feature could be considered if losing built-in hosting is acceptable
  for that development build.
- `clap` is an unconditional client-core dependency, although its direct source usage is in
  `src/bin/vox-cli.rs`. Separating CLI-only dependencies is a smaller cleanup opportunity, not
  the leading explanation for 30-minute builds.
- The dependency graph contains multiple versions of some libraries. These can add work, but
  transitive version differences are not automatically removable or an antipattern. Prioritize
  demonstrated heavyweight duplication rather than pursuing a zero-duplicate graph.
- `.cargo/config.toml` limits builds to six jobs to avoid exhausting memory. This is reasonable
  on smaller machines. Benchmark runner-specific parallelism instead of assuming a larger
  job count will always be faster.
- Native compilation is real: the graph includes WebRTC's C++ bindings, crypto libraries,
  bundled SQLite, and WebP. However, the WebRTC engine itself is downloaded prebuilt by
  `webrtc-sys-build`; the project is not compiling the entire WebRTC engine from source.
- No evidence from this review points to a slow project resource-generation script as the
  dominant problem. The Windows icon/version resource script has narrow rerun declarations.

## Recommended order of work

1. Fix the repeated-build/profile mismatch in the Windows testing workflow.
2. Introduce a fast optimized testing profile and make Windows caching predictable.
3. Collect Cargo `--timings` reports for both cold and warm Windows builds, distinguishing
   checking, code generation, linking, downloads, and actual test execution.
4. Feature-gate heavyweight optional functionality and GUI automation tooling.
5. Audit crypto-provider duplication and smaller CLI-only/dependency cleanup opportunities.
6. Reassess runner resources and compiler caching using measured bottlenecks.

Do not promise a particular speedup before measuring. Preserve full release checks and
real-media performance validation; the goal is to avoid unnecessary work, not to conceal
failures or silently degrade the shipping app.
