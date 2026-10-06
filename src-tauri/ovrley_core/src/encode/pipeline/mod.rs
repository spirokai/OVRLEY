//! Encoding runtime pipelines and their shared frame/process infrastructure.

pub mod composite;
pub mod composite_plan;
#[doc(hidden)]
pub mod composite_support;
pub(crate) mod frame_pool;
pub(crate) mod frames;
pub(crate) mod lifecycle;
pub(crate) mod queue;
pub(crate) mod run;
pub mod transparent;
