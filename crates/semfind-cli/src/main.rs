use anyhow::{Context, Result, ensure};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use semfind_core::{
    Category, Distro, ExecutionDevice, FileKind, FlorenceCaptioner, HitLocation, Index,
    IndexOptions, IndexReport, Language, LocalModel, SearchOptions,
};
mod distro;
mod ui;
use distro::DistroFilter;
use std::{
    env,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};
use ui::Messages;

#[derive(Parser)]
#[command(version, about, subcommand_precedence_over_arg = true)]
struct Cli {
    #[arg(long, env = "SEMFIND_INDEX_DIR", global = true)]
    index_dir: Option<PathBuf>,
    /// Directory containing a local ONNX model and tokenizer.
    #[arg(
        long,
        env = "SEMFIND_MODEL_DIR",
        global = true,
        conflicts_with = "model"
    )]
    model_dir: Option<PathBuf>,
    /// Directory containing the local Florence-2 checkpoint.
    #[arg(long, env = "SEMFIND_CAPTION_MODEL_DIR", global = true)]
    caption_model_dir: Option<PathBuf>,
    /// Select an embedding model installed by scripts/download_model.py.
    #[arg(long, env = "SEMFIND_MODEL", value_enum, global = true)]
    model: Option<ModelChoice>,
    /// Inference backend. Auto tries CUDA, MIGraphX, then CPU.
    #[arg(
        long,
        env = "SEMFIND_DEVICE",
        value_enum,
        default_value = "auto",
        global = true
    )]
    device: DeviceChoice,
    #[arg(long, env = "SEMFIND_PYTHON", default_value = "python3", global = true)]
    python: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    search: Search,
}

#[derive(Clone, Copy, ValueEnum)]
enum DeviceChoice {
    Auto,
    Cpu,
    Cuda,
    Migraphx,
}

#[derive(Clone, Copy, ValueEnum)]
enum ModelChoice {
    E5Small,
    E5Large,
    #[value(name = "granite-97m")]
    Granite97M,
    #[value(name = "granite-311m")]
    Granite311M,
}

impl ModelChoice {
    fn directory(self) -> &'static str {
        match self {
            Self::E5Small => "",
            Self::E5Large => "e5-large",
            Self::Granite97M => "granite-97m-r2",
            Self::Granite311M => "granite-311m-r2",
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Index the home directory, desktop applications, local commands and built-in catalogs.
    Init {
        /// Exclude a directory or file by name or by a path relative to HOME.
        #[arg(long)]
        exclude: Vec<PathBuf>,
        /// Remove the saved exclusions for HOME.
        #[arg(long, conflicts_with = "exclude")]
        clear_excludes: bool,
        /// Skip Florence-2 image captioning for this scan.
        #[arg(long)]
        no_caption_images: bool,
    },
    /// Synchronize a directory and the built-in catalogs. Omit the root for catalogs only.
    Index {
        root: Option<PathBuf>,
        /// Include local manual pages and executable names from PATH.
        #[arg(long)]
        manpages: bool,
        /// Describe supported images in the selected directory with Florence-2.
        #[arg(long, requires = "root")]
        caption_images: bool,
        /// Exclude a directory or file by name or by a path relative to the root.
        #[arg(long, requires = "root")]
        exclude: Vec<PathBuf>,
        /// Remove the saved exclusions for this root.
        #[arg(long, requires = "root", conflicts_with = "exclude")]
        clear_excludes: bool,
    },
    /// Show indexed roots and counts without loading the model.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Remove an indexed directory and its stored contents.
    Unindex { root: PathBuf },
    /// Search explicitly (use this when the query matches a command name).
    Search(Search),
}

#[derive(clap::Args)]
struct Search {
    /// A query, file name or identifier. Omit with --stdin or when listing files with --kind.
    query: Option<String>,
    /// Restrict results to a group of file formats.
    #[arg(long = "kind", value_enum)]
    file_kind: Option<KindFilter>,
    #[arg(long = "type")]
    extension: Option<String>,
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(long, value_enum)]
    category: Option<Filter>,
    /// Distribution for command examples. Defaults to the detected family.
    #[arg(long, value_enum)]
    distro: Option<DistroFilter>,
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=100))]
    limit: u32,
    #[arg(long)]
    json: bool,
    /// Report elapsed search time on stderr.
    #[arg(long)]
    time: bool,
    /// Read one query per line from stdin and write one JSON result array per query.
    #[arg(long, conflicts_with = "query")]
    stdin: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum KindFilter {
    Text,
    #[value(alias = "images")]
    Image,
    #[value(alias = "videos")]
    Video,
}

