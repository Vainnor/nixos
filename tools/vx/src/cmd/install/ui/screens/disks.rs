//! 3. system drive and 4. data drives.

use super::super::app::{Act, App, Field, Use, short};
use super::super::form::{Row, button_row, input_row, label, pad, radio, toggle_row, truncate};
use super::super::super::plan::DiskKind;
use super::super::super::probe;
use super::super::validate::human_bytes;
use super::{kind_label, kind_style, para};
use ratatui::text::{Line, Span};

/// Column widths for a disk line; the model column takes what's left.
fn cols(w: usize) -> (usize, usize, usize, usize, usize) {
    let (name, size, bus, kind) = (10, 9, 6, 20);
    let model = w.saturating_sub(4 + name + size + bus + kind + 1).clamp(8, 32);
    (name, size, model, bus, kind)
}

pub fn system_rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let (cn, cs, cm, cb, ck) = cols(w);
    let mut r = vec![
        Row::info(Line::from(Span::styled(
            format!("    {}{}{}{}{}", pad("disk", cn), pad("size", cs), pad("model", cm + 1), pad("bus", cb), "on it"),
            t.muted(),
        ))),
    ];
    for (i, d) in app.disks.iter().enumerate() {
        let sel = app.system == Some(i);
        let line = Line::from(vec![
            Span::styled(format!("{} ", if d.kind == DiskKind::Installer { "   " } else { radio(sel) }), t.accent()),
            Span::styled(
                format!("{}{}{} {}", pad(short(&d.path), cn), pad(&human_bytes(d.size), cs), pad(&d.model, cm), pad(&d.transport, cb)),
                if d.kind == DiskKind::Installer { t.muted() } else { t.text() },
            ),
            Span::styled(truncate(kind_label(d), ck), kind_style(t, d)),
        ]);
        r.push(if d.kind == DiskKind::Installer { Row::info(line) } else { Row::field(Field::Disk(i), line) });
    }
    if app.disks.is_empty() {
        r.push(Row::colored(t.red, "  no disks found"));
    }
    r.push(Row::blank());
    // Details of the disk under the cursor.
    if let Some(Field::Disk(i)) = app.focus
        && let Some(d) = app.disks.get(i)
    {
        r.push(Row::info(Line::from(vec![label(t, "id"), Span::styled(d.by_id.clone().unwrap_or_else(|| "(none: uses the kernel name)".into()), t.text())])));
        for p in d.partitions.iter().take(5) {
            let what = [p.fstype.as_deref(), p.label.as_deref().or(p.partlabel.as_deref())].into_iter().flatten().collect::<Vec<_>>().join("  ");
            r.push(Row::info(Line::from(vec![
                Span::raw(" ".repeat(18)),
                Span::styled(format!("{} {} ", pad(short(&p.path), 12), pad(&human_bytes(p.size), 8)), t.muted()),
                Span::styled(truncate(&what, w.saturating_sub(40)), t.muted()),
            ])));
        }
        if d.partitions.is_empty() {
            r.push(Row::muted(t, format!("{:18}no partitions", "")));
        }
        r.push(Row::blank());
    }
    r.extend(para(t, "everything on the disk you pick is erased. windows disks ask you to type windows first.", w));
    r.push(Row::blank());
    r.push(button_row(t, Field::Next, "next"));
    r
}

fn use_label(u: Use) -> &'static str {
    match u {
        Use::NotUsed => "not used",
        Use::Format => "format as data",
        Use::Mount => "mount as is",
    }
}

/// Why "mount as is" isn't offered, if it isn't.
fn mount_note(app: &App, i: usize) -> Option<&'static str> {
    let d = &app.disks[i];
    if probe::mountable_partition(d).is_some() {
        return None;
    }
    Some(match d.kind {
        DiskKind::Windows { bitlocker: true, .. } => "bitlocker can't be mounted here",
        DiskKind::NixOs => "encrypted: format only",
        DiskKind::Empty => "empty: format only",
        _ => "nothing to mount",
    })
}

