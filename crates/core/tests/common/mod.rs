//! What the integration tests share. Each area has its own module; each test file uses some of them.
#![allow(dead_code, reason = "each test file uses a different part")]

pub(crate) mod files;
pub(crate) mod ops;
pub(crate) mod recipe;
pub(crate) mod run;

pub(crate) type Outcome = Result<(), Box<dyn std::error::Error>>;
