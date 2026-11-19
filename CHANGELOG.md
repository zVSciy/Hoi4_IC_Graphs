# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Linux and macOS builds. Loading, graphs, comparisons, rankings, the game library and the live
  dashboard work on all platforms; collecting data from a running game stays Windows-only.
- Built-in English names for 328 country tags, so names show without HOI4 installed and are the
  same on every machine. Germany is shown as *German Reich* and the Soviet Union as
  *Soviet Union*. Countries without a built-in name (for example from mods) still use the
  game's localisation.
- Public source code under the MIT License, with documentation (build guide, architecture,
  contributing guide, security policy) and CI on Windows, Linux and macOS.

### Changed

- Releases are now published in this repository. The in-app update check and the **Releases**
  link follow. The old `Hoi4GraphToolReleases` repository receives no new releases.
- Release packages: `HOI4_Analyzer-<version>-windows-x64.zip`, `-linux-x64.tar.gz` and
  `-macos-universal.zip`, with checksums in `SHA256SUMS.txt`.
- On Linux and macOS, settings and the game library are stored in the platform's data folder
  instead of the temporary folder.

## [3.1.0]

Version at the time the source code was published. Earlier release notes are in the
[old releases repository](https://github.com/zVSciy/Hoi4GraphToolReleases/releases).

[Unreleased]: https://github.com/zVSciy/Hoi4_IC_Graphs/compare/v3.1.0...HEAD
[3.1.0]: https://github.com/zVSciy/Hoi4_IC_Graphs/releases/tag/v3.1.0
