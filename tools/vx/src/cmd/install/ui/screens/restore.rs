//! The home backup step: put /home/<user> back from the homelab's restic
//! backup, so a reinstall comes back signed in to everything. Offered when
//! /home is made fresh. The login comes from the secrets (carried host key),
//! else 1Password, else typing it; then the backup is looked up once, so a
//! wrong login or an unreachable server shows up here, not halfway through
//! the install.

use super::super::app::{Act, App, Field, RsPhase};
use super::super::form::{Row, button_row, input_row, wrap};
use super::{mark, para};
use ratatui::text::Span;

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let f = app.focus;
    let rs = &app.rs;
    let user = app.user();
    let mut r = vec![Row::heading(t, "restore /home from the homelab backup")];
    r.push(Row::blank());
    r.extend(para(
        t,
        &format!(
            "/home/{user} comes back from the latest nightly backup (restic on the homelab): your files, browser \
             sessions and logins. without it, /home starts empty."
        ),
        w,
    ));
    r.push(Row::blank());
    if let Some(e) = &rs.error {
        for l in wrap(e, w.saturating_sub(1)) {
            r.push(Row::colored(t.red, l));
        }
        r.push(Row::blank());
    }
    let no = |r: &mut Vec<Row>| r.push(button_row(t, Field::RsNo, "no: start with an empty /home"));
    match &rs.phase {
        RsPhase::Ask => {
            if !app.nb.done() {
                for l in wrap("the backup is only reachable over netbird, and this usb isn't logged in to it yet.", w.saturating_sub(1)) {
                    r.push(Row::colored(t.yellow, l));
                }
                r.push(Row::blank());
                r.push(button_row(t, Field::RsNetbird, "log in to netbird first"));
            } else {
                r.push(button_row(t, Field::RsRestore, "restore from the backup"));
            }
            no(&mut r);
        }
        RsPhase::OnePassword => {
            let item = app.restore_item().unwrap_or_default();
            if let Some(n) = &rs.sops_note {
                r.push(Row::muted(t, format!("secrets: {n}")));
            }
            r.extend(para(t, &format!("the login is in 1password ({item}). sign in with your account; nothing of it stays on this usb."), w));
            r.push(Row::blank());
            r.push(input_row(t, Field::RsOpAddress, "sign-in address", &rs.op_address, f == Some(Field::RsOpAddress), None));
            r.push(input_row(t, Field::RsOpEmail, "email", &rs.op_email, f == Some(Field::RsOpEmail), None));
            r.push(input_row(t, Field::RsOpKey, "secret key", &rs.op_key, f == Some(Field::RsOpKey), None));
            r.push(input_row(t, Field::RsOpPw, "password", &rs.op_pw, f == Some(Field::RsOpPw), None));
            r.push(Row::blank());
            r.push(button_row(t, Field::RsSignIn, "sign in"));
            r.push(button_row(t, Field::RsType, "type the login instead"));
            no(&mut r);
        }
        RsPhase::Typed => {
            if let Some(n) = &rs.sops_note {
                r.push(Row::muted(t, format!("secrets: {n}")));
            }
            r.extend(para(t, "the backup's login: what's in restic/* of the secrets (or the 1password item).", w));
            r.push(Row::blank());
            let hint = Some(Span::styled("rest:https://<server>:8000/<host>/", t.muted()));
            r.push(input_row(t, Field::RsRepo, "repository", &rs.repo, f == Some(Field::RsRepo), hint.filter(|_| rs.repo.value.is_empty())));
            r.push(input_row(t, Field::RsPw, "its password", &rs.pw, f == Some(Field::RsPw), None));
            r.push(input_row(t, Field::RsUser, "server user", &rs.user, f == Some(Field::RsUser), None));
            r.push(input_row(t, Field::RsRestPw, "server password", &rs.rest_pw, f == Some(Field::RsRestPw), None));
            r.push(Row::blank());
            r.push(button_row(t, Field::RsCheck, "look for the backup"));
            no(&mut r);
        }
        RsPhase::Failed => {
            r.push(button_row(t, Field::RsRetry, "try again"));
            r.push(button_row(t, Field::RsType, "use another login"));
            r.push(button_row(t, Field::RsNo, "skip: restore later with vx restore --restic"));
        }
        RsPhase::Ready(latest) => {
            let src = rs.creds.as_ref().map(|(_, s)| s.label()).unwrap_or("");
            r.push(mark(t, 0, format!("latest backup: {latest}")));
            r.push(mark(t, 0, format!("login from {src}")));
            r.push(mark(t, 0, format!("goes into /home/{user} during the install, before the first boot")));
            r.push(Row::blank());
            r.extend(para(
                t,
                "if the network drops, the restore picks up where it stopped. if it can't finish, the install goes on \
                 and the summary says how to restore later.",
                w,
            ));
            r.push(Row::blank());
            r.push(button_row(t, Field::Next, "next"));
            r.push(button_row(t, Field::RsNo, "don't restore"));
        }
    }
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    if !matches!(a, Act::Select | Act::Toggle) {
        return;
    }
    match f {
        Field::RsRestore => app.rs_start(),
        Field::RsNetbird => app.rs_netbird(),
        Field::RsNo => app.rs_skip(),
        Field::RsSignIn => app.rs_signin(),
        Field::RsType => app.rs_type(),
        Field::RsCheck => app.rs_check_typed(),
        Field::RsRetry => app.rs_check(),
        _ => {}
    }
}
