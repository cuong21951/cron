//! `cron` — a lightweight cron-like job scheduler.
//!
//! Add jobs with a classic cron expression, list and remove them, then run
//! the scheduler in the foreground to fire jobs as they come due.

mod schedule;
mod store;

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use chrono::{Local, Timelike};
use clap::{Parser, Subcommand};

use schedule::Schedule;
use store::{Crontab, Job};

#[derive(Parser)]
#[command(
    name = "cron",
    version,
    about = "A lightweight cron-like job scheduler.",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a job: a cron expression plus the command to run.
    Add {
        /// Cron expression, e.g. "*/5 * * * *" or "@daily".
        schedule: String,
        /// Command to run when the job fires.
        command: String,
    },
    /// List all scheduled jobs.
    List,
    /// Remove a job by its index (see `cron list`).
    Remove {
        /// 1-based index of the job to remove.
        index: usize,
    },
    /// Run the scheduler in the foreground, firing jobs as they come due.
    Run,
    /// Print the path to the crontab file.
    Path,
    /// Start `cron run` automatically at each logon (Windows).
    Autostart {
        /// Remove the autostart entry instead of creating it.
        #[arg(long)]
        remove: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Add { schedule, command } => cmd_add(&schedule, &command),
        Commands::List => cmd_list(),
        Commands::Remove { index } => cmd_remove(index),
        Commands::Run => cmd_run(),
        Commands::Path => cmd_path(),
        Commands::Autostart { remove } => cmd_autostart(remove),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_add(schedule: &str, command: &str) -> Result<(), String> {
    // Validate the expression up front so bad jobs never reach the crontab.
    Schedule::parse(schedule)?;

    let mut crontab = Crontab::load()?;
    crontab.jobs.push(Job {
        schedule: schedule.to_string(),
        command: command.to_string(),
    });
    crontab.save()?;
    println!("added job #{}: {schedule}  {command}", crontab.jobs.len());
    Ok(())
}

fn cmd_list() -> Result<(), String> {
    let crontab = Crontab::load()?;
    if crontab.jobs.is_empty() {
        println!("no jobs scheduled (add one with `cron add`)");
        return Ok(());
    }
    let width = crontab
        .jobs
        .iter()
        .map(|j| j.schedule.len())
        .max()
        .unwrap_or(0);
    for (i, job) in crontab.jobs.iter().enumerate() {
        println!(
            "{:>3}  {:<width$}  {}",
            i + 1,
            job.schedule,
            job.command,
            width = width
        );
    }
    Ok(())
}

fn cmd_remove(index: usize) -> Result<(), String> {
    let mut crontab = Crontab::load()?;
    if index == 0 || index > crontab.jobs.len() {
        return Err(format!(
            "no job #{index} (there are {} jobs)",
            crontab.jobs.len()
        ));
    }
    let removed = crontab.jobs.remove(index - 1);
    crontab.save()?;
    println!(
        "removed job #{index}: {}  {}",
        removed.schedule, removed.command
    );
    Ok(())
}

fn cmd_path() -> Result<(), String> {
    println!("{}", store::crontab_path()?.display());
    Ok(())
}

fn cmd_run() -> Result<(), String> {
    // Refuse to start a second scheduler in the same session so that an
    // autostart launch plus a manual `cron run` don't double-fire every job.
    if already_running() {
        println!("cron scheduler is already running in this session; exiting.");
        return Ok(());
    }

    println!(
        "cron scheduler started at {}; crontab: {}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        store::crontab_path()?.display()
    );
    println!("checking jobs every minute (Ctrl+C to stop)");

    loop {
        // Sleep until the top of the next minute so we evaluate once per
        // minute, aligned to the clock.
        let now = Local::now();
        let secs_to_next = 60 - now.second();
        thread::sleep(Duration::from_secs(u64::from(secs_to_next)));

        let tick = Local::now();
        // Reload each tick so `add`/`remove` from another shell take effect
        // without restarting the scheduler.
        let crontab = match Crontab::load() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[{}] failed to load crontab: {e}", stamp(&tick));
                continue;
            }
        };

        for (i, job) in crontab.jobs.iter().enumerate() {
            let sched = match job.parsed_schedule() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "[{}] job #{} has invalid schedule: {e}",
                        stamp(&tick),
                        i + 1
                    );
                    continue;
                }
            };
            if sched.matches(&tick) {
                run_job(job, &tick);
            }
        }
    }
}

