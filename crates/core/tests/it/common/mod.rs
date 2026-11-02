//! What the integration tests share. Each area has its own module; each test module uses some of them. (`memory.rs`,
//! a binary of its own, includes this module by path.)
#![allow(dead_code, reason = "each test module uses a different part")]

pub(crate) mod files;
pub(crate) mod icc;
pub(crate) mod ops;
pub(crate) mod recipe;
pub(crate) mod run;

pub(crate) type Outcome = Result<(), Box<dyn std::error::Error>>;
