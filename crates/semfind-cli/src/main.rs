use anyhow::{Context, Result, ensure};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum, parser::ValueSource};
use semfind_core::{
    Category, Distro, Embedder, ExecutionDevice, FileKind, FlorenceCaptioner, HitLocation, Index,
    IndexOptions, IndexReport, InputKind, Language, LocalModel, SearchOptions, Status,
};
mod config;
mod distro;
#[cfg(target_os = "linux")]
mod gpu_setup;
mod progress;
#[cfg(target_os = "linux")]
mod resident;
mod ui;
use distro::DistroFilter;
use std::{
    env,
    io::{self, BufRead, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};
use ui::Messages;

const DEFAULT_DAEMON_IDLE_SECONDS: NonZeroU32 = NonZeroU32::new(15 * 60).unwrap();

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
    /// Select an embedding model. New indexes default to Granite 97M.
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
    /// Suppress the GPU setup tip shown during CPU inference.
    #[arg(long, env = "SEMFIND_NO_GPU_HINT", global = true)]
    no_gpu_hint: bool,
    /// Run search inference in this process and skip the background worker.
    #[arg(long, env = "SEMFIND_NO_DAEMON", global = true)]
    no_daemon: bool,
    /// Stop the background model after this many idle seconds.
    #[arg(
        long,
        env = "SEMFIND_DAEMON_IDLE_SECONDS",
        default_value_t = DEFAULT_DAEMON_IDLE_SECONDS,
        value_name = "SECONDS",
        global = true
    )]
    daemon_idle_seconds: NonZeroU32,
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
    fn name(self) -> &'static str {
        match self {
            Self::E5Small => "E5 Small",
            Self::E5Large => "E5 Large",
            Self::Granite97M => "Granite 97M Multilingual R2",
            Self::Granite311M => "Granite 311M Multilingual R2",
        }
    }

    fn directory(self) -> &'static str {
        match self {
            Self::E5Small => "",
            Self::E5Large => "e5-large",
            Self::Granite97M => "granite-97m-r2",
            Self::Granite311M => "granite-311m-r2",
        }
    }

    fn from_identity(identity: &str) -> Option<Self> {
        let mut parts = identity.split(':');
        let recipe = parts.next();
        let tokenizer = parts.nth(1);
        match (recipe, tokenizer) {
            (
                Some("e5-mean-pool-v1"),
                Some("62c24cdc13d4c9952d63718d6c9fa4c287974249e16b7ade6b5d85e7bbb75626"),
            ) => Some(Self::E5Large),
            (
                Some("e5-mean-pool-v1"),
                Some("0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39"),
            ) => Some(Self::E5Small),
            (
                Some("granite-r2-cls-512-v1"),
                Some("0087c868b33bad550a78a08d19798cfd7f713cde4f020803b8f51f405503e15f"),
            ) => Some(Self::Granite311M),
            (
                Some("granite-r2-cls-512-v1"),
                Some("4f2842d568e2724370aec203652a42ac783c7937f8347a1a2cc7506d71f1582f"),
            ) => Some(Self::Granite97M),
            _ => None,
        }
    }

    fn for_index(identity: &str) -> Self {
        Self::from_identity(identity).unwrap_or_else(|| {
            if identity.starts_with("e5-mean-pool-v1:") {
                Self::E5Small
            } else {
                Self::Granite97M
            }
        })
    }

    fn index_model_name(identity: &str) -> &'static str {
        match Self::from_identity(identity) {
            Some(Self::E5Small)
                if identity.split(':').nth(1)
                    == Some("dd476dd0c2514e9b9be83aeb3853fac0763e0bdf4a71645407587d77c48a2d88") =>
            {
                "E5 Small (INT8)"
            }
            Some(model) => model.name(),
            None => "Custom ONNX model",
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Index the home directory, desktop applications, local commands and built-in catalogs.
    Init {
        /// Rebuild the entire index, including all registered directories.
        #[arg(long)]
        force: bool,
        /// Report indexing time and phase durations on stderr.
        #[arg(long)]
        time: bool,
        /// Report model execution providers as JSON on stderr.
        #[arg(long)]
        runtime_info: bool,
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
        /// Report model execution providers as JSON on stderr.
        #[arg(long)]
        runtime_info: bool,
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
    /// Rebuild the saved passages with the selected embedding model.
    Reindex {
        /// Report model execution providers as JSON on stderr.
        #[arg(long)]
        runtime_info: bool,
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
    /// Show or change persistent Semfind settings.
    Config {
        #[command(subcommand)]
        setting: Setting,
    },
    /// Install and verify a GPU runtime for text embeddings.
    #[cfg(target_os = "linux")]
    SetupGpu {
        #[command(subcommand)]
        backend: GpuSetup,
    },
    #[cfg(target_os = "linux")]
    #[command(hide = true)]
    ServeModel {
        directory: PathBuf,
        #[arg(long)]
        runtime: Option<PathBuf>,
    },
}

#[cfg(target_os = "linux")]
#[derive(Subcommand)]
enum GpuSetup {
    /// Set up NVIDIA CUDA in your Semfind data directory.
    Cuda,
    /// Set up AMD MIGraphX (requires compatible ROCm libraries).
    Migraphx {
        /// Use a compatible local ONNX Runtime MIGraphX wheel.
        #[arg(long)]
        wheel: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Setting {
    /// Show or save the background model timeout in seconds.
    DaemonIdleSeconds {
        seconds: Option<NonZeroU32>,
        /// Restore the built-in default.
        #[arg(long, conflicts_with = "seconds")]
        reset: bool,
    },
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
    /// Report model execution providers as JSON on stderr.
    #[arg(long)]
    runtime_info: bool,
    /// Skip reading or saving the last search's embedding.
    #[arg(long)]
    no_cache: bool,
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
    let discover_gpu = matches.value_source("python") != Some(ValueSource::CommandLine);
    let idle_override = matches!(
        matches.value_source("daemon_idle_seconds"),
        Some(ValueSource::CommandLine | ValueSource::EnvVariable)
    )
    .then_some(cli.daemon_idle_seconds);
    match run(cli, &mut output, messages, discover_gpu, idle_override)
        .and_then(|()| Ok(output.flush()?))
    {
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
    let command = Cli::command()
        .about(messages.about)
        .mut_arg("no_gpu_hint", |arg| arg.help(messages.no_gpu_hint))
        .mut_arg("no_daemon", |arg| arg.help(messages.no_daemon))
        .mut_arg("daemon_idle_seconds", |arg| {
            arg.help(messages.daemon_idle_seconds)
        });
    #[cfg(target_os = "linux")]
    let command = command.mut_subcommand("setup-gpu", |sub| sub.about(messages.setup_gpu));
    apply_search_help(command, messages)
        .mut_subcommand("init", |sub| {
            sub.about(messages.init)
                .mut_arg("force", |arg| arg.help(messages.force_init))
                .mut_arg("time", |arg| arg.help(messages.init_time))
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
        .mut_subcommand("config", |sub| {
            sub.about(messages.config)
                .mut_subcommand("daemon-idle-seconds", |setting| {
                    setting
                        .about(messages.config_daemon_idle)
                        .mut_arg("reset", |arg| arg.help(messages.config_reset))
                })
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
        ("no_cache", messages.no_cache),
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

fn run(
    cli: Cli,
    output: &mut impl Write,
    messages: &'static Messages,
    discover_gpu: bool,
    idle_override: Option<NonZeroU32>,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(Command::SetupGpu { backend }) = &cli.command {
        let (backend, wheel) = match backend {
            GpuSetup::Cuda => (gpu_setup::Backend::Cuda, None),
            GpuSetup::Migraphx { wheel } => (gpu_setup::Backend::Migraphx, wheel.as_deref()),
        };
        return gpu_setup::install(
            &data_dir(messages)?.join("runtime"),
            &cli.python,
            backend,
            wheel,
            output,
        );
    }
    if let Some(Command::Config {
        setting: Setting::DaemonIdleSeconds { seconds, reset },
    }) = cli.command
    {
        let path = config::daemon_idle_path().context(messages.config_dir_missing)?;
        let seconds = match (seconds, reset) {
            (Some(seconds), _) => {
                config::save_daemon_idle(&path, seconds)?;
                seconds
            }
            (None, true) => {
                config::reset_daemon_idle(&path)?;
                DEFAULT_DAEMON_IDLE_SECONDS
            }
            (None, false) => {
                config::read_daemon_idle(&path)?.unwrap_or(DEFAULT_DAEMON_IDLE_SECONDS)
            }
        };
        writeln!(output, "{seconds}")?;
        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    let _ = idle_override;
    #[cfg(target_os = "linux")]
    if let Some(Command::ServeModel { directory, runtime }) = &cli.command {
        return resident::serve(&cli.python, directory, runtime.as_deref());
    }
    let mut gpu_hint = GpuHint {
        pending: !cli.no_gpu_hint,
        messages,
    };
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
        (None, None) => index
            .embedding_model()?
            .as_deref()
            .map_or(ModelChoice::Granite97M, ModelChoice::for_index),
        _ => ModelChoice::Granite97M,
    };
    let saved_model_directory = if cli.model.is_none() && cli.model_dir.is_none() {
        index.embedding_model_directory()?
    } else {
        None
    };
    let model_directory = || -> Result<PathBuf> {
        match cli.model_dir.as_ref().or(saved_model_directory.as_ref()) {
            Some(path) => Ok(path.clone()),
            None => Ok(data_dir(messages)?
                .join("model")
                .join(selected_model.directory())),
        }
    };
    let device = match cli.device {
        DeviceChoice::Auto => ExecutionDevice::Auto,
        DeviceChoice::Cpu => ExecutionDevice::Cpu,
        DeviceChoice::Cuda => ExecutionDevice::Cuda,
        DeviceChoice::Migraphx => ExecutionDevice::Migraphx,
    };
    let runtime_dir = if discover_gpu {
        data_dir(messages).ok().map(|path| path.join("runtime"))
    } else {
        None
    };
    let start_model = || {
        start_text_model(
            &cli.python,
            &model_directory()?,
            device,
            runtime_dir.as_deref(),
        )
    };
    let cache_model_directory = || {
        if discover_gpu && matches!(device, ExecutionDevice::Auto) {
            model_directory().ok()
        } else {
            None
        }
    };
    let start_search_model = |allow_resident: bool| -> Result<SearchModel> {
        #[cfg(target_os = "linux")]
        if allow_resident
            && !cli.no_daemon
            && let Some(directory) = cache_model_directory()
        {
            let idle = idle_override.unwrap_or_else(|| {
                if let Some(path) = config::daemon_idle_path() {
                    match config::read_daemon_idle(&path) {
                        Ok(Some(seconds)) => return seconds,
                        Ok(None) => {}
                        Err(error) => eprintln!("semfind: {error:#}"),
                    }
                }
                DEFAULT_DAEMON_IDLE_SECONDS
            });
            if let Ok(model) =
                resident::Client::connect(&cli.python, &directory, runtime_dir.as_deref(), idle)
            {
                return Ok(SearchModel::Resident(model));
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = allow_resident;
        start_model().map(SearchModel::Local)
    };
    let caption_directory = || -> Result<PathBuf> {
        match &cli.caption_model_dir {
            Some(path) => Ok(path.clone()),
            None => Ok(data_dir(messages)?.join("model/florence-2-base-ft")),
        }
    };
    let allow_caption_gpu = !matches!(device, ExecutionDevice::Cpu);
    match cli.command {
        Some(Command::Status { json }) => {
            let status = index.status()?;
            if json {
                let model_name = match &status.model {
                    Some(identity) => ModelChoice::index_model_name(identity),
                    None if cli.model_dir.is_some() => "Custom ONNX model",
                    None => selected_model.name(),
                };
                let caption_model_name = caption_directory()
                    .ok()
                    .filter(|path| FlorenceCaptioner::files_present(path))
                    .map(|_| {
                        if cli.caption_model_dir.is_some() {
                            "Florence-2 (local checkpoint)"
                        } else {
                            "Florence-2 Base FT"
                        }
                    });
                return print_json(
                    output,
                    &StatusWithModels {
                        index: status,
                        model_name,
                        caption_model_name,
                    },
                );
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
        Some(Command::Reindex { runtime_info }) => {
            let directory = std::path::absolute(model_directory()?)?;
            let mut model = start_model()?;
            gpu_hint.show(model.uses_cpu(), false);
            let passages = index.rebuild_embeddings(&directory, &mut model)?;
            gpu_hint.show(model.uses_cpu(), false);
            if runtime_info {
                report_index_runtime(&model, None);
            }
            writeln!(output, "Rebuilt embeddings for {passages} passages.")?;
        }
        Some(Command::Init {
            force,
            time,
            runtime_info,
            exclude,
            clear_excludes,
            no_caption_images,
        }) => {
            let mut timing = (time || cli.search.time).then(progress::Timing::start);
            let home = env::var_os("HOME").context(messages.home_required)?;
            let mut model = start_model()?;
            #[cfg(target_os = "linux")]
            if matches!(device, ExecutionDevice::Auto)
                && model.uses_cpu()
                && let Some(root) = runtime_dir.as_deref()
                && gpu_setup::offer_on_init(root, &cli.python, messages)
            {
                match start_model() {
                    Ok(accelerated) => model = accelerated,
                    Err(error) => {
                        eprintln!("semfind: {error:#}\nsemfind: {}", messages.gpu_offer_failed)
                    }
                }
            }
            let directory = (!no_caption_images).then(caption_directory).transpose()?;
            let mut captioner = match directory.as_deref() {
                Some(path) => match open_optional_captioner(
                    &cli.python,
                    path,
                    runtime_dir.as_deref(),
                    allow_caption_gpu,
                ) {
                    Ok(captioner) => captioner,
                    Err(error) if force => return Err(error),
                    Err(error) => {
                        eprintln!(
                            "semfind: Florence-2 unavailable. Continuing without new captions. {error:#}"
                        );
                        None
                    }
                },
                None => None,
            };
            let progress = progress::Progress::start(messages);
            gpu_hint.show(model.uses_cpu(), progress.is_some());
            let initialize = if force {
                Index::rebuild_home_with_progress
            } else {
                Index::initialize_home_with_progress
            };
            let report = initialize(
                &mut index,
                Path::new(&home),
                &IndexOptions {
                    exclude: (clear_excludes || !exclude.is_empty()).then_some(exclude),
                },
                &mut model,
                captioner.as_mut().map(|model| model as &mut _),
                |phase| {
                    if let Some(timing) = &mut timing {
                        timing.update(phase);
                    }
                    if let Some(progress) = &progress {
                        progress.update(phase);
                    }
                },
            );
            let timing = timing.map(progress::Timing::finish);
            drop(progress);
            gpu_hint.show(model.uses_cpu(), false);
            let report = report?;
            let caption_backend = captioner.as_ref().and_then(FlorenceCaptioner::backend);
            if runtime_info {
                report_index_runtime(&model, caption_backend);
            }
            if let Some(timing) = timing {
                eprintln!(
                    "semfind: ONNX Runtime (startup): {}",
                    model.initial_provider()
                );
                if let Some(backend) = caption_backend {
                    eprintln!("semfind: Florence-2: {backend}");
                }
                timing.print(messages);
            }
            print_index_report(output, &report, messages)?;
        }
        Some(Command::Index {
            root,
            runtime_info,
            manpages,
            caption_images,
            exclude,
            clear_excludes,
        }) => {
            let options = IndexOptions {
                exclude: (clear_excludes || !exclude.is_empty()).then_some(exclude),
            };
            let mut model = start_model()?;
            gpu_hint.show(model.uses_cpu(), false);
            let directory = caption_images.then(caption_directory).transpose()?;
            let mut captioner = directory
                .as_deref()
                .map(|path| {
                    FlorenceCaptioner::with_runtime(
                        &cli.python,
                        path,
                        runtime_dir.as_deref(),
                        allow_caption_gpu,
                    )
                })
                .transpose()?;
            let report = if let Some(captioner) = &mut captioner {
                index.synchronize_with_image_captions(
                    root.as_deref().expect("--caption-images requires a root"),
                    &options,
                    manpages.then(Distro::current),
                    &mut model,
                    captioner,
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
            gpu_hint.show(model.uses_cpu(), false);
            if runtime_info {
                report_index_runtime(
                    &model,
                    captioner.as_ref().and_then(FlorenceCaptioner::backend),
                );
            }
            print_index_report(output, &report, messages)?;
        }
        Some(Command::Search(mut search)) => {
            search.time |= cli.search.time;
            search.no_cache |= cli.search.no_cache;
            search.runtime_info |= cli.search.runtime_info;
            run_search(
                &index,
                search,
                start_search_model,
                cache_model_directory(),
                output,
                messages,
                gpu_hint,
            )?;
        }
        None => run_search(
            &index,
            cli.search,
            start_search_model,
            cache_model_directory(),
            output,
            messages,
            gpu_hint,
        )?,
        Some(Command::Config { .. }) => unreachable!(),
        #[cfg(target_os = "linux")]
        Some(Command::SetupGpu { .. }) => unreachable!(),
        #[cfg(target_os = "linux")]
        Some(Command::ServeModel { .. }) => unreachable!(),
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct StatusWithModels {
    #[serde(flatten)]
    index: Status,
    model_name: &'static str,
    caption_model_name: Option<&'static str>,
}

fn report_index_runtime(model: &LocalModel, caption_backend: Option<&str>) {
    let provider = active_provider(model.initial_provider(), model.uses_cpu());
    report_runtime(Some((model.identity(), provider)), caption_backend, false);
}

fn active_provider(provider: &str, uses_cpu: bool) -> &str {
    if uses_cpu {
        "CPUExecutionProvider"
    } else {
        provider
    }
}

fn report_runtime(embedding: Option<(&str, &str)>, caption_backend: Option<&str>, cache_hit: bool) {
    let (model, provider) = embedding.unzip();
    let record = serde_json::json!({
        "model": model,
        "embedding_provider": provider,
        "caption_backend": caption_backend,
        "cache_hit": cache_hit,
    });
    eprintln!("semfind: runtime {record}");
}

struct GpuHint<'a> {
    pending: bool,
    messages: &'a Messages,
}

impl GpuHint<'_> {
    fn show(&mut self, uses_cpu: bool, progress_active: bool) {
        if !self.pending || !uses_cpu {
            return;
        }
        self.pending = false;
        let mut stderr = io::stderr().lock();
        let clear = if progress_active { "\r\x1b[K" } else { "" };
        let (cuda, migraphx) = if cfg!(target_os = "linux") {
            (
                "semfind setup-gpu cuda\n                   ",
                "semfind setup-gpu migraphx\n                   ",
            )
        } else {
            ("", "")
        };
        let _ = writeln!(
            stderr,
            "{clear}semfind: {}\nsemfind: {}\n  NVIDIA (CUDA)    {cuda}https://github.com/Engineer1080/semfind#run-inference-on-an-nvidia-gpu\n  AMD (MIGraphX)   {migraphx}https://github.com/Engineer1080/semfind#amd-gpus",
            self.messages.cpu_hint, self.messages.gpu_setup,
        );
    }
}

fn start_text_model(
    python: &Path,
    directory: &Path,
    device: ExecutionDevice,
    runtime_dir: Option<&Path>,
) -> Result<LocalModel> {
    if matches!(device, ExecutionDevice::Auto)
        && let Some(runtime_dir) = runtime_dir
    {
        for (name, backend) in [
            ("cuda", ExecutionDevice::Cuda),
            ("migraphx", ExecutionDevice::Migraphx),
        ] {
            let interpreter = runtime_dir.join(name).join("bin/python");
            if interpreter.is_file() {
                match LocalModel::start_gpu_with_fallback(&interpreter, directory, backend) {
                    Ok(model) => return Ok(model),
                    Err(error) => eprintln!("semfind: {}: {error:#}", interpreter.display()),
                }
            }
        }
    }
    LocalModel::start_on(python, directory, device)
}

fn open_optional_captioner<'a>(
    python: &'a Path,
    directory: &'a Path,
    runtime_dir: Option<&'a Path>,
    allow_gpu: bool,
) -> Result<Option<FlorenceCaptioner<'a>>> {
    if directory
        .try_exists()
        .with_context(|| format!("check {}", directory.display()))?
    {
        Ok(Some(FlorenceCaptioner::with_runtime(
            python,
            directory,
            runtime_dir,
            allow_gpu,
        )?))
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

enum SearchModel {
    Local(LocalModel),
    #[cfg(target_os = "linux")]
    Resident(resident::Client),
}

impl SearchModel {
    fn provider(&self) -> (&str, &str) {
        match self {
            Self::Local(model) => ("startup", model.initial_provider()),
            #[cfg(target_os = "linux")]
            Self::Resident(model) => {
                let state = if model.reused() {
                    "resident"
                } else {
                    "startup"
                };
                (state, model.provider())
            }
        }
    }

    fn uses_cpu(&self) -> bool {
        match self {
            Self::Local(model) => model.uses_cpu(),
            #[cfg(target_os = "linux")]
            Self::Resident(model) => model.uses_cpu(),
        }
    }

    fn embedder(&self) -> &dyn Embedder {
        match self {
            Self::Local(model) => model,
            #[cfg(target_os = "linux")]
            Self::Resident(model) => model,
        }
    }
}

impl Embedder for SearchModel {
    fn identity(&self) -> &str {
        self.embedder().identity()
    }

    fn minimum_similarity(&self) -> Option<f32> {
        self.embedder().minimum_similarity()
    }

    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        match self {
            Self::Local(model) => model.encode(kind, texts),
            #[cfg(target_os = "linux")]
            Self::Resident(model) => model.encode(kind, texts),
        }
    }
}

fn run_search(
    index: &Index,
    search: Search,
    start_model: impl Fn(bool) -> Result<SearchModel>,
    model_directory: Option<PathBuf>,
    output: &mut impl Write,
    messages: &Messages,
    mut gpu_hint: GpuHint<'_>,
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
    let cache_directory = model_directory
        .as_deref()
        .filter(|_| !search.stdin && !search.no_cache);
    let mut find = |query: &str| -> Result<_> {
        let started = search.time.then(Instant::now);
        let starting_model = model.is_none();
        let search_once = |model: &mut Option<_>, allow_resident| {
            let start = || start_model(allow_resident);
            if let Some(directory) = cache_directory {
                index.search_with_cached_query(query, &options, directory, model, start)
            } else {
                index
                    .search_with_cached_model(query, &options, model, start)
                    .map(|hits| (hits, false))
            }
        };
        let result = search_once(&mut model, !search.stdin);
        #[cfg(target_os = "linux")]
        let result = match result {
            Err(error) if error.is::<resident::Disconnected>() => {
                model = None;
                search_once(&mut model, false)
            }
            result => result,
        };
        let (hits, cache_hit) = result?;
        if search.runtime_info {
            let embedding = model.as_ref().map(|model| {
                let provider = active_provider(model.provider().1, model.uses_cpu());
                (model.identity(), provider)
            });
            report_runtime(embedding, None, cache_hit);
        }
        if let Some(started) = started {
            let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
            eprintln!("semfind: {} {milliseconds:.2} ms", messages.search_time);
            if cache_hit {
                eprintln!("semfind: {}", messages.cache_hit);
            } else if starting_model && let Some(model) = &model {
                let (state, provider) = model.provider();
                eprintln!("semfind: ONNX Runtime ({state}): {provider}");
            }
        }
        if let Some(model) = &model {
            gpu_hint.show(model.uses_cpu(), false);
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
