mod render;
mod style;
mod wrap;

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use clap::{Parser, ValueEnum};

use crate::render::{Options, THEMES, render};
use crate::style::Painter;

/// Pretty-print Markdown in the terminal — cat for markdown.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Files to print; reads stdin when none are given or for "-".
    files: Vec<PathBuf>,

    /// Wrap width in columns [default: terminal width, at most 100].
    #[arg(short, long)]
    width: Option<usize>,

    /// When to use colors and styles.
    #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
    color: ColorMode,

    /// Syntax highlighting theme for code blocks ("ansi" follows the terminal palette).
    #[arg(short, long, default_value = "ansi")]
    theme: String,

    /// When to show the output in a pager ($MAT_PAGER, $PAGER or less).
    #[arg(short, long, value_enum, default_value_t = Paging::Auto)]
    paging: Paging,

    /// Alias for --paging never.
    #[arg(short = 'P', conflicts_with = "paging")]
    no_paging: bool,

    /// List the available code themes and exit.
    #[arg(long)]
    list_themes: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum Paging {
    Auto,
    Always,
    Never,
}

/// The pager command line: $MAT_PAGER, then $PAGER, then `less`. A bare `less`
/// gets `-RF` so colors pass through and short output prints without paging.
fn pager_command() -> Option<Command> {
    let line = ["MAT_PAGER", "PAGER"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "less".into());
    let mut words = line.split_whitespace();
    let program = words.next()?;
    let args: Vec<&str> = words.collect();
    let mut command = Command::new(program);
    if args.is_empty() && PathBuf::from(program).file_name().is_some_and(|n| n == "less") {
        command.arg("-RF");
    } else {
        command.args(args);
    }
    Some(command)
}

/// Shows `output` in the pager, or returns it back if no pager could be started.
fn page(output: &str) -> Result<(), &str> {
    let Some(mut child) = pager_command().and_then(|mut c| c.stdin(Stdio::piped()).spawn().ok()) else {
        return Err(output);
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A write error means the user quit the pager early.
        let _ = stdin.write_all(output.as_bytes());
    }
    let _ = child.wait();
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut stdout = io::stdout().lock();

    if cli.list_themes {
        let mut names: Vec<&str> = THEMES.theme_names().collect();
        names.sort_unstable();
        for name in names {
            let _ = writeln!(stdout, "{name}");
        }
        return ExitCode::SUCCESS;
    }
    if THEMES.get(&cli.theme).is_none() {
        eprintln!("mat: unknown theme '{}' (see --list-themes)", cli.theme);
        return ExitCode::FAILURE;
    }

    let tty = io::stdout().is_terminal();
    let color = match cli.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => tty && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    };
    let truecolor = std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit");
    let width = cli.width.unwrap_or_else(|| {
        terminal_size::terminal_size()
            .map(|(w, _)| usize::from(w.0))
            .unwrap_or(80)
            .min(100)
    });
    let opts = Options { width, painter: Painter { color, truecolor }, theme: cli.theme };

    let paging = match if cli.no_paging { Paging::Never } else { cli.paging } {
        Paging::Always => true,
        Paging::Never => false,
        Paging::Auto => tty,
    };

    let files = if cli.files.is_empty() { vec![PathBuf::from("-")] } else { cli.files };
    let mut status = ExitCode::SUCCESS;
    let mut output = String::new();
    for (i, path) in files.iter().enumerate() {
        let source = if path.as_os_str() == "-" {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).map(|_| buf)
        } else {
            std::fs::read_to_string(path)
        };
        let markdown = match source {
            Ok(text) => text,
            Err(err) => {
                eprintln!("mat: {}: {err}", path.display());
                status = ExitCode::FAILURE;
                continue;
            }
        };
        if i > 0 {
            output.push('\n');
        }
        output.push_str(&render(&markdown, &opts));
    }

    let unpaged = if paging { page(&output).err() } else { Some(output.as_str()) };
    if let Some(text) = unpaged {
        // An error means downstream closed the pipe (e.g. `mat README.md | head`).
        let _ = stdout.write_all(text.as_bytes()).and_then(|_| stdout.flush());
    }
    status
}
