//! The command line of bullterrier-image: `bt-image COMMAND ...`.
//!
//! A thin layer over the core: parse, call, print. [`main`] takes the arguments and the two streams, so a test (or the
//! Python console script) runs it in-process and sees the exit code and the output.

mod finders;
mod init;
mod run;
mod stamp;

use std::io::Write;

use bullterrier_image::Error;
use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

const EXIT_CODES: &str = "\
exit codes:
  0    success
  1    the command failed (reason after \"error:\" on stderr); for `run`: a failed result, or with --strict a warning
  2    wrong usage: an unknown command or option (the message explains)
  130  interrupted with Ctrl-C (the console script)

`bt-image COMMAND --help` shows a command's options and an example. Errors go to stderr, results to stdout.";

/// The variable that turns on `--debug`.
const DEBUG_VARIABLE: &str = "BT_IMAGE_DEBUG";

#[derive(Parser)]
#[command(
    name = "bt-image",
    about = "Style photos with composable effects, optionally on a cut-out subject.",
    after_help = EXIT_CODES
)]
struct Cli {
    /// Show the full error (its debug form) instead of one line (also: `BT_IMAGE_DEBUG=1`).
    #[arg(long, global = true)]
    debug: bool,
    /// Hide the progress lines (errors still show).
    #[arg(short, long, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a starter bt-image.toml, a photos/ folder and two recipes.
    #[command(after_help = "example: bt-image init my-project")]
    Init(init::Args),
    /// Process every photo of a project with every recipe: cached, checked, with a report.
    #[command(after_help = "example: bt-image run examples/bt-image.toml --dry-run")]
    Run(run::Args),
}

/// The streams a command writes to: results to `out`, progress and errors to `err`.
pub(crate) struct Console<'a> {
    pub(crate) out: &'a mut dyn Write,
    pub(crate) err: &'a mut dyn Write,
    pub(crate) quiet: bool,
}

impl Console<'_> {
    /// A result line, to stdout.
    pub(crate) fn result(&mut self, line: &str) -> Result<(), Error> {
        written(writeln!(self.out, "{line}"))
    }

    /// A progress line, to stderr, unless the person asked for quiet.
    pub(crate) fn progress(&mut self, line: &str) -> Result<(), Error> {
        if self.quiet { Ok(()) } else { self.problem(line) }
    }

    /// A line that shows even with `-q`, to stderr.
    pub(crate) fn problem(&mut self, line: &str) -> Result<(), Error> {
        written(writeln!(self.err, "{line}"))
    }
}

fn written(result: std::io::Result<()>) -> Result<(), Error> {
    result.map_err(|error| Error::ImageWrite(format!("cannot write to the terminal: {error}")))
}

/// Runs `bt-image` with `argv` (the arguments after the program's name) and returns the exit code: 0 success, 1 the command
/// failed (`error: <what is wrong>` on `err`), 2 wrong usage. `version` is the package's, which `--version` prints.
///
/// With `--debug` (or `BT_IMAGE_DEBUG=1`) an error prints in its debug form. Ctrl-C is the console script's: it restores
/// the default signal handler, so the process ends with 130.
pub fn main(argv: &[String], version: &str, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let arguments = std::iter::once("bt-image").chain(argv.iter().map(String::as_str));
    let matches = match Cli::command()
        .version(version.to_owned())
        .try_get_matches_from(arguments)
    {
        Ok(matches) => matches,
        Err(usage) => {
            let (text, code) = match usage.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => (usage.render().to_string(), 0),
                _ => (usage.render().to_string(), 2),
            };
            // a closed stream cannot be told about: the code is the answer
            let _ = if code == 0 {
                write!(out, "{text}")
            } else {
                write!(err, "{text}")
            };
            return code;
        }
    };
    let Ok(cli) = Cli::from_arg_matches(&matches) else {
        return 2;
    };
    let debug = cli.debug || std::env::var(DEBUG_VARIABLE).is_ok_and(|value| value == "1");
    let mut console = Console {
        out,
        err,
        quiet: cli.quiet,
    };
    let outcome = match cli.command {
        Command::Init(given) => init::command(&given, &mut console),
        Command::Run(given) => run::command(&given, &mut console),
    };
    match outcome {
        Ok(code) => code,
        Err(error) => {
            let line = if debug {
                format!("error: {error:?}")
            } else {
                format!("error: {error}")
            };
            let _ = writeln!(console.err, "{line}"); // a closed stream cannot be told about: the code is the answer
            1
        }
    }
}
