# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Built-in English names for 328 country tags, so names show without HOI4 installed and are the
  same on every machine. Germany is shown as *German Reich* and the Soviet Union as
  *Soviet Union*. Countries without a built-in name (for example from mods) still use the
  game's localisation.
- Public source code under the MIT License, with documentation (build guide, architecture,
  contributing guide, security policy) and CI for every push and pull request.
- Project website (`site/`, published to GitHub Pages): a tour of the source code and an
  interactive 2D / 3D graph of the code base, in GitHub's dark colours.

### Changed

- Releases are now published in this repository. The in-app update check and the **Releases**
  link follow. The old `Hoi4GraphToolReleases` repository receives no new releases.
- Release package renamed to `HOI4_Analyzer-<version>-windows-x64.zip`, with checksums in
  `SHA256SUMS.txt`.

## [3.1.0]

Version at the time the source code was published. Earlier release notes are in the
[old releases repository](https://github.com/zVSciy/Hoi4GraphToolReleases/releases).

[Unreleased]: https://github.com/zVSciy/Hoi4_IC_Graphs/compare/v3.1.0...HEAD
[3.1.0]: https://github.com/zVSciy/Hoi4_IC_Graphs/releases/tag/v3.1.0
