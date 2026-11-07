//! Output in the house style: lowercase, square brackets, the accent for what
//! matters, no emoji. Colours turn off when stdout isn't a terminal.

use owo_colors::{OwoColorize, Stream::Stderr, Stream::Stdout};

/// A section heading: `── updates ──`
pub fn heading(s: &str) {
    println!("{}", format!("── {s} ──").if_supports_color(Stdout, |t| t.bold()));
}

/// `[ ok ] text`
pub fn ok(s: &str) {
    println!("{} {s}", "[ ok ]".if_supports_color(Stdout, |t| t.green()));
}

/// `[ warn ] text`
pub fn warn(s: &str) {
    println!("{} {s}", "[ warn ]".if_supports_color(Stdout, |t| t.yellow()));
}

/// `[ fail ] text` (to stderr)
pub fn fail(s: &str) {
    eprintln!("{} {s}", "[ fail ]".if_supports_color(Stderr, |t| t.red()));
}

/// `[ info ] text`
pub fn info(s: &str) {
    println!("{} {s}", "[ .. ]".if_supports_color(Stdout, |t| t.cyan()));
}

/// Muted text (hints, commands being run).
pub fn dim(s: &str) {
    eprintln!("{}", s.if_supports_color(Stderr, |t| t.dimmed()));
}

/// `key   value` rows, aligned.
pub fn kv(rows: &[(&str, String)]) {
    let w = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (k, v) in rows {
        println!("  {:w$}  {v}", k.if_supports_color(Stdout, |t| t.dimmed()), w = w);
    }
}

/// Ask yes/no on the terminal; default no.
pub fn confirm(q: &str) -> bool {
    use std::io::Write;
    print!("{q} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).is_ok() && matches!(s.trim(), "y" | "Y" | "yes")
}
