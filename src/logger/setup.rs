//! Setup logging subsystem.

use std::collections::{HashMap, HashSet};

use log_utils::{
    AdditionalFieldsPlacement, ConsoleLogFormat, ConsoleLoggingConfig, DirectivePrintTarget,
    LoggerConfig, LoggerError,
};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{Layer, prelude::*};

use super::config;

fn get_envfilter_directive(
    default_log_level: tracing::Level,
    filter_log_level: tracing::Level,
    crates_to_filter: &[&'static str],
) -> String {
    let mut workspace_members = build_info::cargo_workspace_members!();
    workspace_members.extend(build_info::framework_libs_workspace_members());
    workspace_members.extend(crates_to_filter.iter().copied());

    workspace_members
        .into_iter()
        .zip(std::iter::repeat(filter_log_level))
        .fold(
            vec![default_log_level.to_string()],
            |mut directives, (target, level)| {
                directives.push(format!("{}={}", target, level));
                directives
            },
        )
        .join(",")
}

fn get_logger_config(
    config: &config::Log,
    service_name: &str,
    crates_to_filter: &[&'static str],
) -> LoggerConfig {
    let console_config = if config.console.enabled {
        let console_filter_directive =
            config
                .console
                .filtering_directive
                .clone()
                .unwrap_or_else(|| {
                    get_envfilter_directive(
                        tracing::Level::WARN,
                        config.console.level.into_level(),
                        crates_to_filter,
                    )
                });

        let log_format = match config.console.log_format {
            config::LogFormat::Default => ConsoleLogFormat::HumanReadable,
            config::LogFormat::Json => {
                error_stack::Report::set_color_mode(error_stack::fmt::ColorMode::None);
                ConsoleLogFormat::CompactJson
            }
        };

        Some(ConsoleLoggingConfig {
            level: config.console.level.into_level(),
            log_format,
            filtering_directive: Some(console_filter_directive),
            print_filtering_directive: DirectivePrintTarget::Stdout,
        })
    } else {
        None
    };

    LoggerConfig {
        static_top_level_fields: HashMap::from([
            ("service".to_string(), serde_json::json!(service_name)),
            #[cfg(feature = "vergen")]
            (
                "build_version".to_string(),
                serde_json::json!(crate::version!()),
            ),
        ]),
        top_level_keys: HashSet::new(),
        persistent_keys: HashSet::new(),
        log_span_lifecycles: true,
        additional_fields_placement: AdditionalFieldsPlacement::TopLevel,
        file_config: None,
        console_config,
        global_filtering_directive: None,
    }
}

/// Contains guards necessary for logging
#[derive(Debug)]
pub struct TelemetryGuard {
    _log_guards: Vec<WorkerGuard>,
}

/// Setup logging sub-system specifying the logging configuration, service (binary) name, and a
/// list of external crates for which a more verbose logging must be enabled. All crates within the
/// current cargo workspace are automatically considered for verbose logging.
pub fn setup(
    config: &config::Log,
    service_name: &str,
    crates_to_filter: impl AsRef<[&'static str]>,
) -> Result<TelemetryGuard, LoggerError> {
    let logger_config = get_logger_config(config, service_name, crates_to_filter.as_ref());

    let components = log_utils::build_logging_components(logger_config)?;

    let mut layers: Vec<Box<dyn Layer<_> + Send + Sync>> = Vec::new();
    layers.push(components.storage_layer.boxed());

    if let Some(console_layer) = components.console_log_layer {
        layers.push(console_layer);
    }

    let subscriber = tracing_subscriber::registry().with(layers);

    #[cfg(feature = "console")]
    let subscriber = subscriber.with(console_subscriber::spawn());

    subscriber.init();

    Ok(TelemetryGuard {
        _log_guards: components.guards,
    })
}
