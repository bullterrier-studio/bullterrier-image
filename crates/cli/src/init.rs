//! `bt-image init`.

use std::path::PathBuf;

use bullterrier_image::Error;
use bullterrier_image::pipeline::init_project;
use clap::Args as ClapArgs;

use crate::Console;

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// Where to create the project (default: here).
    #[arg(default_value = ".")]
    directory: PathBuf,
}

pub(crate) fn command(args: &Args, console: &mut Console<'_>) -> Result<i32, Error> {
    let file = init_project(&args.directory)?;
    console.result(&file.display().to_string())?;
    Ok(0)
}