impl From<KindFilter> for FileKind {
    fn from(value: KindFilter) -> Self {
        match value {
            KindFilter::Text => Self::Text,
            KindFilter::Image => Self::Image,
            KindFilter::Video => Self::Video,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Filter {
    #[value(alias = "files")]
    File,
    #[value(alias = "applications")]
    Application,
    #[value(alias = "settings")]
    Setting,
    #[value(alias = "commands")]
    Command,
    #[value(alias = "actions")]
    Action,
}

impl From<Filter> for Category {
    fn from(value: Filter) -> Self {
        match value {
            Filter::File => Self::File,
            Filter::Application => Self::Application,
            Filter::Setting => Self::Setting,
            Filter::Command => Self::Command,
            Filter::Action => Self::Action,
        }
    }
}

fn main() -> ExitCode {
    let language = Language::current();
    let messages = Messages::for_language(language);
    let mut command = localized_help(messages);
    let matches = command
        .try_get_matches_from_mut(env::args_os())
        .unwrap_or_else(|error| {
            if language != Language::En && error.kind() == clap::error::ErrorKind::DisplayHelp {
                print_localized_help(error, messages);
            }
            error.exit()
        });
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    if cli.search.stdin {
        match &mut cli.command {
            Some(Command::Search(search)) if search.query.is_none() => search.stdin = true,
            None => {}
            _ => command
                .error(
                    clap::error::ErrorKind::ArgumentConflict,
                    "--stdin requires search without a positional query",
                )
                .exit(),
        }
    }
    if cli.command.is_none()
        && !cli.search.stdin
        && cli.search.query.is_none()
        && cli.search.file_kind.is_none()
    {
        command
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                messages.command_required,
            )
            .exit();
    }
    if matches!(&cli.command, Some(Command::Search(search)) if !search.stdin && search.query.is_none() && search.file_kind.is_none())
    {
        command
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                messages.query_required,
            )
            .exit();
    }
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    match run(cli, &mut output, messages).and_then(|()| Ok(output.flush()?)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe) =>
        {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("semfind: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn localized_help(messages: &Messages) -> clap::Command {
    apply_search_help(Cli::command().about(messages.about), messages)
        .mut_subcommand("init", |sub| {
            sub.about(messages.init)
                .mut_arg("exclude", |arg| arg.help(messages.exclude))
                .mut_arg("clear_excludes", |arg| arg.help(messages.clear_excludes))
                .mut_arg("no_caption_images", |arg| {
                    arg.help(messages.no_caption_images)
                })
        })
        .mut_subcommand("index", |sub| {
            sub.about(messages.index)
                .mut_arg("exclude", |arg| arg.help(messages.exclude))
                .mut_arg("clear_excludes", |arg| arg.help(messages.clear_excludes))
                .mut_arg("manpages", |arg| arg.help(messages.manpages))
                .mut_arg("caption_images", |arg| arg.help(messages.caption_images))
        })
        .mut_subcommand("status", |sub| {
            sub.about(messages.status)
                .mut_arg("json", |arg| arg.help(messages.json))
        })
        .mut_subcommand("unindex", |sub| sub.about(messages.unindex))
        .mut_subcommand("search", |sub| {
            apply_search_help(sub.about(messages.search), messages)
        })
}

fn apply_search_help(command: clap::Command, messages: &Messages) -> clap::Command {
    [
        ("query", messages.query),
        ("file_kind", messages.kind),
        ("distro", messages.distro),
        ("json", messages.json),
        ("category", messages.category),
        ("path", messages.path),
        ("extension", messages.extension),
        ("limit", messages.limit),
        ("time", messages.time),
        ("stdin", messages.stdin),
    ]
    .into_iter()
    .fold(command, |cmd, (id, text)| {
        cmd.mut_arg(id, |arg| arg.help(text))
    })
}

fn print_localized_help(error: clap::Error, messages: &Messages) -> ! {
    let mut help = error.to_string();
    let [
        usage,
        commands,
        arguments,
        options,
        print_help,
        print_version,
        help_command,
    ] = messages.help_headers;
    for (original, localized) in [
        ("Usage", usage),
        ("Commands", commands),
        ("Arguments", arguments),
        ("Options", options),
    ] {
        help = help.replace(&format!("\n{original}:"), &format!("\n{localized}:"));
    }
    for (original, localized) in [
        (
            "Print this message or the help of the given subcommand(s)",
            help_command,
        ),
        ("Print help", print_help),
        ("Print version", print_version),
    ] {
        help = help.replace(original, localized);
    }
    let status = match io::stdout().lock().write_all(help.as_bytes()) {
        Ok(()) => 0,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 1,
    };
    std::process::exit(status)
}

fn run(cli: Cli, output: &mut impl Write, messages: &Messages) -> Result<()> {
    let dir = match cli.index_dir {
        Some(path) => path,
        None => data_dir(messages)?.join("index"),
    };
    let mut index =
        Index::open(&dir).with_context(|| format!("{} {}", messages.open_index, dir.display()))?;
    if cfg!(target_os = "linux")
        && !matches!(cli.command, Some(Command::Init { .. }))
        && !index.is_initialized()?
    {
        for line in messages.hint {
            eprintln!("semfind: {line}");
        }
    }
    let selected_model = match (cli.model, &cli.model_dir) {
        (Some(choice), _) => choice,
        (None, None)
            if index
                .embedding_model()?
                .is_some_and(|identity| identity.starts_with("e5-mean-pool-v1:")) =>
        {
            ModelChoice::E5Small
        }
        _ => ModelChoice::Granite97M,
    };
    let start_model = || {
        let directory = match &cli.model_dir {
            Some(path) => path.clone(),
            None => data_dir(messages)?
                .join("model")
                .join(selected_model.directory()),
        };
        let device = match cli.device {
            DeviceChoice::Auto => ExecutionDevice::Auto,
            DeviceChoice::Cpu => ExecutionDevice::Cpu,
            DeviceChoice::Cuda => ExecutionDevice::Cuda,
            DeviceChoice::Migraphx => ExecutionDevice::Migraphx,
        };
        LocalModel::start_on(&cli.python, &directory, device)
    };
    let caption_directory = || -> Result<PathBuf> {
        match &cli.caption_model_dir {
            Some(path) => Ok(path.clone()),
            None => Ok(data_dir(messages)?.join("model/florence-2-base-ft")),
        }
    };
    match cli.command {
        Some(Command::Status { json }) => {
            let status = index.status()?;
            if json {
                return print_json(output, &status);
            }
            let [
                files,
                settings,
                applications,
                commands,
                passages,
                root_label,
            ] = messages.status_labels;
            writeln!(
                output,
                "{files}: {} | {settings}: {} | {applications}: {} | {commands}: {} | {passages}: {}",
                status.files, status.settings, status.applications, status.commands, status.chunks
            )?;
            for root in status.roots {
                writeln!(output, "{root_label}: {}", root.display())?;
            }
        }
        Some(Command::Unindex { root }) => {
            ensure!(
                index.remove_root(&root)?,
                "{}: {}",
                messages.root_missing,
                root.display()
            );
            writeln!(output, "{} {}", messages.root_removed, root.display())?;
        }
        Some(Command::Init {
            exclude,
            clear_excludes,
            no_caption_images,
        }) => {
            let home = env::var_os("HOME").context(messages.home_required)?;
            let directory = (!no_caption_images).then(caption_directory).transpose()?;
            let mut captioner = match directory.as_deref() {
                Some(path) => match open_optional_captioner(&cli.python, path) {
                    Ok(captioner) => captioner,
                    Err(error) => {
                        eprintln!(
                            "semfind: Florence-2 unavailable. Continuing without new captions. {error:#}"
                        );
                        None
                    }
                },
                None => None,
            };
            let report = index.initialize_home_with_image_captions(
                Path::new(&home),
                &IndexOptions {
                    exclude: (clear_excludes || !exclude.is_empty()).then_some(exclude),
                },
                &mut start_model()?,
                captioner.as_mut().map(|model| model as &mut _),
            )?;
            print_index_report(output, &report, messages)?;
        }
        Some(Command::Index {
            root,
            manpages,
            caption_images,
            exclude,
            clear_excludes,
        }) => {
            let options = IndexOptions {
                exclude: (clear_excludes || !exclude.is_empty()).then_some(exclude),
            };
            let mut model = start_model()?;
            let report = if caption_images {
                let directory = caption_directory()?;
                let mut captioner = FlorenceCaptioner::new(&cli.python, &directory)?;
                index.synchronize_with_image_captions(
                    root.as_deref().expect("--caption-images requires a root"),
                    &options,
                    manpages.then(Distro::current),
                    &mut model,
                    &mut captioner,
                )?
            } else if manpages {
                index.synchronize_with_manpages(
                    root.as_deref(),
                    &options,
                    Distro::current(),
                    &mut model,
                )?
            } else {
                index.synchronize(root.as_deref(), &options, &mut model)?
            };
            print_index_report(output, &report, messages)?;
        }
        Some(Command::Search(mut search)) => {
            search.time |= cli.search.time;
            run_search(&index, search, start_model, output, messages)?;
        }
        None => run_search(&index, cli.search, start_model, output, messages)?,
    }
    Ok(())
}

fn open_optional_captioner<'a>(
    python: &'a Path,
    directory: &'a Path,
) -> Result<Option<FlorenceCaptioner<'a>>> {
    if directory
        .try_exists()
        .with_context(|| format!("check {}", directory.display()))?
    {
        Ok(Some(FlorenceCaptioner::new(python, directory)?))
    } else {
        Ok(None)
    }
}

fn print_index_report(
    output: &mut impl Write,
    report: &IndexReport,
    messages: &Messages,
) -> Result<()> {
    let [written, unchanged, removed, skipped] = messages.report_labels;
    for warning in &report.warnings {
        eprintln!("semfind: {skipped}: {warning}");
    }
    writeln!(
        output,
        "{written}: {} | {unchanged}: {} | {removed}: {} | {skipped}: {}",
        report.written, report.unchanged, report.removed, report.skipped
    )?;
    Ok(())
}

fn run_search(
    index: &Index,
    search: Search,
    start_model: impl Fn() -> Result<LocalModel>,
    output: &mut impl Write,
    messages: &Messages,
) -> Result<()> {
    let local_distro = Distro::current();
    let selected_distro = search.distro.map(Into::into).unwrap_or(local_distro);
    let options = SearchOptions {
        path: search.path,
        extension: search.extension,
        file_kind: search.file_kind.map(Into::into),
        category: search.category.map(Into::into),
        distro: Some(selected_distro),
        include_local_commands: selected_distro == local_distro,
        limit: search.limit as usize,
    };
    let mut model = None;
    let mut find = |query: &str| -> Result<_> {
        let started = search.time.then(Instant::now);
        let hits = index.search_with_cached_model(query, &options, &mut model, &start_model)?;
        if let Some(started) = started {
            let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
            eprintln!("semfind: {} {milliseconds:.2} ms", messages.search_time);
        }
        Ok(hits)
    };
    if search.stdin {
        for (number, line) in io::stdin().lock().lines().enumerate() {
            let context = || format!("stdin line {}", number + 1);
            let line = line.with_context(context)?;
            let query = line.trim();
            if !query.is_empty() {
                let hits = find(query).with_context(context)?;
                print_json(output, &hits)?;
                output.flush()?;
            }
        }
        return Ok(());
    }
    let hits = find(search.query.as_deref().unwrap_or_default())?;
    if search.json {
        return print_json(output, &hits);
    }
    for hit in hits {
        let category = messages.category(hit.category);
        let label = hit.distro.map_or_else(
            || category.to_owned(),
            |family| format!("{category}/{}", family.as_str()),
        );
        writeln!(
            output,
            "[{}] {}  ({:.4})",
            label,
            visible(&hit.title),
            hit.score
        )?;
        let id = visible(&hit.id);
        match hit.location {
            Some(HitLocation::Lines {
                line_start,
                line_end,
            }) if line_start == line_end => {
                writeln!(output, "  {id}:{line_start}")?;
            }
            Some(HitLocation::Lines {
                line_start,
                line_end,
            }) => {
                writeln!(output, "  {id}:{line_start}-{line_end}")?;
            }
            Some(HitLocation::Page { page }) => writeln!(output, "  {id}#page={page}")?,
            None => writeln!(output, "  {id}")?,
        }
        if !hit.snippet.is_empty() {
            writeln!(output, "  {}", visible(&hit.snippet))?;
        }
        if let Some(example) = &hit.example {
            writeln!(output, "  $ {}", visible(example))?;
        }
    }
    Ok(())
}

fn visible(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn print_json(output: &mut impl Write, data: &impl serde::Serialize) -> Result<()> {
    // Buffer the record so a write error remains an io::Error for pipe handling.
    let json = serde_json::to_string(data)?;
    writeln!(output, "{json}")?;
    Ok(())
}

fn data_dir(messages: &Messages) -> Result<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .context(messages.data_dir_missing)?;
    Ok(base.join("semfind"))
}
