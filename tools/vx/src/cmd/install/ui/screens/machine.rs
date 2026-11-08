//! The machine step: reinstall a host that's in the repo, or set up a new one (its
//! own folder, features, graphics card).

use super::super::super::engine::settings::host_name;
use super::super::super::plan::{Gpu, toggle_aspect};
use super::super::app::{Act, App, Field};
use super::super::form::{Row, button_row, check, choice_row, input_row, pad, radio};
use super::para;
use ratatui::text::{Line, Span};

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let mut r = vec![Row::heading(t, "which machine is this?"), Row::blank()];
    r.extend(para(
        t,
        "reinstall a machine that's already in the repo, or set up a new one: it gets its own folder (modules/hosts/<name>) with its own hardware and disks, so it never changes the others.",
        w,
    ));
    r.push(Row::blank());
    let hosts = app.hosts();
    let name_w = hosts.iter().map(|h| h.name.len()).max().unwrap_or(0).clamp(8, 16);
    for (i, h) in hosts.iter().enumerate() {
        let on = app.host_choice.as_deref() == Some(h.name.as_str());
        let features: Vec<&str> = h.aspects.iter().map(String::as_str).filter(|a| *a != "base").collect();
        let mut note = String::from("reinstall");
        if !features.is_empty() {
            note += &format!(" · {}", features.join(" "));
        }
        if h.gpu != Gpu::None {
            note += &format!(" · {}", h.gpu.name());
        }
        let prev = app.previous.is_some() && app.data.as_ref().is_some_and(|d| d.host == h.name);
        let room = w.saturating_sub(name_w + 8);
        r.push(Row::field(
            Field::Host(i),
            Line::from(vec![
                Span::styled(format!("{} ", radio(on)), t.accent()),
                Span::styled(pad(&h.name, name_w), t.text()),
                Span::styled(format!("  {}", super::super::form::truncate(&note, room)), t.muted()),
            ]),
        ));
        if prev && on {
            r.push(Row::muted(t, format!("    {:name_w$}  the nixos install found on this pc", "")));
        }
    }
    let new = app.host_choice.is_none();
    r.push(Row::field(
        Field::NewHost,
        Line::from(vec![Span::styled(format!("{} ", radio(new)), t.accent()), Span::styled("new machine", t.text())]),
    ));
    if new {
        r.push(Row::blank());
        let v = &app.new_name.value;
        let note = if v.is_empty() {
            Some(Span::styled("also its hostname", t.muted()))
        } else {
            match host_name(v, &app.aspect_names()) {
                Err(e) => Some(Span::styled(e, t.fg(t.red))),
                Ok(()) if hosts.iter().any(|h| h.name.eq_ignore_ascii_case(v)) => {
                    Some(Span::styled("already in the repo: pick it above", t.fg(t.red)))
                }
                Ok(()) => Some(Span::styled(format!("modules/hosts/{v}"), t.muted())),
            }
        };
        r.push(input_row(t, Field::HostName, "name", &app.new_name, app.focus == Some(Field::HostName), note));
        r.push(Row::blank());
        r.push(Row::info(Line::from(vec![Span::styled("features", t.bold()), Span::styled("  base is always on", t.muted())])));
        if let Some(d) = &app.data {
            let aw = d.aspects.iter().map(|a| a.name.len()).max().unwrap_or(0).clamp(8, 14);
            for (i, a) in d.aspects.iter().enumerate().filter(|(_, a)| a.name != "base") {
                let on = app.new_aspects.contains(&a.name);
                let room = w.saturating_sub(aw + 9);
                r.push(Row::field(
                    Field::Aspect(i),
                    Line::from(vec![
                        Span::styled(format!("  {} ", check(on)), t.accent()),
                        Span::styled(pad(&a.name, aw), t.text()),
                        Span::styled(format!("  {}", super::super::form::truncate(&a.description, room)), t.muted()),
                    ]),
                ));
            }
        }
        r.push(Row::blank());
        let found = app.machine.as_ref().map(|m| m.gpu).unwrap_or_default();
        let note = if app.new_gpu == found { "found on this pc".to_string() } else { format!("this pc has: {}", found.name()) };
        r.push(choice_row(t, Field::Gpu, "graphics", app.new_gpu.name(), &note));
    }
    r.push(Row::blank());
    r.push(button_row(t, Field::Next, "next"));
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    let flip = matches!(a, Act::Select | Act::Toggle);
    match f {
        Field::Host(i) if flip => {
            if let Some(h) = app.hosts().get(i) {
                app.host_choice = Some(h.name.clone());
            }
        }
        Field::NewHost if flip => {
            app.host_choice = None;
            app.focus = Some(Field::HostName);
        }
        Field::Aspect(i) if flip => {
            if let Some(d) = app.data.as_ref()
                && let Some(name) = d.aspects.get(i).map(|x| x.name.clone())
            {
                toggle_aspect(&d.aspects, &mut app.new_aspects, &name);
            }
        }
        Field::Gpu => {
            let n = Gpu::ALL.len();
            let i = Gpu::ALL.iter().position(|g| *g == app.new_gpu).unwrap_or(0);
            app.new_gpu = Gpu::ALL[if a == Act::Left { (i + n - 1) % n } else { (i + 1) % n }];
        }
        _ => {}
    }
}
