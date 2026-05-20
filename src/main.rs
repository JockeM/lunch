mod app;
mod date;
mod domain;
mod render;
mod restaurants;

use std::env;
use std::process::ExitCode;

use app::load_todays_lunches;
use render::render_day;

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
    parse_args(env::args().skip(1))?;
    let (weekday, lunches) = load_todays_lunches();

    Ok(render_day(weekday, &lunches))
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut args = args.into_iter();

    match args.next().as_deref() {
        None | Some("today") => {}
        Some(command) => return Err(format!("unknown command '{command}'. Usage: lunch today")),
    }

    if let Some(arg) = args.next() {
        return Err(format!("unknown argument '{arg}'. Usage: lunch today"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_default_today_command() {
        assert!(parse_args(["today".to_string()]).is_ok());
    }

    #[test]
    fn rejects_extra_arguments() {
        assert!(parse_args(["today".to_string(), "--date".to_string()]).is_err());
    }
}
