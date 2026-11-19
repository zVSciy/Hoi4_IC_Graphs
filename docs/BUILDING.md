# Building HOI4 IC Analyzer from source

This guide takes you from a fresh machine to a running, self-built copy of the analyzer on
Windows, Linux or macOS, and explains how the release packages are made.

- [1. Prerequisites](#1-prerequisites)
- [2. Get the code](#2-get-the-code)
- [3. Build and run](#3-build-and-run)
- [4. Checks before a pull request](#4-checks-before-a-pull-request)
- [5. Feature flags and build settings](#5-feature-flags-and-build-settings)
- [6. Packaging like the release workflow](#6-packaging-like-the-release-workflow)
- [7. Troubleshooting](#7-troubleshooting)

## 1. Prerequisites

Everything is built with Cargo. The exact Rust version is pinned in
[`rust-toolchain.toml`](../rust-toolchain.toml); `rustup` downloads it automatically the first
time you run `cargo` in the project folder, together with `rustfmt` and `clippy`.

SQLite and zstd are compiled from bundled C sources, so you also need a C compiler. No other
system libraries are needed at build time.

### Windows 10/11

1. Install **Visual Studio Build Tools** (or Visual Studio) with the
   **Desktop development with C++** workload. This provides the MSVC compiler and linker.
2. Install Rust with [rustup-init.exe](https://rustup.rs) and accept the default
   `x86_64-pc-windows-msvc` host.
3. Install [Git](https://git-scm.com/download/win).

### Linux

Install Git, a C toolchain and `curl`, then Rust:

```sh
# Debian / Ubuntu
sudo apt install git build-essential curl
# Fedora
sudo dnf install git gcc make curl
# Arch
sudo pacman -S git base-devel curl

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

At **runtime** the app loads the usual desktop libraries dynamically: X11 or Wayland,
`libxkbcommon` and OpenGL/EGL (Mesa or your GPU driver). Every desktop installation has them;
on a minimal system install `libxkbcommon0 libgl1 libegl1` (Debian/Ubuntu names). File
dialogs use the XDG desktop portal (`xdg-desktop-portal` plus the GTK or KDE backend).

### macOS 11 or newer

```sh
xcode-select --install          # Apple's C compiler and linker
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## 2. Get the code

```sh
git clone https://github.com/zVSciy/Hoi4_IC_Graphs.git
cd Hoi4_IC_Graphs
```

## 3. Build and run

```sh
cargo run                 # debug build: quick to compile, shows a console with the log on Windows
cargo run --release       # optimised build, as shipped
```

The executable ends up in `target/debug/` or `target/release/` (`HOI4_Analyzer.exe` on Windows,
`HOI4_Analyzer` elsewhere). Data files can be passed on the command line:

```sh
cargo run --release -- path/to/output.txt [path/to/second.txt]
```

> The release profile uses fat LTO and a single codegen unit, so the first release build takes a
> few minutes. On Windows it also compresses the bundled DLLs at the highest zstd level.

To try the app without touching your real settings and game library, point it at scratch files:

```sh
# Linux / macOS
HOI4_ANALYZER_SETTINGS=/tmp/settings.json HOI4_ANALYZER_LIBRARY=/tmp/library.sqlite3 cargo run
```

```powershell
# Windows PowerShell
$env:HOI4_ANALYZER_SETTINGS = "$env:TEMP\settings.json"
$env:HOI4_ANALYZER_LIBRARY = "$env:TEMP\library.sqlite3"
cargo run
```

## 4. Checks before a pull request

CI runs these on Windows, Linux and macOS; run them locally first:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Two extra tools for performance work:

```sh
cargo bench --bench parse                                          # parser benchmarks (Criterion)
cargo run --release --example parse_file -- path/to/output.txt     # load timing for a real file
```

## 5. Feature flags and build settings

| Cargo feature | Default | Effect |
| --- | :---: | --- |
| `embed-dlls` | on | Compresses every `DataCollection/DataCollection_<version>.dll` with zstd and embeds it in the Windows executable, so a single `.exe` covers all supported game versions. Ignored for Linux and macOS builds. Build with `--no-default-features` to leave the DLLs out; the app then uses DLLs from a `DataCollection/` folder next to it or ones added in Settings. |
| `admin-menu` | off | Maintainer-only. Also embeds the DLLs in `AdminMenu/`. Not used for public releases. |

| Environment variable | Read at | Effect |
| --- | --- | --- |
| `HOI4_ANALYZER_RELEASE` | build time | Release tag baked into the binary (e.g. `v3.2.0`). Only builds with a tag check GitHub for newer releases. Set by the release workflow. |
| `HOI4_DLL_ZSTD_LEVEL` | build time | zstd level for embedded DLLs. Default 19 for release builds, 3 for debug builds. |
| `HOI4_ANALYZER_SETTINGS` | run time | Path of the settings file to use instead of the default one. |
| `HOI4_ANALYZER_LIBRARY` | run time | Path of the game library database to use instead of the default one. |

Adding support for a new HOI4 version needs no code change: put the new
`DataCollection_<version>.dll` into `DataCollection/` and rebuild. The version is read from the
file name.

## 6. Packaging like the release workflow

Releases are built by [`.github/workflows/release.yml`](../.github/workflows/release.yml). To
produce the same packages locally:

### Windows

```powershell
$env:HOI4_ANALYZER_RELEASE = "v3.2.0"
cargo build --release --locked
Compress-Archive -Path target\release\HOI4_Analyzer.exe, README.md, LICENSE `
  -DestinationPath HOI4_Analyzer-v3.2.0-windows-x64.zip
```

### Linux

```sh
HOI4_ANALYZER_RELEASE=v3.2.0 cargo build --release --locked
mkdir -p package/HOI4_Analyzer-v3.2.0
cp target/release/HOI4_Analyzer README.md LICENSE package/HOI4_Analyzer-v3.2.0/
tar -C package -czf HOI4_Analyzer-v3.2.0-linux-x64.tar.gz HOI4_Analyzer-v3.2.0
```

Binaries built on a newer distribution need at least the same glibc version to run. Build on the
oldest distribution you want to support (the release workflow uses Ubuntu 24.04).

### macOS (universal app bundle)

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
export HOI4_ANALYZER_RELEASE=v3.2.0
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin
lipo -create -output HOI4_Analyzer \
  target/aarch64-apple-darwin/release/HOI4_Analyzer \
  target/x86_64-apple-darwin/release/HOI4_Analyzer
```

Then wrap the binary in `HOI4 Analyzer.app/Contents/MacOS/` with an `Info.plist`. The release
workflow shows the exact steps, including the icon conversion. The bundle is ad-hoc signed but
not notarized.

### Publishing a release (maintainers)

1. Update `version` in `Cargo.toml` and move the `Unreleased` entries in `CHANGELOG.md` under the
   new version.
2. Commit, then tag and push: `git tag v3.2.0 && git push origin v3.2.0`.
3. The release workflow builds all three platforms, writes `SHA256SUMS.txt` and publishes the
   GitHub release with generated notes. It can also be started by hand from the **Actions** tab
   (**Release → Run workflow**) with a tag name.

## 7. Troubleshooting

**`link.exe` not found / `cannot open input file 'kernel32.lib'` (Windows)**
The C++ build tools are missing. Install the *Desktop development with C++* workload and open a
new terminal.

**`error: failed to run custom build command for libsqlite3-sys` or `zstd-sys`**
No C compiler was found. Install `build-essential` (Linux), the Xcode command line tools (macOS)
or the MSVC build tools (Windows).

**The window does not open on Linux (`NoGlutinConfigs`, `libxkbcommon` or `libEGL` not found)**
Install your distribution's OpenGL/EGL and `libxkbcommon` packages. On a remote session without
GPU acceleration, try `LIBGL_ALWAYS_SOFTWARE=1 ./HOI4_Analyzer`.

**File dialogs do nothing on Linux**
Install `xdg-desktop-portal` and a backend (`xdg-desktop-portal-gtk` or `xdg-desktop-portal-kde`).

**macOS says the app "cannot be opened" or "is damaged"**
Self-built and downloaded bundles are not notarized. Right-click the app and choose **Open**, or
run `xattr -dr com.apple.quarantine "HOI4 Analyzer.app"`.

**Antivirus quarantines the Windows executable**
The executable contains the DataCollection DLLs and code to load them into `hoi4.exe`, which some
scanners flag on heuristics. Build with `--no-default-features` to leave the DLLs out, or add an
exception for your own build.

**Country names are missing for a mod**
Built-in names cover the base game. Tags without a built-in name use the game's own localisation,
read from the HOI4 installation the app finds through Steam. Send a pull request to
[`src/countries.rs`](../src/countries.rs) to add names.
