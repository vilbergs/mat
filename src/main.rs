mod render;
mod style;
mod wrap;

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

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

    let files = if cli.files.is_empty() { vec![PathBuf::from("-")] } else { cli.files };
    let mut status = ExitCode::SUCCESS;
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
        let separator = if i > 0 { "\n" } else { "" };
        let written = write!(stdout, "{separator}{}", render(&markdown, &opts)).and_then(|_| stdout.flush());
        if written.is_err() {
            // Downstream closed the pipe (e.g. `mat README.md | head`).
            return status;
        }
    }
    status
}
