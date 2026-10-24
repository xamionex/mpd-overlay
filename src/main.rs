mod color;
mod config;
mod mpd_watcher;
mod overlay;
mod render;
mod song_info;
mod tui;

use crate::config::Config;
use std::{cell::RefCell, fs, rc::Rc, sync::mpsc, time::SystemTime};

#[derive(Default)]
struct CliOptions {
    help: bool,
    config_tui: bool,
    enable_service: bool,
    disable_service: bool,
}

fn parse_args(args: &[String]) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => options.help = true,
            "-c" | "--config" => options.config_tui = true,
            "-e" | "--enable" => options.enable_service = true,
            "-d" | "--disable" => options.disable_service = true,
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    let count = [
        options.help,
        options.config_tui,
        options.enable_service,
        options.disable_service,
    ]
    .into_iter()
    .filter(|value| *value)
    .count();
    if count > 1 {
        return Err("only one of -h, -c, -e, or -d may be used at a time".to_string());
    }

    Ok(options)
}

fn print_help() {
    println!(
        "mpd-overlay

USAGE:
  mpd-overlay [OPTIONS]

OPTIONS:
  -h, --help     Print this help message and exit
  -c, --config   Open the interactive configuration editor
  -e, --enable   Enable and start the mpd-overlay systemd user service
  -d, --disable  Disable and stop the mpd-overlay systemd user service

With no options, mpd-overlay runs as a daemon showing the music overlay.
Configuration is read from ~/.config/mpd-overlay/config.toml and an initial
default is written there when it is missing. Changes to that file are detected
while running and applied without restarting the daemon."
    );
}

fn systemd_service_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let config_dir =
        dirs::config_dir().ok_or("could not resolve the user configuration directory")?;
    Ok(config_dir.join("systemd/user/mpd-overlay.service"))
}

fn write_service_unit() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = systemd_service_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let executable = std::env::current_exe()?;
    let unit = format!(
        "[Unit]
Description=MPD Overlay
Documentation=man:mpd-overlay(1)
After=mpd.service
Wants=mpd.service

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
",
        executable.display()
    );
    std::fs::write(&path, unit)?;
    Ok(path)
}

fn run_systemctl(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()?;
    if !status.success() {
        return Err(format!("systemctl --user {} failed", args.join(" ")).into());
    }
    Ok(())
}

fn enable_service() -> Result<(), Box<dyn std::error::Error>> {
    write_service_unit()?;
    run_systemctl(&["daemon-reload"])?;
    run_systemctl(&["disable", "--now", "mpd-overlay.service"])?;
    run_systemctl(&["enable", "--now", "mpd-overlay.service"])?;
    println!("Enabled and started mpd-overlay.service");
    Ok(())
}

fn disable_service() -> Result<(), Box<dyn std::error::Error>> {
    let _ = run_systemctl(&["disable", "--now", "mpd-overlay.service"]);
    let _ = run_systemctl(&["daemon-reload"]);
    println!("Disabled mpd-overlay.service");
    Ok(())
}

struct Runtime {
    config: Config,
    last_modified: Option<SystemTime>,
    watcher: Option<mpd_watcher::WatcherHandle>,
    receiver: Option<mpsc::Receiver<mpd_watcher::Update>>,
}

fn config_modified(path: &std::path::Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn same_mpd_config(a: &crate::config::MpdConfig, b: &crate::config::MpdConfig) -> bool {
    a.music_dir == b.music_dir && a.ip == b.ip && a.port == b.port && a.use_cover == b.use_cover
}

fn run_daemon() {
    let config = match Config::from_file() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Error loading configuration: {error}");
            eprintln!(
                "Create {}, for example from doc/example_config.toml.",
                Config::config_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| "~/.config/mpd-overlay/config.toml".to_string())
            );
            return;
        }
    };

    if std::env::var_os("DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }

    if let Err(error) = gtk::init() {
        eprintln!("Could not initialize GTK: {error}");
        return;
    }

    let overlay = overlay::Overlay::new(config.overlay.clone());
    let (sender, receiver) = mpsc::channel();
    let watcher = mpd_watcher::spawn(config.mpd.clone(), sender);

    let config_path = match Config::config_path() {
        Ok(path) => path,
        Err(_) => std::path::PathBuf::from("~/.config/mpd-overlay/config.toml"),
    };

    let runtime = Rc::new(RefCell::new(Runtime {
        config,
        last_modified: config_modified(&config_path),
        watcher: Some(watcher),
        receiver: Some(receiver),
    }));

    gtk::glib::timeout_add_local(std::time::Duration::from_millis(20), {
        let overlay = overlay.clone();
        let runtime = Rc::clone(&runtime);
        move || {
            loop {
                let update = {
                    let mut runtime = runtime.borrow_mut();
                    runtime
                        .receiver
                        .as_mut()
                        .and_then(|receiver| receiver.try_recv().ok())
                };
                match update {
                    Some(update) => overlay.handle(update),
                    None => break,
                }
            }
            gtk::glib::ControlFlow::Continue
        }
    });

    gtk::glib::timeout_add_local(std::time::Duration::from_millis(500), {
        let overlay = overlay.clone();
        let runtime = Rc::clone(&runtime);
        let config_path = config_path.clone();
        move || {
            let modified = match config_modified(&config_path) {
                Some(modified) => modified,
                None => return gtk::glib::ControlFlow::Continue,
            };

            let needs_reload = {
                let runtime = runtime.borrow();
                runtime.last_modified != Some(modified)
            };

            if needs_reload {
                match Config::from_file() {
                    Ok(new_config) => {
                        let mpd_changed = {
                            let runtime = runtime.borrow();
                            !same_mpd_config(&runtime.config.mpd, &new_config.mpd)
                        };
                        let overlay_changed = {
                            let runtime = runtime.borrow();
                            runtime.config.overlay != new_config.overlay
                        };

                        {
                            let mut runtime = runtime.borrow_mut();
                            runtime.config = new_config.clone();
                            runtime.last_modified = Some(modified);
                        }

                        if overlay_changed {
                            overlay.update_config(new_config.overlay.clone());
                        }

                        if mpd_changed {
                            let old_watcher = runtime.borrow_mut().watcher.take();
                            let old_receiver = runtime.borrow_mut().receiver.take();

                            let (sender, receiver) = mpsc::channel();
                            let handle = mpd_watcher::spawn(new_config.mpd.clone(), sender);

                            {
                                let mut runtime = runtime.borrow_mut();
                                runtime.watcher = Some(handle);
                                runtime.receiver = Some(receiver);
                            }

                            if let Some(old) = old_watcher {
                                old.stop();
                            }
                            let _ = old_receiver;
                        }
                    }
                    Err(error) => {
                        eprintln!("Could not reload configuration: {error}");
                    }
                }
            }

            gtk::glib::ControlFlow::Continue
        }
    });

    gtk::main();
}

fn main() {
    let options = match parse_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            print_help();
            std::process::exit(2);
        }
    };

    if options.help {
        print_help();
        return;
    }

    if options.config_tui {
        if let Err(error) = tui::run() {
            eprintln!("Error opening configuration editor: {error}");
            std::process::exit(1);
        }
        return;
    }

    if options.enable_service {
        if let Err(error) = enable_service() {
            eprintln!("Could not enable service: {error}");
            std::process::exit(1);
        }
        return;
    }

    if options.disable_service {
        if let Err(error) = disable_service() {
            eprintln!("Could not disable service: {error}");
            std::process::exit(1);
        }
        return;
    }

    run_daemon();
}
