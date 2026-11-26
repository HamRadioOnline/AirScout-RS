//! Shared test-side reference model for the GUI 1:1 rebuild.
//!
//! Several integration tests use `mod gui_ref;` and each compiles this module
//! independently, so items that only one test uses would otherwise trip
//! `dead_code` under `-D warnings`. The allow below covers the whole module tree.
#![allow(dead_code)]

pub mod designer;
pub mod model;
pub mod resx;
