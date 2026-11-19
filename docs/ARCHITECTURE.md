# Architecture

A map of the code for contributors. Each module also starts with a doc comment
(`cargo doc --open` renders them).

## The big picture

```text
 HOI4 (Windows)                         HOI4 IC Analyzer
┌──────────────────────┐   output.txt   ┌───────────────────────────────────────────────┐
│ hoi4.exe             │ ─────────────► │ data::parser → data::accumulator → Dataset    │
│  └ DataCollection DLL│  one line per  │        │                         │            │
└──────────────────────┘  country/day   │        ▼                         ▼            │
          ▲                             │ live (follows the file)   library (SQLite)    │
          │ inject::inject (Windows)    │        │                         │            │
          └──────────────────────────── │        └──────────► analysis ◄───┘            │
                                        │                        │                      │
                                        │            ui (egui)   ▼   export (HTML/CSV)  │
                                        └───────────────────────────────────────────────┘
```

1. **Collect (Windows only).** `inject` finds `hoi4.exe`, picks the DataCollection DLL made for
   the running game version and loads it into the game with `LoadLibraryW`. The DLL appends lines
   like `DLL: 12:00, 1 January, 1936 GER MIL 12 34.50` (time, date, tag, metric, units, IC) to
   `output.txt` in the game folder.
2. **Parse.** `data::parser` is a hand-written single-pass parser (no regex, no allocations per
   line). `data::accumulator` sums the records into a grid of countries × days, and
   `data::Dataset` is the immutable result.
3. **Store.** `library` keeps every dataset in a SQLite database, compressed with the compact
   binary format in `library::codec`, so old games open without re-parsing. `live` follows a file
   that is still being written and only reads the new bytes.
4. **Analyse.** `analysis` turns one or more datasets into daily and cumulative series per
   country or group (`analysis::factions`: Axis/Allies or user-defined groups, names, colours).
5. **Show.** `ui` is the egui front end; each graph, rankings, library and settings window is its
   own OS window (egui viewport). `export` writes the same series as interactive HTML (Plotly)
   or CSV.

## Modules

| Module | Responsibility |
| --- | --- |
| `main.rs` | Entry point: logger, crash dialog, command-line files, starts the UI. |
| `analysis` | Datasets → plottable series; `factions` for groups, labels and stable colours. |
| `countries` | Built-in English country names, one per tag (generated table, sorted by tag). |
| `data` | Output-file format: `parser`, `accumulator`, `dataset`, `date` (days as integers), `tag` (country tags packed in a `u64`). |
| `export` | CSV and standalone HTML export. |
| `game` | The HOI4 installation: Steam library lookup, game version, country names (built-in table plus the game's localisation), `GameWatcher` for game start/exit. |
| `inject` | DLL catalogue (user-added, next to the executable, embedded) and injection. `win` holds the Win32 code; `embedded` the DLLs compiled in by `build.rs`. |
| `library` | SQLite game library and its binary `codec`. |
| `live` | Incremental reading of a growing file. |
| `logging` | Logger for stderr and the in-app log viewer. |
| `paths` | The app's data folder (`%LOCALAPPDATA%\HOI4_Analyzer`). |
| `settings` | User settings, stored as JSON. |
| `theme` | Colours shared by the native UI and the HTML export. |
| `ui` | Windows: `app` (main window), `graph_window`, `chart_panel`/`chart`, `live_dashboard`, `rankings_window`, `library_window`, `settings_window`, `dialogs`, `style`. |
| `update` | Release check against the GitHub API (release builds only, notice only). |

## Build script

[`build.rs`](../build.rs):

- compresses `DataCollection/*.dll` (and `AdminMenu/*.dll` with the `admin-menu` feature) with
  zstd and generates the table that `inject::embedded` includes. Windows targets only.
- embeds the icon and version info into the Windows executable.

## Platform-specific code

The analyzer targets Windows only. Direct Win32 calls are kept in a few places, behind
`#[cfg(windows)]`:

- `inject::win`: process enumeration and `LoadLibraryW` injection, used by
  `inject::running_game` and `inject::inject`.
- `logging`: `GetLocalTime` for log timestamps.
- `build.rs`: icon and version resource of the executable (`winresource`).

## Conventions

- `cargo fmt` formatting, `cargo clippy --all-targets -- -D warnings` clean.
- Unit tests live next to the code (`#[cfg(test)] mod tests`).
- No panics on user data: malformed lines are counted and skipped, unreadable files produce a
  message in the UI.
- User-facing text is plain English and tells the user what to do next.
