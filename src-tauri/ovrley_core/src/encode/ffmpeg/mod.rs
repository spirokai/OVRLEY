//! FFmpeg boundary concerns: discovery, capability detection, profiles, and args.

pub mod assemble;
pub mod binary;
pub mod catalog;
pub mod composite;
pub(crate) mod composite_filters;
pub mod composite_profiles;
pub mod detect;
pub(crate) mod probes;
pub mod settings;
pub mod transitions;
pub mod transparent_profiles;
