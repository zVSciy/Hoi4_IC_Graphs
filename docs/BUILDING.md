# Building HOI4 IC Analyzer from source

This guide takes you from a fresh Windows PC to a running, self-built copy of the analyzer, and
explains how the release package is made.

- [1. Prerequisites](#1-prerequisites)
- [2. Get the code](#2-get-the-code)
- [3. Build and run](#3-build-and-run)
- [4. Checks before a pull request](#4-checks-before-a-pull-request)
- [5. Feature flags and build settings](#5-feature-flags-and-build-settings)
- [6. Packaging like the release workflow](#6-packaging-like-the-release-workflow)
- [7. Troubleshooting](#7-troubleshooting)

## 1. Prerequisites

The analyzer is a Windows application (Windows 10 or 11, 64-bit).

1. **Visual Studio Build Tools** (or Visual Studio) with the **Desktop development with C++**
   workload. It provides the MSVC compiler and linker; SQLite and zstd are compiled from bundled
   C sources.
2. **Rust**, installed with [rustup-init.exe](https://rustup.rs) (accept the default
   `x86_64-pc-windows-msvc` host). The exact Rust version is pinned in
   [`rust-toolchain.toml`](../rust-toolchain.toml); `rustup` downloads it automatically the first
   time you run `cargo` in the project folder, together with `rustfmt` and `clippy`.
3. **Git** from [git-scm.com](https://git-scm.com/download/win).

Open a new terminal after installing, so the tools are on your `PATH`.

## 2. Get the code

```powershell
git clone https://github.com/zVSciy/Hoi4_IC_Graphs.git
cd Hoi4_IC_Graphs
```

## 3. Build and run

```powershell
cargo run                 # debug build: quick to compile, shows a console with the log
cargo run --release       # optimised build, as shipped
```

The executable ends up in `target\debug\` or `target\release\` as `HOI4_Analyzer.exe`. Data files
can be passed on the command line:

```powershell
cargo run --release -- path\to\output.txt [path\to\second.txt]
```

> [!NOTE]
> The release profile uses fat LTO and a single codegen unit, and compresses the bundled DLLs at
> the highest zstd level, so the first release build takes a few minutes.

To try the app without touching your real settings and game library, point it at scratch files:

```powershell
$env:HOI4_ANALYZER_SETTINGS = "$env:TEMP\settings.json"
$env:HOI4_ANALYZER_LIBRARY = "$env:TEMP\library.sqlite3"
cargo run
```

## 4. Checks before a pull request

CI runs these on every push and pull request; run them locally first:

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Two extra tools for performance work:

```powershell
cargo bench --bench parse                                          # parser benchmarks (Criterion)
cargo run --release --example parse_file -- path\to\output.txt     # load timing for a real file
```

## 5. Feature flags and build settings

| Cargo feature | Default | Effect |
| --- | :---: | --- |
| `embed-dlls` | on | Compresses every `DataCollection\DataCollection_<version>.dll` with zstd and embeds it in the executable, so a single `.exe` covers all supported game versions. Build with `--no-default-features` to leave the DLLs out; the app then uses DLLs from a `DataCollection\` folder next to it or ones added in Settings. |
| `admin-menu` | off | Maintainer-only. Also embeds the DLLs in `AdminMenu\`. Not used for public releases. |

| Environment variable | Read at | Effect |
| --- | --- | --- |
| `HOI4_ANALYZER_RELEASE` | build time | Release tag baked into the binary (e.g. `v3.2.0`). Only builds with a tag check GitHub for newer releases. Set by the release workflow. |
| `HOI4_DLL_ZSTD_LEVEL` | build time | zstd level for embedded DLLs. Default 19 for release builds, 3 for debug builds. |
| `HOI4_ANALYZER_SETTINGS` | run time | Path of the settings file to use instead of the default one. |
| `HOI4_ANALYZER_LIBRARY` | run time | Path of the game library database to use instead of the default one. |

Adding support for a new HOI4 version needs no code change: put the new
`DataCollection_<version>.dll` into `DataCollection\` and rebuild. The version is read from the
file name.

## 6. Packaging like the release workflow

Releases are built by [`.github/workflows/release.yml`](../.github/workflows/release.yml). To
produce the same package locally:

```powershell
$env:HOI4_ANALYZER_RELEASE = "v3.2.0"
cargo build --release --locked
Compress-Archive -Path target\release\HOI4_Analyzer.exe, README.md, LICENSE `
  -DestinationPath HOI4_Analyzer-v3.2.0-windows-x64.zip
Get-FileHash HOI4_Analyzer-v3.2.0-windows-x64.zip -Algorithm SHA256
```

### Publishing a release (maintainers)

1. Update `version` in `Cargo.toml` and move the `Unreleased` entries in `CHANGELOG.md` under the
   new version.
2. Commit, then tag and push: `git tag v3.2.0; git push origin v3.2.0`.
3. The release workflow runs the CI checks, builds and signs the executable, writes
   `SHA256SUMS.txt` and publishes the GitHub release with generated notes. It can also be started
   by hand from the **Actions** tab (**Release → Run workflow**) with a tag name.

## 7. Troubleshooting

**`link.exe` not found / `cannot open input file 'kernel32.lib'`**
The C++ build tools are missing. Install the *Desktop development with C++* workload and open a
new terminal.

**`error: failed to run custom build command for libsqlite3-sys` or `zstd-sys`**
No C compiler was found. Same fix as above.

**Antivirus quarantines the executable**
The executable contains the DataCollection DLLs and code to load them into `hoi4.exe`, which some
scanners flag on heuristics. Build with `--no-default-features` to leave the DLLs out, or add an
exception for your own build folder.

**Country names are missing for a mod**
Built-in names cover the base game. Tags without a built-in name use the game's own localisation,
read from the HOI4 installation the app finds through Steam. Send a pull request to
[`src/countries.rs`](../src/countries.rs) to add names.
