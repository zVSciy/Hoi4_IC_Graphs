# Security policy

## Supported versions

Only the [latest release](https://github.com/zVSciy/Hoi4_IC_Graphs/releases/latest) receives
fixes. Please check that a problem still occurs there before reporting it.

## Reporting a vulnerability

**Do not open a public issue for security problems.**

Report them privately through GitHub:
**[Security → Report a vulnerability](https://github.com/zVSciy/Hoi4_IC_Graphs/security/advisories/new)**.

Please include what is affected, how to reproduce it and what an attacker could achieve. The
maintainer will reply as soon as possible. Once a fix is released, the advisory is published, with
credit to you if you want it.

## What is in scope

The analyzer does a few things where security matters:

- **DLL injection (Windows).** The app loads a DataCollection DLL into `hoi4.exe`. It only does
  this when you ask (or when automatic collection is on) and only into `hoi4.exe`; automatic
  collection only uses a DLL made for the running game version. DLLs come from the app itself, a
  `DataCollection` folder next to it, or ones you added in Settings. Problems with how DLLs are
  chosen, extracted or loaded are in scope.
- **Parsing files.** `output.txt` logs, `.hoi4game` exports and the SQLite game library are
  parsed by the app; `.hoi4game` files in particular may come from other people. Crashes, hangs
  or memory problems caused by crafted files are in scope.
- **Update check.** Release builds ask the GitHub API, over HTTPS, whether a newer release exists.
  The app only shows a notice and never downloads or runs anything by itself.
- **Release packages.** Each release lists SHA-256 checksums in `SHA256SUMS.txt`. Report
  mismatches or anything suspicious about published files.

Not in scope: the behaviour of Hearts of Iron IV itself, antivirus heuristics flagging the
injector (see the README), and problems that need an attacker who already controls your user
account.

## Prebuilt DLLs

The DataCollection DLLs in [`DataCollection/`](DataCollection/) are prebuilt binaries whose
source is not part of this repository. If you prefer not to run them, build the analyzer with
`--no-default-features` and use it only to analyse files.
