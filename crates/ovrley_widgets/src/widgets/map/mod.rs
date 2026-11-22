//! A real map behind the route: camera, GPU frame, marker and attribution, shared by both hosts.
//! The renderer draws the map and the route together, so the widget only owns what they show.
pub mod config;
pub mod content;
pub mod interaction;
pub mod layout;
pub mod preview;
pub mod track;
pub mod validation;
