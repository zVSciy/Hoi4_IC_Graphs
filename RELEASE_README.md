<div align="center">

# HOI4 Analyzer GUI

**A powerful desktop application for analyzing Hearts of Iron IV gameplay data**

[![Latest Release](https://img.shields.io/github/v/release/zVSciy/Hoi4GraphToolReleases?style=for-the-badge)](https://github.com/zVSciy/Hoi4GraphToolReleases/releases)
[![Downloads](https://img.shields.io/github/downloads/zVSciy/Hoi4GraphToolReleases/total?style=for-the-badge&color=blue)](https://github.com/zVSciy/Hoi4GraphToolReleases/releases)
[![License](https://img.shields.io/badge/License-Private-red?style=for-the-badge)](#)

[Download](https://github.com/zVSciy/Hoi4GraphToolReleases/releases) • [Report Bug](https://github.com/zVSciy/Hoi4GraphToolReleases/issues)

</div>

---

## About

The HOI4 Analyzer GUI is a sophisticated desktop application designed to capture, analyze, and visualize gameplay data from Hearts of Iron IV. Transform your gaming sessions into detailed insights with interactive graphs and comprehensive data analysis.

> **Note:** This repository contains only the public releases. Source code is maintained in a private repository.

## What's New in v3.1.0

- **Fully automatic mode:** start the analyzer, start HOI4, play. It detects the game and its version, picks the matching DataCollection DLL, starts collection once the game has loaded, opens the Live Dashboard and keeps the game saved.
- **Add DLLs yourself:** when a new HOI4 version comes out, add its DataCollection DLL in Settings; no new app download needed.
- **Settings window:** every automation, graph and library option in one place, remembered between sessions (including your selected countries and version).
- **Rankings:** every country on any date, by military, naval, civilian or combined output, with world share.
- **Smarter graphs:** 7 to 90-day smoothing, a Share % view for faction balance, country names from your game, and a fixed colour per country.
- **Your own groups:** define custom country groups (e.g. multiplayer teams) instead of Axis / Allies.
- **Library upgrades:** per-game summary (top producer, total output), share games as `.hoi4game` files (export / import), and compare games by day since start.
- **Update notice:** the app tells you when a new release is available.

## What's New in v3.1.0

- **Civilian IC graph:** DataCollection 1.19.3+ also records civilian factory output (`CIV`). It gets its own tab next to Military IC and Naval IC.
- **Game Library:** every loaded game is saved automatically, and a running game is kept saved while the Live Dashboard watches it. Browse, rename, annotate and filter past games by HOI4 version, and compare any number of them in one graph.
- **Export:** interactive HTML (all tabs in one page), CSV for spreadsheets, and PNG.
- **Easier file picking:** the Browse dialog opens straight in your HOI4 folder.
- **Rewritten in Rust:** a single small executable that parses even long campaigns in milliseconds.

## Features

### **Real-time Data Collection**
- Fully automatic: detects HOI4, its version and the matching DLL, then starts collecting
- Live Dashboard follows your game while you play
- Support for multiple HOI4 versions; add DLLs for new versions yourself

### **Interactive Visualizations**
- Military, naval and civilian IC, each daily and cumulative
- Per country or combined by faction (Axis, Allies, Other) or your own groups
- Smoothing, Share % (faction balance) and a Rankings table per date
- Click legend entries to show or hide countries, zoom and pan freely
- Export as interactive HTML, CSV or PNG

### **Game Library**
- Every loaded game is saved automatically, without duplicates; live games every 30 seconds
- A continued campaign updates its existing entry
- Name, HOI4 version and notes per game, with search and version filter
- Compare any number of past games side by side, also by day since start
- Share games with friends as `.hoi4game` files

### **Easy to Use**
- Simple, intuitive interface
- One-click data collection start/stop
- Automatic file detection and processing

### **Secure & Reliable**
- SHA256 hash verification for all releases
- No internet connection required during gameplay
- Local data processing only

## Screenshots

### Main Interface
![Main Interface](/assets/Overview.png)
*The clean, intuitive main interface of HOI4 Analyzer GUI*

### Data Collection in Progress
![Data Collection](/assets/Live%20Dashboard.png)
*Real-time monitoring of your HOI4 gameplay session*

### Interactive Graphs
![Graph Visualization](/assets/Military%20IC.png)
*Rich, interactive graphs with customizable filters*

### Filter Options
![Filter Panel](/assets/File%20Comparison.png)
*Comprehensive filtering options for detailed analysis*

## Quick Start

### Download

1. Navigate to the **[Releases](https://github.com/zVSciy/Hoi4GraphToolReleases/releases)** section
2. Download the latest `HOI4_Analyzer.exe` from the Assets
3. Verify the download using the provided SHA256 hash

### Usage

#### **Step 1: Start**
```
1. Launch HOI4_Analyzer.exe
2. Launch Hearts of Iron IV
```

#### **Step 2: Play & Collect**
```
1. The analyzer detects the game and starts data collection by itself
2. The Live Dashboard opens and follows your game
3. Your game is saved to the Game Library while you play
(New HOI4 version without a DLL? Settings -> Add DLL...)
```

#### **Step 3: Analyze**
```
1. Click Browse (opens in your HOI4 directory) and pick output.txt
2. Tick the countries you are interested in
3. Open a graph: Military, Naval and Civilian IC tabs
```

#### **Step 4: Compare Games**
```
1. Loaded and live-monitored games are saved to the Game Library automatically
2. Open Game Library, filter by version, tick two or more games
3. Tick countries in the main window and press Countries or Factions
```

## Security & Verification

Each release includes a **SHA256 hash** for integrity verification:

```bash
# Windows PowerShell
Get-FileHash "HOI4_Analyzer.exe" -Algorithm SHA256

# Command Prompt
certutil -hashfile "HOI4_Analyzer.exe" SHA256
```

Compare the output with the hash provided in the release notes.

## File Locations

- **Output Data**: `{HOI4_Installation_Directory}/output.txt`
- **Game Library**: `%LOCALAPPDATA%\HOI4_Analyzer\library.sqlite3` (one file, copy it to back up; set `HOI4_ANALYZER_LIBRARY` to move it)
- **Settings & added DLLs**: `%LOCALAPPDATA%\HOI4_Analyzer\settings.json` and `%LOCALAPPDATA%\HOI4_Analyzer\DataCollection\`
- **Application**: Wherever you downloaded the executable
- **No Installation Required**: Portable application

## Issues & Support

Found a bug or have a feature request?
- [Create an issue](https://github.com/zVSciy/Hoi4GraphToolReleases/issues)
- Include your system info and steps to reproduce

## License

This software is proprietary. Source code is not publicly available.

---

<div align="center">

**Made with care for the HOI4 Community**

**Star this repo if you find it useful!**

</div>

