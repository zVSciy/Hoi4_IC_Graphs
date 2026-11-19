# Contributing to HOI4 IC Analyzer

Thanks for helping! Bug reports, ideas, country names, documentation and code are all welcome.
By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Reporting bugs

Open a [bug report](https://github.com/zVSciy/Hoi4_IC_Graphs/issues/new?template=bug_report.yml)
and include:

- the app version (Settings → Updates) and your operating system,
- your HOI4 version and the DataCollection version you used,
- what you did, what you expected and what happened instead,
- the log: **Show Log Viewer** in the main window, then copy the relevant lines.

If a specific `output.txt` causes the problem, say so; you can attach a zipped copy or a
`.hoi4game` export from the game library.

Security problems are not reported in public issues; see [SECURITY.md](SECURITY.md).

## Suggesting features

Open a [feature request](https://github.com/zVSciy/Hoi4_IC_Graphs/issues/new?template=feature_request.yml).
Describe the problem you want to solve, not only the solution; it often leads to a better design.
For larger changes, please discuss the idea in an issue before writing code.

## Your first pull request

1. Fork the repository and create a branch from `main`
   (`git switch -c fix/rankings-sorting`).
2. Set up the toolchain as described in [docs/BUILDING.md](docs/BUILDING.md).
3. Make your change. Add or update tests where it makes sense.
4. Run the same checks as CI:

   ```sh
   cargo fmt
   cargo clippy --locked --all-targets -- -D warnings
   cargo test --locked
   ```

5. If users will notice the change, add a line under **Unreleased** in
   [CHANGELOG.md](CHANGELOG.md), and update the [user guide](docs/USER_GUIDE.md) if behaviour changes.
6. Open the pull request and fill in the template. CI builds and tests it on Windows, Linux and
   macOS.

Keep pull requests focused: one fix or feature per PR is much easier to review than a mix.

## Code guidelines

- **Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first**; it shows where things live.
- Formatting is whatever `cargo fmt` produces; Clippy warnings are errors.
- Keep it cross-platform. Windows-only code goes behind `#[cfg(windows)]` with a fallback for
  other platforms, and must not break the Linux or macOS build.
- Never panic on user data. Malformed lines, missing files and unreadable settings must lead to a
  message or a sensible default.
- The parser is performance-critical. Run `cargo bench --bench parse` before and after changing
  `src/data/`, and mention the numbers in the PR.
- New dependencies need a good reason; prefer small, well-maintained crates without C code.
- UI text is short, plain English, and says what the user can do next.

## Commit messages

Write the subject in the imperative mood, at most about 72 characters, for example
`Fix rankings order for equal values` or `Add Linux data folder`. Explain the *why* in the body
when it isn't obvious.

## Common contributions

### Country names

Built-in names live in [`src/countries.rs`](src/countries.rs), one `("TAG", "Name")` pair per
line, **sorted by tag**. Each name must be unique, so two countries never share a legend entry.
A unit test checks both rules. Tags without a built-in name fall back to the game's localisation.

### A new HOI4 version

Supporting a new game version needs a matching DataCollection DLL, not a code change. Maintainers
add `DataCollection/DataCollection_<version>.dll`; users can add a DLL themselves in
**Settings → DataCollection DLLs → Add DLL…**.

### Documentation and screenshots

Docs are Markdown in the repository root and in [`docs/`](docs/). Screenshots are in
[`docs/screenshots/`](docs/screenshots/). Use the dark default theme, and make sure no personal
paths or names are visible.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE)
of this project.
