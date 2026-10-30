use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Count lines, Unicode characters, and estimated tokens in the current directory.
    Count(Common),
    /// Concatenate file contents to stdout or an output file.
    Read {
        #[command(flatten)]
        common: Common,
        /// Write to this file instead of stdout (overrides the config).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Omit per-file headers; use =false to override a true config value.
        #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
        hide_filenames: Option<bool>,
    },
}

#[derive(Args)]
struct Common {
    /// Exclude a file or directory; repeat to exclude multiple paths.
    /// When present, replaces the config's entire exclude list.
    #[arg(long)]
    exclude: Vec<PathBuf>,
    /// Load options from a JSON file (default: max250k.json, if present).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Diagnostic verbosity (default: info; overrides the config).
    #[arg(long, value_enum)]
    log_level: Option<LogLevel>,
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    #[default]
    Info,
    Debug,
}

impl LogLevel {
    fn debug(self, message: std::fmt::Arguments<'_>) {
        if self == Self::Debug {
            eprintln!("{message}");
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    exclude: Vec<PathBuf>,
    output: Option<PathBuf>,
    log_level: LogLevel,
    hide_filenames: bool,
}

impl Common {
    fn resolve(self) -> Result<Config> {
        let config_path = match self.config {
            Some(path) => Some(path),
            None => {
                let path = PathBuf::from("max250k.json");
                if path
                    .try_exists()
                    .context("Cannot check default config max250k.json")?
                {
                    Some(path)
                } else {
                    None
                }
            }
        };
        let mut config: Config = match config_path {
            Some(path) => serde_json::from_str(
                &fs::read_to_string(&path)
                    .with_context(|| format!("Cannot read config {}", path.display()))?,
            )
            .with_context(|| format!("Invalid config {}", path.display()))?,
            None => Config::default(),
        };
        if !self.exclude.is_empty() {
            config.exclude = self.exclude;
        }
        if let Some(level) = self.log_level {
            config.log_level = level;
        }
        Ok(config)
    }
}

// Resolve existing paths (including aliases); also support not-yet-created paths.
fn absolute(root: &Path, path: &Path) -> PathBuf {
    let path = root.join(path);
    if let Ok(path) = path.canonicalize() {
        return path;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn files(root: &Path, excluded: &[PathBuf], log_level: LogLevel) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let skip = excluded.iter().any(|path| entry.path().starts_with(path));
            if skip {
                log_level.debug(format_args!(
                    "Skipped (excluded): {}",
                    entry.path().display()
                ));
            }
            !skip
        })
    {
        let entry = entry.context("Cannot traverse directory")?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        } else if entry.file_type().is_symlink() {
            log_level.debug(format_args!(
                "Skipped (symlink): {}",
                entry.path().display()
            ));
        }
    }
    files.sort();
    Ok(files)
}

fn text(path: &Path, log_level: LogLevel) -> Result<Option<String>> {
    let bytes = fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => {
            if log_level == LogLevel::Debug {
                let mut counts = Counts::default();
                counts.add(&text);
                log_level.debug(format_args!(
                    "Read: {} | lines: {} | characters: {} | tokens: {}",
                    path.display(),
                    counts.lines,
                    counts.characters,
                    counts.tokens
                ));
            }
            Ok(Some(text))
        }
        _ => {
            log_level.debug(format_args!(
                "Skipped (binary/non-UTF-8): {}",
                path.display()
            ));
            Ok(None)
        }
    }
}

#[derive(Default)]
struct Counts {
    lines: usize,
    characters: usize,
    tokens: usize,
}

impl Counts {
    fn add(&mut self, text: &str) {
        self.lines += text.lines().count();
        self.characters += text.chars().count();
        self.tokens += tokenx_rs::estimate_token_count(text);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let (common, read, output, hide_filenames) = match cli.command {
        Command::Count(common) => (common, false, None, None),
        Command::Read {
            common,
            output,
            hide_filenames,
        } => (common, true, output, hide_filenames),
    };
    let config = common.resolve()?;
    let root = std::env::current_dir()?.canonicalize()?;
    let output = if read { output.or(config.output) } else { None };
    let mut excluded: Vec<_> = config.exclude.iter().map(|p| absolute(&root, p)).collect();
    if let Some(path) = &output {
        excluded.push(absolute(&root, path));
    }
    let paths = files(&root, &excluded, config.log_level)?;
    if read {
        let hide_filenames = hide_filenames.unwrap_or(config.hide_filenames);
        let writer: Box<dyn Write> = match output {
            Some(path) => Box::new(
                File::create(&path)
                    .with_context(|| format!("Cannot create output {}", path.display()))?,
            ),
            None => Box::new(io::stdout().lock()),
        };
        let mut writer = BufWriter::new(writer);
        let mut needs_newline = false;
        for path in paths {
            if let Some(text) = text(&path, config.log_level)? {
                if !hide_filenames {
                    if needs_newline {
                        writeln!(writer)?;
                    }
                    writeln!(
                        writer,
                        "# file content: {}",
                        path.strip_prefix(&root)?.display()
                    )?;
                }
                writer.write_all(text.as_bytes())?;
                needs_newline = !text.is_empty() && !text.ends_with('\n');
            }
        }
        writer.flush()?;
    } else {
        let mut counts = Counts::default();
        for path in paths {
            if let Some(text) = text(&path, config.log_level)? {
                counts.add(&text);
            }
        }
        writeln!(
            io::stdout().lock(),
            "lines: {}\ncharacters: {}\ntokens: {}",
            counts.lines,
            counts.characters,
            counts.tokens
        )?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe) =>
        {
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
