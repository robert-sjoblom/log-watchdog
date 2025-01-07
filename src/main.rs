use clap::Parser;
use log_watchdog::run;
use settings::Settings;

#[derive(clap::Parser, Debug)]
struct Args {
    /// The settings file used to configure the watchdogs.
    ///
    /// The file should be in YAML format, and has the following schema:
    ///   watchdogs:
    ///     watchdog_name:
    ///     log_file: path/to/log/file.log
    ///     output_file: path/to/output/file.txt
    ///     debounce: 1000
    ///     oneshot: false
    ///     regex: .*
    ///     commands:
    ///       curl:
    ///         args:
    ///          - https://example.com
    ///          - -v
    #[clap(short, long, verbatim_doc_comment, value_parser = settings_from_path)]
    settings: Settings,
}

fn settings_from_path(path: &str) -> Result<Settings, settings::SettingsError> {
    let path = std::path::Path::new(path);
    Settings::try_from(path)
}

fn main() {
    let args = Args::parse();
    let _logging = logging::init_logging();

    run(args.settings);
}
