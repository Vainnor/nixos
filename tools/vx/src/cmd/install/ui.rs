//! The installer's screens (ratatui): welcome, previous install, system
//! drive, data drives, options, passwords, review, progress.
//!
//! - `app`      state, keys, plan building
//! - `screens`  what each screen shows and does
//! - `form`     rows, inputs, the focus fill
//! - `view`     the frame: steps, borders, help, modals
//! - `backend`  the real probe/engine, or `demo` fakes
//! - `theme`    colours from install-data (ANSI on the raw console)

mod app;
mod backend;
mod demo;
mod form;
mod screens;
mod theme;
mod validate;
mod view;

#[cfg(test)]
mod tests;

use super::InstallArgs;
use super::engine::Mode;
use super::plan::InstallData;
use anyhow::Result;
use app::{App, Exit, LOG_FILE, WORKER_PANIC};
use backend::Backend;
use ratatui::crossterm::event::{self, Event};
use std::time::Duration;

pub fn run(args: InstallArgs, repo: Option<std::path::PathBuf>) -> Result<()> {
    let backend = if args.demo {
        Backend::Demo
    } else if args.dry_run {
        Backend::Engine(Mode::DryRun)
    } else {
        Backend::Engine(Mode::Real)
    };
    let ansi = theme::console_is_ansi();
    let mut app = App::new(backend, args.offline, args.data.clone(), repo, ansi);
    // Colours early, from install-data when it's a file (on the ISO it is).
    if backend.is_demo() {
        app.theme = theme::Theme::pick(Some(&demo::data()), ansi);
    } else if let Some(d) = args.data.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str::<InstallData>(&s).ok()) {
        app.theme = theme::Theme::pick(Some(&d), ansi);
    }
    app.start();

    let mut term = ratatui::init();
    // ratatui's hook restores the terminal; a panic in a worker thread (the
    // engine) shouldn't tear the screen down, it's shown as the error instead.
    let restore_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() == Some("main") {
            restore_hook(info);
        } else if let Ok(mut p) = WORKER_PANIC.lock() {
            let msg = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a bug in the installer".into());
            *p = Some(format!("{msg} ({})", info.location().map(|l| l.to_string()).unwrap_or_default()));
        }
    }));
    let res = event_loop(&mut term, &mut app);
    ratatui::restore();
    if app.unlocked.is_some() && !app.installing() {
        backend.close_previous();
    }
    let exit = res?;
    if app.progress.is_some() {
        println!("log: {LOG_FILE}");
    }
    if exit == Exit::Reboot {
        if backend.is_real() {
            std::process::Command::new("systemctl").arg("reboot").status()?;
        } else {
            println!("({}: not rebooting)", backend.label());
        }
    }
    Ok(())
}

fn event_loop(term: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<Exit> {
    loop {
        app.poll();
        if let Some(e) = app.exit {
            return Ok(e);
        }
        term.draw(|f| view::draw(f, app))?;
        if event::poll(Duration::from_millis(60))?
            && let Event::Key(k) = event::read()?
        {
            app.key(k);
        }
    }
}
