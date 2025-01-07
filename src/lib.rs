use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Seek, SeekFrom, Write},
    process::Command,
    sync::mpsc::{Receiver, Sender, TryRecvError},
    time::{Duration, Instant},
};

use log::{error, info};
use notify::{Config, RecommendedWatcher, Watcher};
use settings::{Settings, Watchdog};
use thiserror::Error;

#[derive(Error, Debug)]
enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Send(std::sync::mpsc::SendError<String>),
    #[error("watcher {0 }error: {1}")]
    Watcher(String, notify::Error),
    #[error("command {0} failed with exit code {1:?}: {2}")]
    Command(String, Option<i32>, String),
}

pub fn run(settings: Settings) {
    info!("starting log-watchdog");
    let (tx, rx) = std::sync::mpsc::channel::<()>();

    for watchdog in settings.into_watchdogs() {
        let tx = tx.clone();
        std::thread::spawn(move || match watch(watchdog, tx) {
            Ok(name) => info!("watchdog::{name}: completed"),
            Err(e) => {
                error!("watchdog failed: {e}");
                std::process::exit(1);
            }
        });
    }

    // drop the last one so that we know when to exit
    drop(tx);

    for _ in rx.iter() {}
}

fn watch(watchdog: Watchdog, _: Sender<()>) -> Result<String, Error> {
    let watchdog_name = watchdog.name.clone();
    info!("watchdog::{watchdog_name}: starting");

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())
        .map_err(|e| Error::Watcher(watchdog.name.clone(), e))?;

    watcher
        .watch(&watchdog.log_file, notify::RecursiveMode::NonRecursive)
        .map_err(|e| Error::Watcher(watchdog.name.clone(), e))?;

    info!(
        "watchdog::{watchdog_name}: watching {:?}",
        &watchdog.log_file.as_os_str()
    );
    let mut log_file = File::open(&watchdog.log_file).unwrap();
    let mut position = log_file.seek(SeekFrom::End(0)).unwrap();

    /*
        The linesender is used whenever there's a Modify event on our log file
        The linereceiver parses each line and acts on it. It handles eventual
        debouncing or one-shot functionality.

        To track whether the current thread should complete (outside of errors, ofc),
        we give the linereceiver a close flag -- because the linereceiver knows
        when to exit.
    */
    let (linesender, linereceiver) = std::sync::mpsc::channel::<String>();
    let (close_flag, close_receiver) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(
        move || match match_log_entries(watchdog, linereceiver, close_flag) {
            Ok(name) => info!("watchdog::{name}: match_log_entries completed"),
            Err(e) => error!("match_log_entries failed: {e}"),
        },
    );

    for res in rx {
        if is_closed(&close_receiver) {
            break;
        }
        match res {
            Ok(event) => match event.kind {
                notify::EventKind::Modify(_) => {
                    read_new_lines(&mut log_file, &mut position, linesender.clone())?;
                }
                notify::EventKind::Any
                | notify::EventKind::Access(_)
                | notify::EventKind::Create(_)
                | notify::EventKind::Remove(_)
                | notify::EventKind::Other => (), // do nothing on these events for now,
            },
            Err(e) => {
                return Err(Error::Watcher(watchdog_name, e));
            }
        }
    }

    Ok(watchdog_name)
}

fn match_log_entries(
    watchdog: Watchdog,
    linereceiver: Receiver<String>,
    _close_flag: Sender<()>,
) -> Result<String, Error> {
    let mut last_match = Instant::now();
    let debounce_duration = Duration::from_millis(watchdog.debounce);

    let mut out_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&watchdog.output_file)?;

    for line in linereceiver.iter() {
        if last_match.elapsed() >= debounce_duration {
            last_match = Instant::now();
            if watchdog.regex.is_match(&line) {
                execute_commands(&watchdog.commands, &mut out_file)?;

                if watchdog.oneshot {
                    break;
                }
            }
        }
    }

    Ok(watchdog.name)
}

fn execute_commands(commands: &[settings::Command], out_file: &mut File) -> Result<(), Error> {
    for command in commands {
        let output = Command::new(&command.name).args(&command.args).output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Command(
                command.name.clone(),
                output.status.code(),
                error.to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        writeln!(out_file, "{}", stdout)?;
    }

    Ok(())
}

fn is_closed(chan: &Receiver<()>) -> bool {
    match chan.try_recv() {
        Ok(_) => false,
        Err(TryRecvError::Disconnected) => true,
        Err(TryRecvError::Empty) => false,
    }
}

fn read_new_lines(file: &mut File, position: &mut u64, tx: Sender<String>) -> Result<(), Error> {
    let mut reader = BufReader::new(file);

    reader.seek(SeekFrom::Start(*position))?;

    for line in reader.lines() {
        let line = line.map_err(Error::Io)?;
        *position += line.len() as u64 + 1;
        tx.send(line).map_err(Error::Send)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    fn test_read_new_lines_from_0() {
        let dir = tempdir::TempDir::new("test_read_new_lines").unwrap();
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(dir.path().join("test_read_new_lines.txt"))
            .unwrap();

        writeln!(file, "Hello, world!").unwrap();
        writeln!(file, "Goodbye, world!").unwrap();

        let mut position = 0;
        file.seek(SeekFrom::Start(position)).unwrap();

        let expected_lines = vec!["Hello, world!", "Goodbye, world!"];
        let expected_position = file.seek(SeekFrom::End(0)).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        read_new_lines(&mut file, &mut position, tx).unwrap();

        let actual_lines = rx.iter().collect::<Vec<String>>();

        assert_eq!(actual_lines, expected_lines);
        assert_eq!(position, expected_position);
    }

    #[test]
    fn test_read_new_lines_from_position() {
        let dir = tempdir::TempDir::new("test_read_new_lines").unwrap();
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(dir.path().join("test_read_new_lines.txt"))
            .unwrap();

        writeln!(file, "Hello, world!").unwrap();

        let mut position = file.seek(SeekFrom::End(0)).unwrap();

        writeln!(file, "Goodbye, world!").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();

        let expected_lines = vec!["Goodbye, world!"];
        let expected_position = file.seek(SeekFrom::End(0)).unwrap();

        read_new_lines(&mut file, &mut position, tx).unwrap();
        let actual_lines = rx.iter().collect::<Vec<String>>();

        assert_eq!(actual_lines, expected_lines);
        assert_eq!(position, expected_position);
    }
}
