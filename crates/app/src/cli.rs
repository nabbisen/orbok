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

#[cfg(test)]
mod tests {
    use super::{CliCommand, parse_args};

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
}
