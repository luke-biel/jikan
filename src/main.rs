#![feature(path_try_exists)]

use anyhow::Context;
use chrono::{Date, Datelike, Duration, NaiveDate, Utc, Weekday};
use colored::Colorize;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Editor, Input};
use itertools::Itertools;
use pico_args::Arguments;
use std::collections::HashMap;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::current;

// TODO: abstractapi.com/holidays-api
fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let mut args = Arguments::from_env();

    let data_dir = option_env!("JIKAN_HOME").map(PathBuf::from).unwrap_or(
        dirs::data_local_dir()
            .context("Cant fetch local_dir location")?
            .join("jikan"),
    );

    match args.subcommand()?.as_deref() {
        Some("display") | Some("d") => {
            handle_display(args, data_dir)?;
        }
        Some("set") | Some("s") => handle_set(args, data_dir)?,
        _ => print_help()?,
    }

    Ok(())
}

fn handle_display(mut args: Arguments, data_dir: PathBuf) -> anyhow::Result<()> {
    let now = args
        .opt_value_from_str(["-d", "--date"])
        .or_else(|_| {
            args.opt_value_from_str(["-m", "--month"]).map(|opt| {
                opt.map(|month: u32| Utc::now().date().naive_local().with_month(month))
                    .flatten()
            })
        })?
        .unwrap_or_else(|| Utc::now().date().naive_local());

    let project = if let Some(project) = args.opt_value_from_str(["-p", "--project"])? {
        project
    } else {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Project name:")
            .interact_text()?
    };

    print_display(project, data_dir, now)
}

fn handle_set(mut args: Arguments, data_dir: PathBuf) -> anyhow::Result<()> {
    #[derive(Default)]
    struct AddSettings {
        project: Option<String>,
        time: Option<usize>,
        day: Option<NaiveDate>,
    }

    let project: Option<String> = args.opt_value_from_str(["-p", "--project"])?;
    let day: Option<NaiveDate> = args.opt_value_from_str(["-d", "--day"])?;
    let time: Option<usize> = args.opt_value_from_str(["-t", "--hours"])?;

    let project = if let Some(project) = project {
        project
    } else {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Project name:")
            .interact_text()?
    };

    let day = if let Some(day) = day {
        day
    } else {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Date:")
            .with_initial_text(Utc::now().date().naive_local().to_string())
            .interact_text()?
    };

    let time = if let Some(time) = time {
        time
    } else {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Hours spent working:")
            .interact_text()?
    };

    let timesheet_file_path =
        data_dir.join(format!("{}-time-{}.csv", project, day.format("%m-%Y")));
    let mut state: HashMap<NaiveDate, usize> = if fs::try_exists(&timesheet_file_path)? {
        let mut current = HashMap::default();
        let mut f = File::open(&timesheet_file_path)?;

        let mut data = String::new();
        let _ = f.read_to_string(&mut data)?;

        let mut data = data
            .lines()
            .nth(1)
            .context("Invalid timesheet format")?
            .split(',');

        let mut iter = day.with_day(1).context("Can't find 1st day of the month")?;

        while iter.month() == day.month() {
            current.insert(
                iter,
                data.next()
                    .context("Missing data in month report")?
                    .parse()?,
            );

            iter += Duration::days(1);
        }

        current
    } else {
        fs::create_dir_all(&data_dir)?;

        let mut current = HashMap::default();
        let mut iter = day.with_day(1).context("Can't find 1st day of the month")?;

        while iter.month() == day.month() {
            current.insert(iter, 0);

            iter += Duration::days(1);
        }

        current
    };

    *state.get_mut(&day).unwrap() = time;

    let mut f = File::create(&timesheet_file_path)?;

    let mut state: Vec<_> = state.into_iter().collect();
    state.sort_by(|(l_key, _), (r_key, _)| l_key.cmp(r_key));

    writeln!(
        f,
        "{}",
        state.iter().map(|(date, _)| date.to_string()).join(",")
    )?;
    writeln!(
        f,
        "{}",
        state.iter().map(|(_, hours)| hours.to_string()).join(",")
    )?;

    print_display(project, &data_dir, day)?;

    Ok(())
}

fn print_display(
    project: String,
    data_dir: impl AsRef<Path>,
    now: NaiveDate,
) -> anyhow::Result<()> {
    draw_days(now);
    draw_month(project, data_dir, now)?;

    Ok(())
}

fn draw_days(now: NaiveDate) {
    let mut iter = now.clone().with_day(1).unwrap();
    while iter.month() == now.month() {
        print!("{: <3}", iter.day().to_string().black().on_blue());
        iter += Duration::days(1);
        if iter.weekday() == Weekday::Mon {
            print!("{}", "|".black().on_cyan())
        }
    }
    println!();
}

fn draw_month(project: String, data_dir: impl AsRef<Path>, now: NaiveDate) -> anyhow::Result<()> {
    let timesheet_file_path =
        data_dir
            .as_ref()
            .join(format!("{}-time-{}.csv", project, now.format("%m-%Y")));
    let mut f = File::open(timesheet_file_path)?;
    let mut data = String::new();
    f.read_to_string(&mut data)?;
    let mut data = data
        .lines()
        .nth(1)
        .context("Invalid timesheet file")?
        .split(',');

    let mut iter = now.clone().with_day(1).unwrap();
    while iter.month() == now.month() {
        let mut amt = data.next().context("Missing day data")?;

        if matches!(iter.weekday(), Weekday::Sat | Weekday::Sun) {
            print!(
                "{:<3}",
                if amt == "0" { "   " } else { amt }.black().on_red()
            )
        } else {
            print!("{:<3}", amt.black().on_green())
        }
        iter += Duration::days(1);
        if iter.weekday() == Weekday::Mon {
            print!("{}", "|".black().on_cyan())
        }
    }
    println!();

    Ok(())
}

fn print_help() -> anyhow::Result<()> {
    Ok(())
}
