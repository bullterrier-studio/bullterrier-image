//! The command line through `main(argv)`: exit code, stdout and stderr, in-process. The boundary: a test calls
//! `bullterrier_image_cli::main` and reads files the command wrote; it reaches nothing else of the crate.

mod common;

mod init;
mod render;
mod run;
mod usage;