pub fn data_rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let mut r = vec![];
    let disks = app.data_disks();
    if disks.is_empty() {
        r.push(Row::muted(t, "no other disks: just the system disk."));
    } else {
        r.extend(para(t, "the other disks: leave them alone, format one as a data drive, or mount it as it is.", w));
    }
    let model_w = w.saturating_sub(10 + 9 + 22 + 18).clamp(6, 28);
    for i in disks {
        let (d, row) = (&app.disks[i], &app.data_rows[i]);
        r.push(Row::blank());
        let mut spans = vec![
            Span::styled(format!("{}{}{} ", pad(short(&d.path), 10), pad(&human_bytes(d.size), 9), pad(&d.model, model_w)), t.text()),
            Span::styled(pad(kind_label(d), 21), kind_style(t, d)),
            Span::styled(format!("< {} >", use_label(row.use_)), if row.use_ == Use::Format { t.fg(t.red) } else { t.text() }),
        ];
        if row.use_ == Use::NotUsed
            && let Some(n) = mount_note(app, i)
        {
            spans.push(Span::styled(format!("  {n}"), t.muted()));
        }
        r.push(Row::field(Field::DataUse(i), Line::from(spans)));
        let ind = |l: &str| format!("  {l}");
        match row.use_ {
            Use::NotUsed => {}
            Use::Format => {
                r.push(input_row(t, Field::DataName(i), &ind("name"), &row.name, app.focus == Some(Field::DataName(i)), None));
                r.push(input_row(t, Field::DataMount(i), &ind("mount at"), &row.mount, app.focus == Some(Field::DataMount(i)), None));
                r.push(toggle_row(t, Field::DataEncrypt(i), &ind("encrypt"), app.data_encrypt(i), "unlocked at boot by a key file"));
                r.push(Row::colored(t.red, format!("{:18}erases everything on it: one btrfs volume", "")));
            }
            Use::Mount => {
                if let Some(p) = probe::mountable_partition(d) {
                    r.push(Row::info(Line::from(vec![
                        label(t, &ind("partition")),
                        Span::styled(
                            format!("{}  {}  {}", short(&p.path), probe::mount_fs_type(p.fstype.as_deref().unwrap_or("")), human_bytes(p.size)),
                            t.text(),
                        ),
                    ])));
                }
                r.push(input_row(t, Field::DataName(i), &ind("name"), &row.name, app.focus == Some(Field::DataName(i)), None));
                r.push(input_row(t, Field::DataMount(i), &ind("mount at"), &row.mount, app.focus == Some(Field::DataMount(i)), None));
                let note = if matches!(d.kind, DiskKind::Windows { system: true, .. }) { "windows c: safest read-only" } else { "" };
                r.push(toggle_row(t, Field::DataReadOnly(i), &ind("read-only"), row.read_only, note));
            }
        }
    }
    r.push(Row::blank());
    r.push(button_row(t, Field::Next, "next"));
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    match f {
        Field::Disk(i) if matches!(a, Act::Select | Act::Toggle) => app.select_system(i),
        Field::DataUse(i) => {
            let order = [Use::NotUsed, Use::Format, Use::Mount];
            let mountable = probe::mountable_partition(&app.disks[i]).is_some();
            let avail: Vec<Use> = order.into_iter().filter(|u| *u != Use::Mount || mountable).collect();
            let cur = avail.iter().position(|u| *u == app.data_rows[i].use_).unwrap_or(0);
            let n = avail.len();
            let next = match a {
                Act::Left => avail[(cur + n - 1) % n],
                _ => avail[(cur + 1) % n],
            };
            app.set_use(i, next);
        }
        Field::DataEncrypt(i) if matches!(a, Act::Select | Act::Toggle) => {
            let v = app.data_encrypt(i);
            app.data_rows[i].encrypt = Some(!v);
        }
        Field::DataReadOnly(i) if matches!(a, Act::Select | Act::Toggle) => {
            app.data_rows[i].read_only = !app.data_rows[i].read_only;
        }
        _ => {}
    }
}
