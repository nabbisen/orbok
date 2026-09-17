//! Command-line arguments, parsed before anything touches the filesystem
//! (Task 051).
//!
//! `main` used to look for three known flags and ignore everything else,
//! so `orbok --help`, `orbok -h` or a typo such as `orbok --chek` went on to
//! resolve the default profile, open its catalog and run pending
//! migrations. Since RFC-062 a catalog recorded at a newer schema is
//! refused by an older build, so a mistyped command on a newer build could
//! leave a profile the user's usual build can no longer open. Parsing into
//! a type first means an unrecognised argument cannot reach startup.

/// CLI usage text. Outside the GUI i18n boundary, like `--version`'s
/// output: it is printed to a terminal, never rendered in the window.
pub(crate) const USAGE: &str = "\
Usage: orbok [OPTIONS]

Options:
      --portable   Keep all data under ./orbok-data/ in the current directory
      --check      Validate the backend without opening a window, then exit
  -V, --version    Print the version and exit
  -h, --help       Print this help and exit

Environment:
  ORBOK_DATA_DIR   Use this directory for the whole profile (not with --portable)
";

/// What one invocation asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CliCommand {
    Gui {
        portable: bool,
    },
    Check {
        portable: bool,
    },
    Version,
    Help,
    /// The first argument that is not an option this build knows.
    Unknown(String),
}

/// Parse `std::env::args()`, including the program name at index 0.
///
/// An unrecognised argument wins over everything else: acting on the
/// recognised ones while one was mistyped is how a typo reaches a profile.
/// Among recognised options, help beats version beats check, so
/// `--version` and `--check` behave exactly as they did.
pub(crate) fn parse_args(args: &[String]) -> CliCommand {
    let (mut portable, mut check, mut version, mut help) = (false, false, false, false);
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--portable" => portable = true,
            "--check" => check = true,
            "--version" | "-V" => version = true,
            "--help" | "-h" => help = true,
            other => return CliCommand::Unknown(other.to_string()),
        }
    }
    if help {
        CliCommand::Help
    } else if version {
        CliCommand::Version
    } else if check {
        CliCommand::Check { portable }
    } else {
        CliCommand::Gui { portable }
    }
}

/// The stderr text for an unrecognised argument: the argument named, then
/// the usage.
pub(crate) fn unknown_argument_message(arg: &str) -> String {
    format!("orbok: unrecognized argument '{arg}'\n\n{USAGE}")
}

/// Task 061 §1: whether this invocation prints before any window opens, and
/// so needs the parent console on Windows, where `orbok.exe` is a GUI
/// program with no console of its own. Only a plain GUI launch prints
/// nothing: `--portable` announces its data directory, and every other
/// command's whole output is text.
pub(crate) fn needs_console(command: &CliCommand) -> bool {
    !matches!(command, CliCommand::Gui { portable: false })
}

/// The stderr text refusing `--portable` in the Microsoft Store package.
pub(crate) const PORTABLE_UNAVAILABLE_WHEN_PACKAGED: &str =
    "orbok: portable mode is not available in the Microsoft Store version\n";

/// Task 061 §4: a packaged install directory is read-only, so data "beside
/// the executable" cannot work. `Some` is the message to print before
/// refusing -- decided before any runtime context is resolved, so nothing
/// is created or opened. Commands that never resolve a profile (help,
/// version, an unrecognised argument) are unaffected.
pub(crate) fn portable_refusal(command: &CliCommand, packaged: bool) -> Option<&'static str> {
    match command {
        CliCommand::Gui { portable: true } | CliCommand::Check { portable: true } if packaged => {
            Some(PORTABLE_UNAVAILABLE_WHEN_PACKAGED)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CliCommand, PORTABLE_UNAVAILABLE_WHEN_PACKAGED, needs_console, parse_args, portable_refusal,
    };

    fn parse(args: &[&str]) -> CliCommand {
        let mut all = vec!["orbok".to_string()];
        all.extend(args.iter().map(|arg| arg.to_string()));
        parse_args(&all)
    }

    #[test]
    fn no_arguments_launch_the_gui() {
        assert_eq!(parse(&[]), CliCommand::Gui { portable: false });
    }

    #[test]
    fn the_four_existing_flags_keep_their_meaning() {
        assert_eq!(parse(&["--portable"]), CliCommand::Gui { portable: true });
        assert_eq!(parse(&["--check"]), CliCommand::Check { portable: false });
        assert_eq!(
            parse(&["--portable", "--check"]),
            CliCommand::Check { portable: true }
        );
        assert_eq!(parse(&["--version"]), CliCommand::Version);
        assert_eq!(parse(&["-V"]), CliCommand::Version);
        // `--version` took precedence over `--check` before this change.
        assert_eq!(parse(&["--check", "--version"]), CliCommand::Version);
    }

    #[test]
    fn help_is_recognised_in_both_spellings() {
        assert_eq!(parse(&["--help"]), CliCommand::Help);
        assert_eq!(parse(&["-h"]), CliCommand::Help);
    }

    #[test]
    fn an_unrecognised_argument_is_named_and_wins() {
        assert_eq!(
            parse(&["--chek"]),
            CliCommand::Unknown("--chek".to_string())
        );
        assert_eq!(
            parse(&["--portable", "--bogus"]),
            CliCommand::Unknown("--bogus".to_string())
        );
        assert_eq!(
            parse(&["--version", "extra"]),
            CliCommand::Unknown("extra".to_string())
        );
    }

    /// Task 061 §1: every command that prints needs a console; a plain GUI
    /// launch does not.
    #[test]
    fn only_a_plain_gui_launch_runs_without_a_console() {
        for (args, expected) in [
            (&[][..], false),
            (&["--portable"][..], true),
            (&["--check"][..], true),
            (&["--portable", "--check"][..], true),
            (&["--version"][..], true),
            (&["--help"][..], true),
            (&["--chek"][..], true),
        ] {
            assert_eq!(needs_console(&parse(args)), expected, "{args:?}");
        }
    }

    /// Task 061 §4: packaged, `--portable` is refused whether it opens the
    /// window or runs `--check`; unpackaged, nothing changes; commands that
    /// never resolve a profile are never refused.
    #[test]
    fn portable_is_refused_only_when_packaged() {
        let refused = Some(PORTABLE_UNAVAILABLE_WHEN_PACKAGED);
        for (args, packaged, expected) in [
            (&["--portable"][..], true, refused),
            (&["--portable", "--check"][..], true, refused),
            (&["--portable"][..], false, None),
            (&["--portable", "--check"][..], false, None),
            (&[][..], true, None),
            (&["--check"][..], true, None),
            (&["--portable", "--version"][..], true, None),
            (&["--portable", "--help"][..], true, None),
            (&["--portable", "--chek"][..], true, None),
        ] {
            assert_eq!(
                portable_refusal(&parse(args), packaged),
                expected,
                "{args:?} packaged={packaged}"
            );
        }
    }
}