/// Spawn a job's command through the system shell, detached from this process.
fn run_job(job: &Job, tick: &chrono::DateTime<Local>) {
    println!("[{}] running: {}", stamp(tick), job.command);

    let spawned = shell_command(&job.command).stdin(Stdio::null()).spawn();

    if let Err(e) = spawned {
        eprintln!("[{}] failed to start `{}`: {e}", stamp(tick), job.command);
    }
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    use std::os::windows::process::CommandExt;
    // Pass the command line to cmd verbatim. Rust's normal arg quoting mangles
    // embedded quotes (breaking redirects like `>> "C:\with space\out.txt"`),
    // so build the line by hand: `/S /C "<command>"`. With `/S`, cmd strips
    // only the outermost quote pair and runs everything between literally.
    let mut c = Command::new("cmd");
    c.raw_arg(format!("/S /C \"{command}\""));
    c
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(command);
    c
}

fn stamp(when: &chrono::DateTime<Local>) -> String {
    when.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Enable or disable launching `cron run` automatically at logon.
///
/// On Windows this drops a hidden VBScript launcher in the per-user Startup
/// folder, so the scheduler starts in *your* session (with your profile and
/// Downloads) without a console window, no admin rights, and no Task Scheduler.
#[cfg(windows)]
fn cmd_autostart(remove: bool) -> Result<(), String> {
    use std::path::PathBuf;

    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
    let startup = PathBuf::from(&appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
    let launcher = startup.join("cron-autostart.vbs");

    if remove {
        if launcher.exists() {
            std::fs::remove_file(&launcher)
                .map_err(|e| format!("failed to remove {}: {e}", launcher.display()))?;
            println!("autostart disabled (removed {})", launcher.display());
        } else {
            println!("autostart was not enabled");
        }
        return Ok(());
    }

    let exe = std::env::current_exe().map_err(|e| format!("cannot locate cron.exe: {e}"))?;
    let log = PathBuf::from(&appdata).join(r"cron\scheduler.log");
    std::fs::create_dir_all(&startup)
        .map_err(|e| format!("failed to create {}: {e}", startup.display()))?;

    // Launch hidden (window style 0) via cmd so output is appended to a log the
    // user can inspect. In VBScript a literal `"` inside a string is written as
    // `""`, so each quoted path below is wrapped in a doubled pair of quotes.
    let content = format!(
        "' Auto-generated by `cron autostart` - launches the scheduler hidden at logon.\r\n\
         CreateObject(\"WScript.Shell\").Run \"cmd /c \"\"{}\"\" run >> \"\"{}\"\" 2>&1\", 0, False\r\n",
        exe.display(),
        log.display()
    );
    std::fs::write(&launcher, content)
        .map_err(|e| format!("failed to write {}: {e}", launcher.display()))?;

    println!("autostart enabled: {}", launcher.display());
    println!(
        "`cron run` will start hidden at each logon; logs go to {}",
        log.display()
    );
    println!("disable with: cron autostart --remove");
    Ok(())
}

#[cfg(not(windows))]
fn cmd_autostart(_remove: bool) -> Result<(), String> {
    Err(
        "`cron autostart` is currently Windows-only. On Linux/macOS, run \
         `cron run` from a systemd user service, launchd agent, or your \
         system's own cron."
            .to_string(),
    )
}

/// Returns true if another `cron run` already holds the session-wide lock.
///
/// Implemented with a named mutex; the handle is intentionally leaked so the
/// mutex lives for the whole process and is released automatically on exit.
#[cfg(windows)]
fn already_running() -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = std::ffi::OsStr::new("cron-scheduler-singleton")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle.is_null() {
            // Couldn't create the mutex; don't block startup over it.
            return false;
        }
        // Leak `handle` on purpose: closing it would release the lock.
        ERROR_ALREADY_EXISTS == windows_sys::Win32::Foundation::GetLastError()
    }
}

#[cfg(not(windows))]
fn already_running() -> bool {
    false
}
