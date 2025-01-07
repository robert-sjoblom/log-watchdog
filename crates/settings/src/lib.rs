use std::{
    collections::HashMap,
    fs::OpenOptions,
    path::{Path, PathBuf},
};

use regex::Regex;
use serde_yaml::Value;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SettingsError {
    #[error("missing setting key: {key}")]
    MissingSettingKey { key: &'static str },
    #[error("invalid value type: key {key} not valid")]
    InvalidValueType { key: String },
    #[error(transparent)]
    Regex(#[from] regex::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SerdeYaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ParseBoolError(#[from] std::str::ParseBoolError),
    #[error(transparent)]
    TryFromIntError(#[from] std::num::TryFromIntError),
}

#[derive(Debug, Clone)]
pub struct Settings {
    watchdogs: Vec<Watchdog>,
}

impl Settings {
    pub fn watchdogs(&self) -> &[Watchdog] {
        &self.watchdogs
    }

    pub fn into_watchdogs(self) -> Vec<Watchdog> {
        self.watchdogs
    }
}

/// A watchdog will watch a log file for a regex match and run any commands when
/// it matches.
#[derive(Debug, Clone)]
pub struct Watchdog {
    /// Watchdog name, will be used in any log output
    pub name: String,
    /// Path to the log file to watch
    pub log_file: PathBuf,
    /// Path to the output file to write to
    pub output_file: PathBuf,
    /// Time in milliseconds to debounce the watchdog after a positive match
    pub debounce: u64,
    /// If true, only run the command once
    pub oneshot: bool,
    /// Regex to match in the log file
    pub regex: Regex,
    /// Commands to run when the regex matches
    pub commands: Vec<Command>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Command {
    /// Name of the program to execute (e.g. `curl`)
    pub name: String,
    /// Any arguments to pass to the program
    pub args: Vec<String>,
}

impl From<&'static str> for SettingsError {
    fn from(key: &'static str) -> Self {
        SettingsError::MissingSettingKey { key }
    }
}

impl TryFrom<HashMap<String, HashMap<String, Value>>> for Settings {
    type Error = SettingsError;

    fn try_from(value: HashMap<String, HashMap<String, Value>>) -> Result<Self, Self::Error> {
        let m = value
            .get("watchdogs")
            .ok_or(SettingsError::from("watchdogs"))?;

        let watchdogs = m
            .iter()
            .map(|(name, v)| {
                let name = name.clone();
                let log_file: PathBuf = get_val_or_err(v, "log_file")?;
                let output_file: PathBuf = get_val_or_err(v, "output_file")?;

                let debounce: u64 = v
                    .get("debounce")
                    .ok_or(SettingsError::from("debounce"))?
                    .as_i64()
                    .ok_or(SettingsError::InvalidValueType {
                        key: "debounce".into(),
                    })?
                    .try_into()?;

                let oneshot: bool = v
                    .get("oneshot")
                    .ok_or(SettingsError::from("oneshot"))?
                    .as_bool()
                    .ok_or(SettingsError::InvalidValueType {
                        key: "oneshot".into(),
                    })?;

                let regex = Regex::new(get_val_or_err::<String>(v, "regex")?.as_str())?;

                let commands = v.get("commands").ok_or(SettingsError::from("commands"))?;

                let commands = parse_commands_value(commands)?;

                Ok(Watchdog {
                    name,
                    log_file,
                    output_file,
                    debounce,
                    oneshot,
                    regex,
                    commands,
                })
            })
            .collect::<Result<Vec<Watchdog>, SettingsError>>()?;

        Ok(Settings { watchdogs })
    }
}

impl TryFrom<&Path> for Settings {
    type Error = SettingsError;

    fn try_from(value: &Path) -> Result<Self, Self::Error> {
        let file = OpenOptions::new().read(true).open(value)?;
        let settings: HashMap<String, HashMap<String, Value>> = serde_yaml::from_reader(file)?;
        Settings::try_from(settings)
    }
}

fn parse_commands_value(commands: &Value) -> Result<Vec<Command>, SettingsError> {
    let commands = commands
        .as_mapping()
        .ok_or(SettingsError::InvalidValueType {
            key: "commands".into(),
        })?;

    commands
        .into_iter()
        .map(|(name, v)| {
            let name = name
                .as_str()
                .ok_or(SettingsError::from("command name"))?
                .to_string();

            let args: Result<Vec<String>, SettingsError> = v
                .as_mapping()
                .ok_or(SettingsError::InvalidValueType {
                    key: "commands.named_command".into(),
                })?
                .get("args")
                .ok_or(SettingsError::from("commands.named_command.args"))?
                .as_sequence()
                .ok_or(SettingsError::InvalidValueType {
                    key: "commands.named_command.args".into(),
                })?
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or(SettingsError::InvalidValueType {
                            key: "commands.named_command.args.arg".into(),
                        })
                        .map(|s| s.to_string())
                })
                .collect();

            Ok(Command { name, args: args? })
        })
        .collect()
}

fn get_val_or_err<T: From<String>>(v: &Value, key: &'static str) -> Result<T, SettingsError> {
    Ok(T::from(
        v.get(key)
            .ok_or(SettingsError::from(key))?
            .as_str()
            .ok_or(SettingsError::InvalidValueType { key: key.into() })?
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use serde_yaml::Value;

    use std::{collections::HashMap, path::PathBuf};

    use super::*;

    #[test]
    fn test_when_valid_settings_then_hashmap_of_hashmaps() {
        let settings_path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("fixtures/valid_settings.yml");

        let yaml_string = std::fs::read_to_string(settings_path).unwrap();
        let settings: Result<HashMap<String, HashMap<String, Value>>, _> =
            serde_yaml::from_str(&yaml_string);

        assert!(settings.is_ok());
    }

    #[test]
    fn test_when_valid_settings_then_settings_from_parse() {
        let settings_path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("fixtures/valid_settings.yml");
        let settings = Settings::try_from(settings_path.as_path()).unwrap();

        assert_eq!(settings.watchdogs[0].name, "pgbouncer");
        assert_eq!(
            settings.watchdogs[0].log_file,
            PathBuf::from("/var/log/pgbouncer/pgbouncer.log")
        );
        assert_eq!(
            settings.watchdogs[0].output_file,
            PathBuf::from("/var/log/pgbouncer/pgbouncer.out")
        );
        assert_eq!(settings.watchdogs[0].regex.as_str(), ".*");
        assert_eq!(settings.watchdogs[0].commands.len(), 1);
        assert_eq!(
            settings.watchdogs[0].commands[0],
            Command {
                name: "ls".into(),
                args: vec!["-a".into()]
            }
        );

        assert_eq!(settings.watchdogs[0].debounce, 5000);
        assert!(settings.watchdogs[0].oneshot);
    }

    #[test]
    fn test_when_invalid_settings_then_error() {
        let settings_path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("fixtures/invalid_settings.yml");
        let settings = Settings::try_from(settings_path.as_path());
        assert!(settings.is_err());
    }
}
