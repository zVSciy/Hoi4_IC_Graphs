//! HOI4 IC Analyzer: collects industrial capacity data from a running Hearts of
//! Iron IV process (via an injected DLL) and graphs it.

pub mod analysis;
pub mod countries;
pub mod data;
pub mod export;
pub mod game;
pub mod inject;
pub mod library;
pub mod live;
pub mod logging;
pub mod paths;
pub mod settings;
pub mod theme;
pub mod ui;
pub mod update;
