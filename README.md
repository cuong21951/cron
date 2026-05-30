# cron

A lightweight, cross-platform **cron-like job scheduler** written in Rust.

Schedule shell commands with the classic 5-field cron syntax, then run a tiny
foreground scheduler that fires them as they come due. No background service,
no database — just a single binary and a small JSON crontab.

```text
┌───────────── minute       (0-59)
│ ┌───────────── hour       (0-23)
│ │ ┌───────────── day of month (1-31)
│ │ │ ┌───────────── month    (1-12)
│ │ │ │ ┌───────────── day of week (0-6, Sunday = 0; 7 also = Sunday)
│ │ │ │ │
* * * * *
```

## Install

### winget (Windows)

```powershell
winget install cuong21951.cron
```

### From source

Requires a [Rust toolchain](https://rustup.rs/).

```bash
cargo install --git https://github.com/cuong21951/cron
```

Or download a prebuilt binary from the
[Releases](https://github.com/cuong21951/cron/releases) page.

## Usage

```bash
# Add a job (cron expression + command)
cron add "*/5 * * * *" "echo hello"
cron add "@daily" "backup.bat"
cron add "0 9-17 * * 1-5" "ping -n 1 example.com"

# List jobs
cron list

# Remove a job by its index
cron remove 1

# Run the scheduler in the foreground (Ctrl+C to stop)
cron run

# Show where the crontab lives
cron path
```

### Supported syntax

Each of the five fields accepts:

| Form        | Example   | Meaning                          |
|-------------|-----------|----------------------------------|
| `*`         | `*`       | every value                      |
| number      | `5`       | exactly that value               |
| range       | `9-17`    | inclusive range                  |
| step        | `*/15`    | every 15th value                 |
| range+step  | `0-30/10` | every 10th value within a range  |
| list        | `1,3,5`   | any of the listed terms          |

Shorthand macros: `@yearly` (`@annually`), `@monthly`, `@weekly`, `@daily`
(`@midnight`), `@hourly`.

As in classic cron, when **both** the day-of-month and day-of-week fields are
restricted, a job fires when **either** matches.

## How it works

- `cron run` wakes at the top of every minute and runs any job whose schedule
  matches the current time.
- Jobs are executed through the system shell (`cmd /C` on Windows, `sh -c`
  elsewhere), detached from the scheduler.
- The crontab is reloaded on every tick, so `add` / `remove` from another
  shell take effect without restarting the scheduler.

## Crontab location

| Platform | Path                                            |
|----------|-------------------------------------------------|
| Windows  | `%APPDATA%\cron\crontab.json`                   |
| Other    | `$XDG_CONFIG_HOME/cron/` or `$HOME/.config/cron/` |

Override with the `CRON_HOME` environment variable.

## Running automatically at startup (Windows)

`cron run` is a foreground process. To keep it running across logins, register
it with Task Scheduler once:

```powershell
schtasks /create /tn "cron" /tr "$env:LOCALAPPDATA\Microsoft\WinGet\Links\cron.exe run" /sc onlogon /rl highest
```

## License

[MIT](LICENSE)
