#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use log_watchdog::run;
    use settings::Settings;

    struct TestSettings {
        log_name: &'static str,
        out_name: &'static str,
        debounce: u64,
        oneshot: bool,
        regex: &'static str,
    }

    type SettingsPath = PathBuf;
    type LogPath = PathBuf;
    type OutPath = PathBuf;

    fn setup_settings(dir: &Path, test_settings: TestSettings) -> (SettingsPath, LogPath, OutPath) {
        let log_path = dir.join(test_settings.log_name);
        let outfile_path = dir.join(test_settings.out_name);

        let settings = format!(
            r#"
watchdogs:
  stdout_txt:
    log_file: {}
    output_file: {}
    debounce: {}
    oneshot: {}
    regex: {}
    commands:
      echo:
        args:
          - "hello world!"
        "#,
            log_path.to_str().unwrap(),
            outfile_path.to_str().unwrap(),
            test_settings.debounce,
            test_settings.oneshot,
            test_settings.regex
        );

        let settings_path = dir.join("settings.yml");
        let mut settings_file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&settings_path)
            .unwrap();

        write!(settings_file, "{}", settings).unwrap();

        (settings_path, log_path, outfile_path)
    }

    #[test]
    fn when_match_then_output_is_saved() {
        let dir = tempdir::TempDir::new("test_").unwrap();
        let settings = TestSettings {
            log_name: "log.txt",
            out_name: "out.txt",
            debounce: 0,
            oneshot: true,
            regex: "^aaa",
        };

        let (settings_path, log_path, outfile_path) = setup_settings(dir.path(), settings);

        let settings = Settings::try_from(settings_path.as_path()).unwrap();

        let mut log_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path)
            .unwrap();

        write!(log_file, "foo bar baz").unwrap();

        // run binary on different thread
        std::thread::spawn(move || {
            run(settings);
        });

        std::thread::sleep(std::time::Duration::from_secs(1));

        writeln!(log_file, "\naaa").unwrap();
        writeln!(log_file, "aaaa").unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let contents = std::fs::read_to_string(outfile_path).unwrap();

        assert_eq!(contents, "hello world!\n\n");
    }

    #[test]
    fn when_no_match_then_no_output() {
        let dir = tempdir::TempDir::new("test_").unwrap();
        let settings = TestSettings {
            log_name: "log.txt",
            out_name: "out.txt",
            debounce: 0,
            oneshot: true,
            regex: "^aaa",
        };

        let (settings_path, log_path, outfile_path) = setup_settings(dir.path(), settings);

        let settings = Settings::try_from(settings_path.as_path()).unwrap();

        let mut log_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path)
            .unwrap();

        write!(log_file, "foo bar baz").unwrap();

        // run binary on different thread
        std::thread::spawn(move || {
            run(settings);
        });

        std::thread::sleep(std::time::Duration::from_secs(1));

        writeln!(log_file, "\nbbb").unwrap();
        writeln!(log_file, "abbb").unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let contents = std::fs::read_to_string(&outfile_path).unwrap_or_default();
        assert!(contents.is_empty());
    }
}
