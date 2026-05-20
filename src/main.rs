mod app;
mod date;
mod domain;
mod render;
mod restaurants;

use std::env;
use std::process::ExitCode;

use app::load_todays_lunches;
use render::{render_day, render_slack_payload};

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<String, String> {
    let command = parse_args(env::args().skip(1))?;
    let (weekday, lunches) = load_todays_lunches();

    Ok(match command {
        Command::Today => render_day(weekday, &lunches),
        Command::Slack => render_slack_payload(weekday, &lunches),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Today,
    Slack,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();

    let command = match args.next().as_deref() {
        None | Some("today") => Command::Today,
        Some("slack") => Command::Slack,
        Some(command) => {
            return Err(format!(
                "unknown command '{command}'. Usage: lunch [today|slack]"
            ));
        }
    };

    if let Some(arg) = args.next() {
        return Err(format!(
            "unknown argument '{arg}'. Usage: lunch [today|slack]"
        ));
    }

    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_default_today_command() {
        assert_eq!(parse_args(["today".to_string()]), Ok(Command::Today));
    }

    #[test]
    fn accepts_slack_command() {
        assert_eq!(parse_args(["slack".to_string()]), Ok(Command::Slack));
    }

    #[test]
    fn rejects_extra_arguments() {
        assert!(parse_args(["today".to_string(), "--date".to_string()]).is_err());
    }
}
