# User guide

Everything HOI4 IC Analyzer can do, from the first start to comparing past campaigns.

**Contents**

1. [Automatic mode](#1-automatic-mode)
2. [Starting data collection by hand](#2-starting-data-collection-by-hand)
3. [Loading data files](#3-loading-data-files)
4. [Graphs](#4-graphs)
5. [Rankings](#5-rankings)
6. [Exporting](#6-exporting)
7. [Game library](#7-game-library)
8. [Live dashboard](#8-live-dashboard)
9. [Settings](#9-settings)
10. [Storage locations](#10-storage-locations)

---

## 1. Automatic mode

1. **Launch** `HOI4_Analyzer.exe` and leave it open.
2. **Start Hearts of Iron IV** and play.

That's it. The analyzer:

- notices the game and reads its version,
- picks the matching DataCollection DLL,
- starts data collection once the game has loaded (30 seconds by default),
- opens the **Live Dashboard**,
- saves the game in the **Game Library** while you play, so nothing is lost when you close the game.

Every step can be switched off or adjusted in [Settings](#9-settings). The main window always shows
whether HOI4 is running and whether a DLL exists for its version.

> [!IMPORTANT]
> **New HOI4 version?** If the analyzer says there is no DataCollection DLL for your game version,
> get the DLL for that version and add it in **Settings → DataCollection DLLs → Add DLL…**.
> No new app download is needed. For safety, automatic collection only starts with a DLL made for
> your exact game version.

## 2. Starting data collection by hand

1. Choose your Hearts of Iron IV version under **Version**.
2. Click **Start Data Collection** while the game is running.

## 3. Loading data files

DataCollection writes `output.txt` into your **HOI4 game folder**. To load it:

- click **Browse File 1…** or **Browse File 2…** (the dialog opens in the game folder),
- drag one or two files onto the window, or
- pass them on the command line: `HOI4_Analyzer.exe output.txt [second.txt]`.

## 4. Graphs

### Opening a graph

1. **Choose countries.** Tick the countries to chart, or use **Toggle All** per column. Countries
   are shown by name (*German Reich*, *Soviet Union*, *United Kingdom*, …) and your choice is
   remembered. Countries from mods get their names from your HOI4 installation.
2. **Open a graph.**

| Button | Shows |
| --- | --- |
| **Individual IC** | One line per ticked country. |
| **Factions IC** | Countries added up per faction, or per your own groups (see [Settings](#9-settings)). |
| **Compare …** | Data File 1 and 2 side by side. |

### Tabs

Every graph window has a tab per value the game logs. Each tab shows the daily value and the
running total.

| Tab | Measures |
| --- | --- |
| **Military IC** | Military factory output |
| **Naval IC** | Dockyard output |
| **Civilian IC** | Civilian factory output (DataCollection 1.19.3 and newer; hidden for older files) |

### Toolbar

- **Smoothing** (7 to 90-day average) hides day-to-day noise in the daily values.
- **Share %** shows each line as a percentage of all visible lines, for example the Axis vs Allies
  balance over time. Hide lines in the legend to compare fewer.

### Mouse controls

| Action | Effect |
| --- | --- |
| Click a country in the legend | Show or hide it in every tab |
| Drag | Pan |
| Scroll | Zoom |
| Double-click | Reset the view |

Each country keeps the same colour in every graph.

## 5. Rankings

**Rankings** lists every country on one date, ordered by military, naval, civilian or combined
output, with its share of the world total.

- Choose the date with the slider.
- Show that day's value (optionally averaged over several days) or the total so far.
- Pick the data source: Data File 1 or 2, the live game, or games ticked in the library.

## 6. Exporting

Every graph window has export buttons:

| Button | Result |
| --- | --- |
| **Export HTML…** | One interactive page with all tabs. Opens in any browser and can be shared. |
| **Export CSV…** | The raw numbers (`date, series, metric, daily, cumulative`) for Excel, LibreOffice or pandas. |
| **Save PNG…** | An image of the current tab. |
| **Open in Browser** | The interactive page, right away. |

## 7. Game library

Your games are saved in the **Game Library** automatically:

- every file you load as Data File 1 or 2 is stored right away,
- while the **Live Dashboard** monitors a running game, it is saved every 30 seconds and once more
  when monitoring stops or the app closes.

Open it with **Game Library** in the main window.

- **Nothing is stored twice.** Saving `output.txt` again after playing further updates that game
  instead of creating a new entry. A new campaign becomes a new game.
- **At a glance:** every game shows its dates, countries, the top military producer and the total
  military output.
- **Name, version and notes** can be edited directly in the list. New games get the HOI4 version
  selected in the main window.
- **Search and version filter** narrow the list, for example to games played on 1.19.4.
- **Actions** on a game: load it as Data File 1 or 2, **Export…** it as a `.hoi4game` file to share
  with friends, or delete it. **Import…** adds games others exported.
- Games are stored already parsed and compressed, so opening an old game is instant and the
  original `output.txt` is no longer needed.

### Comparing games

1. Tick two or more games in the library.
2. Tick countries in the main window (the countries of the ticked games are added there).
3. Press **Countries** or **Factions**.

Each line is labelled with its game, for example `German Reich (Game #3)`. Tick
**Align by day since start** to compare games that began at different dates (for example 1936 and
1939).

The library is a single file, `library.sqlite3`, in the app's [data folder](#10-storage-locations).
**Open Folder** in the library window shows it. Back it up by copying that file.

## 8. Live dashboard

The **Live Dashboard** follows `output.txt` while you play. Only newly written data is read on each
check, so the graphs stay current even late in a long campaign.

- Automatic mode opens it for you; otherwise open it and click **Start Live Monitoring**.
- If no countries are ticked, it shows the ten biggest producers.

## 9. Settings

**Settings** in the main window holds every option. Changes apply immediately and are remembered.

| Section | Options |
| --- | --- |
| **HOI4 & Data Collection** | Watch for the game, pick the matching DLL version, start collection automatically (and how long to wait after the game starts), open the live dashboard automatically, where DataCollection writes its file. |
| **DataCollection DLLs** | All available versions and where each comes from (built in, added by you, or a `DataCollection` folder next to the app). **Add DLL…** adds the DLL for a new game version; the version is read from a file name like `DataCollection_1.19.5.dll`. Added DLLs can be removed again. |
| **Game Library & Live Dashboard** | Automatic saving, how often the live file is checked, how often a running game is saved. |
| **Graphs** | Country names or tags, same colour per country, default smoothing. |
| **Country Groups** | Keep Axis and Allies, or define your own groups (for example multiplayer teams: `Team 1: GER ITA JAP`). Groups decide the columns of the main window and what the Factions buttons add up. |
| **Updates** | Check for a new release on start. Only a notice; nothing is downloaded automatically. |

## 10. Storage locations

Settings, the game library and DLLs you added are kept in the app's data folder,
`%LOCALAPPDATA%\HOI4_Analyzer\`:

| File | Contents |
| --- | --- |
| `library.sqlite3` | Game library |
| `settings.json` | Settings |
| `DataCollection/` | DLLs you added |

To keep the library or settings somewhere else, for example in a synced folder, set the environment
variable `HOI4_ANALYZER_LIBRARY` or `HOI4_ANALYZER_SETTINGS` to the full path of the file to use.
