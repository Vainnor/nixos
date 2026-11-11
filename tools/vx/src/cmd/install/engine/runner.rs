//! The one place the engine runs programs and touches files. In
//! [`Mode::DryRun`] it only logs what it would do and hands back the canned
//! output the caller set with [`Cmd::fake`].
//!
//! Secrets never go in the log, and only in argv when a program takes them no
//! other way ([`Cmd::secret_arg`], shown as `***`): they go in through stdin
//! ([`Cmd::stdin`]), the environment ([`Cmd::env`], never logged) or files
//! written with [`Runner::write_secret`].

use super::super::plan::Event;
use super::Mode;
use anyhow::{Context, Result, bail};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

/// How many lines of output an error message shows.
const TAIL: usize = 20;
/// Progress lines (ending in \r, e.g. rsync --info=progress2) are forwarded at most this often.
const PROGRESS_EVERY: Duration = Duration::from_secs(2);

/// One external command.
#[derive(Clone, Debug, Default)]
pub struct Cmd {
    pub prog: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Fed to the program's stdin (then closed). Never logged.
    stdin: Option<Vec<u8>>,
    /// What DryRun returns as stdout.
    fake: String,
    /// Don't forward stdout to the log (still returned), e.g. a password hash.
    quiet: bool,
    /// Indexes into `args` shown as `***` in the log.
    hidden: Vec<usize>,
    /// Turns each stdout line into the log line (None: not logged), e.g.
    /// restic's JSON progress into "restored 3.1 of 80 GB".
    format: Option<fn(&str) -> Option<String>>,
}

impl Cmd {
    pub fn new(prog: &str) -> Self {
        Cmd { prog: prog.into(), ..Default::default() }
    }

    pub fn arg(mut self, a: impl AsRef<str>) -> Self {
        self.args.push(a.as_ref().to_string());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.args.extend(it.into_iter().map(|s| s.as_ref().to_string()));
        self
    }

    /// An argument that is a secret: in argv (the program takes it no other
    /// way), `***` in the log.
    pub fn secret_arg(mut self, a: impl AsRef<str>) -> Self {
        self.hidden.push(self.args.len());
        self.args.push(a.as_ref().to_string());
        self
    }

    pub fn format(mut self, f: fn(&str) -> Option<String>) -> Self {
        self.format = Some(f);
        self
    }

    pub fn env(mut self, k: &str, v: &str) -> Self {
        self.env.push((k.into(), v.into()));
        self
    }

    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(bytes.into());
        self
    }

    pub fn fake(mut self, out: impl Into<String>) -> Self {
        self.fake = out.into();
        self
    }

    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// `prog arg ...`, quoted like a shell would need. What the log shows after "$ ".
    pub fn line(&self) -> String {
        let mut s = shell_quote(&self.prog);
        for (i, a) in self.args.iter().enumerate() {
            s.push(' ');
            s.push_str(&if self.hidden.contains(&i) { "***".to_string() } else { shell_quote(a) });
        }
        if self.stdin.is_some() {
            s.push_str(" < (stdin)");
        }
        s
    }

    #[cfg(test)]
    pub fn stdin_bytes(&self) -> Option<&[u8]> {
        self.stdin.as_deref()
    }
}

/// Quote for sh only when needed (logs, and `nixos-enter -c`).
pub fn shell_quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%#^".contains(c));
    if plain { s.to_string() } else { format!("'{}'", s.replace('\'', r"'\''")) }
}

/// Runs [`Cmd`]s and file operations, honouring the [`Mode`], and forwards
/// output to the UI as [`Event::Log`] lines.
pub struct Runner {
    pub mode: Mode,
    tx: Option<Sender<Event>>,
}

impl Runner {
    pub fn new(mode: Mode, tx: Option<Sender<Event>>) -> Self {
        Runner { mode, tx }
    }

    pub fn dry(&self) -> bool {
        self.mode == Mode::DryRun
    }

    pub fn log(&self, s: impl Into<String>) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Event::Log(s.into()));
        }
    }

    pub fn send(&self, e: Event) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(e);
        }
    }

    /// Run it; stdout on success, an error with the last lines of output otherwise.
    pub fn run(&self, c: Cmd) -> Result<String> {
        let (ok, out, tail) = self.exec(&c, c.fake.clone(), true)?;
        if !ok {
            bail!("{} failed:\n{}", c.prog, tail);
        }
        Ok(out)
    }

    /// Run it; Ok(true) when it exited 0. DryRun: `dry` is the pretend answer.
    pub fn check(&self, c: Cmd, dry: bool) -> Result<bool> {
        if self.dry() {
            self.log(format!("$ {}", c.line()));
            return Ok(dry);
        }
        Ok(self.exec(&c, String::new(), true)?.0)
    }

    /// Like [`run`](Self::run) but output is only collected, not logged
    /// (queries such as lsblk/findmnt whose output means nothing to the user).
    pub fn query(&self, c: Cmd) -> Result<String> {
        let (ok, out, tail) = self.exec(&c, c.fake.clone(), false)?;
        if !ok {
            bail!("{} failed:\n{}", c.prog, tail);
        }
        Ok(out)
    }

    /// Run it, logging the command but not its output: (exited 0, stdout, last
    /// lines of output). For callers that tell failures apart themselves.
    pub fn output(&self, c: &Cmd) -> Result<(bool, String, String)> {
        if !self.dry() {
            self.log(format!("$ {}", c.line()));
        }
        self.exec(c, c.fake.clone(), false)
    }

    fn exec(&self, c: &Cmd, fake: String, forward: bool) -> Result<(bool, String, String)> {
        if forward || self.dry() {
            self.log(format!("$ {}", c.line()));
        }
        if self.dry() {
            return Ok((true, fake, String::new()));
        }
        let mut cmd = Command::new(&c.prog);
        cmd.args(&c.args).current_dir("/").stdout(Stdio::piped()).stderr(Stdio::piped());
        for (k, v) in &c.env {
            cmd.env(k, v);
        }
        cmd.stdin(if c.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        let mut child = cmd.spawn().with_context(|| format!("could not start {} (is it installed?)", c.prog))?;

        let writer = match (child.stdin.take(), c.stdin.clone()) {
            (Some(mut pipe), Some(bytes)) => Some(std::thread::spawn(move || {
                // A program that exits without reading all of it is fine.
                let _ = pipe.write_all(&bytes);
            })),
            _ => None,
        };

        let (ltx, lrx) = mpsc::channel::<(bool, String, bool)>();
        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            let t = ltx.clone();
            readers.push(std::thread::spawn(move || read_lines(out, true, t)));
        }
        if let Some(err) = child.stderr.take() {
            let t = ltx.clone();
            readers.push(std::thread::spawn(move || read_lines(err, false, t)));
        }
        drop(ltx);

        let mut stdout = String::new();
        let mut tail: VecDeque<String> = VecDeque::new();
        let mut last_progress: Option<Instant> = None;
        for (is_out, line, progress) in lrx {
            let (line, progress) = match c.format {
                Some(f) if is_out => {
                    stdout.push_str(&line);
                    stdout.push('\n');
                    match f(&line) {
                        // Not rate-limited here: the program paces itself, and the last line counts.
                        Some(l) => (l, false),
                        None => continue,
                    }
                }
                _ => (line, progress),
            };
            if is_out && c.format.is_none() {
                if !progress {
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                if c.quiet {
                    continue;
                }
            }
            if progress {
                if last_progress.is_some_and(|t| t.elapsed() < PROGRESS_EVERY) {
                    continue;
                }
                last_progress = Some(Instant::now());
            } else {
                tail.push_back(line.clone());
                if tail.len() > TAIL {
                    tail.pop_front();
                }
            }
            if forward && !line.trim().is_empty() {
                self.log(line);
            }
        }
        for r in readers {
            let _ = r.join();
        }
        if let Some(w) = writer {
            let _ = w.join();
        }
        let status = child.wait().with_context(|| format!("{} did not finish", c.prog))?;
        let tail = Vec::from(tail).join("\n");
        let tail = if tail.trim().is_empty() { format!("(no output; {status})") } else { tail };
        Ok((status.success(), stdout, tail))
    }

    // ------------------------------------------------------------ files

    /// mkdir -p, then set the mode of the last component.
    pub fn mkdir(&self, p: &Path, mode: u32) -> Result<()> {
        self.log(format!("mkdir {} ({mode:o})", p.display()));
        if self.dry() {
            return Ok(());
        }
        std::fs::create_dir_all(p).with_context(|| format!("could not create {}", p.display()))?;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
            .with_context(|| format!("could not chmod {}", p.display()))
    }

    /// Write a file that holds no secret.
    pub fn write(&self, p: &Path, bytes: &[u8], mode: u32) -> Result<()> {
        self.log(format!("write {} ({mode:o})", p.display()));
        if self.dry() {
            return Ok(());
        }
        write_file(p, bytes, mode)
    }

    /// Write a secret: created with `mode` from the start (never world-readable), content never logged.
    pub fn write_secret(&self, p: &Path, bytes: &[u8], mode: u32) -> Result<()> {
        self.log(format!("write {} (secret, {mode:o})", p.display()));
        if self.dry() {
            return Ok(());
        }
        write_file(p, bytes, mode)
    }

    /// `n` random bytes from /dev/urandom into a secret file.
    pub fn write_random_key(&self, p: &Path, n: usize, mode: u32) -> Result<()> {
        self.log(format!("write {} ({n} random bytes, {mode:o})", p.display()));
        if self.dry() {
            return Ok(());
        }
        let mut key = vec![0u8; n];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut key))
            .context("could not read /dev/urandom")?;
        write_file(p, &key, mode)
    }

    pub fn copy_file(&self, from: &Path, to: &Path, mode: u32) -> Result<()> {
        self.log(format!("copy {} -> {} ({mode:o})", from.display(), to.display()));
        if self.dry() {
            return Ok(());
        }
        let bytes = std::fs::read(from).with_context(|| format!("could not read {}", from.display()))?;
        write_file(to, &bytes, mode)
    }

    pub fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.log(format!("move {} -> {}", from.display(), to.display()));
        if self.dry() {
            return Ok(());
        }
        std::fs::rename(from, to).with_context(|| format!("could not move {} to {}", from.display(), to.display()))
    }

    /// rm -rf; fine when it isn't there.
    pub fn remove(&self, p: &Path) -> Result<()> {
        self.log(format!("remove {}", p.display()));
        if self.dry() {
            return Ok(());
        }
        let r = match std::fs::symlink_metadata(p) {
            Err(_) => return Ok(()),
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
            Ok(_) => std::fs::remove_file(p),
        };
        r.with_context(|| format!("could not remove {}", p.display()))
    }

    /// Does `p` exist? DryRun: `dry`.
    pub fn exists(&self, p: &Path, dry: bool) -> bool {
        if self.dry() { dry } else { p.exists() }
    }

    /// Read a text file. DryRun: the file if it happens to exist, else `dry`.
    pub fn read(&self, p: &Path, dry: &str) -> Result<String> {
        if self.dry() {
            return Ok(std::fs::read_to_string(p).unwrap_or_else(|_| dry.to_string()));
        }
        std::fs::read_to_string(p).with_context(|| format!("could not read {}", p.display()))
    }
}

/// Create (or truncate) with the mode already set, so a secret is never briefly readable.
fn write_file(p: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(p)
        .with_context(|| format!("could not write {}", p.display()))?;
    // An existing file keeps its old mode with .mode(); set it explicitly.
    f.set_permissions(std::fs::Permissions::from_mode(mode))?;
    f.write_all(bytes).with_context(|| format!("could not write {}", p.display()))?;
    f.sync_all()?;
    Ok(())
}

/// Split a stream on \n and \r; a piece ended by \r is a progress line.
fn read_lines(mut r: impl Read, is_out: bool, tx: Sender<(bool, String, bool)>) {
    let mut buf = [0u8; 8192];
    let mut cur: Vec<u8> = Vec::new();
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\n' || b == b'\r' {
                let line = String::from_utf8_lossy(&cur).into_owned();
                cur.clear();
                if b == b'\r' && line.is_empty() {
                    continue; // the \r of a \r\n pair, or a bare redraw
                }
                if tx.send((is_out, line, b == b'\r')).is_err() {
                    return;
                }
            } else {
                cur.push(b);
            }
        }
    }
    if !cur.is_empty() {
        let _ = tx.send((is_out, String::from_utf8_lossy(&cur).into_owned(), false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("/dev/nvme0n1"), "/dev/nvme0n1");
        assert_eq!(shell_quote("nix-command flakes"), "'nix-command flakes'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn line_hides_stdin() {
        let c = Cmd::new("cryptsetup").args(["open", "--key-file", "-"]).stdin("hunter2");
        assert_eq!(c.line(), "cryptsetup open --key-file - < (stdin)");
        assert!(!c.line().contains("hunter2"));
    }

    #[test]
    fn secret_args_and_formatted_lines() {
        let c = Cmd::new("op").args(["account", "add", "--secret-key"]).secret_arg("A3-SECRET").arg("--raw");
        assert_eq!(c.line(), "op account add --secret-key *** --raw");
        assert_eq!(c.args[3], "A3-SECRET");
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(Mode::Real, Some(tx));
        let f: fn(&str) -> Option<String> = |l| l.strip_prefix("n=").map(|n| format!("got {n}"));
        let out = r.run(Cmd::new("/bin/sh").args(["-c", "echo n=1; echo noise; echo n=2"]).format(f)).unwrap();
        assert_eq!(out, "n=1\nnoise\nn=2\n", "stdout is returned as it was");
        drop(r);
        let logs: Vec<String> = rx.iter().filter_map(|e| if let Event::Log(s) = e { Some(s) } else { None }).collect();
        assert_eq!(logs[1..], ["got 1", "got 2"]);
    }

    #[test]
    fn dry_run_logs_and_fakes() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(Mode::DryRun, Some(tx));
        let out = r.run(Cmd::new("nix").arg("build").fake("/nix/store/x")).unwrap();
        assert_eq!(out, "/nix/store/x");
        drop(r);
        let logs: Vec<String> = rx.iter().filter_map(|e| if let Event::Log(s) = e { Some(s) } else { None }).collect();
        assert_eq!(logs, ["$ nix build"]);
    }

    #[test]
    fn real_run_collects_and_forwards() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(Mode::Real, Some(tx));
        let out = r.run(Cmd::new("/bin/sh").args(["-c", "cat; echo err >&2"]).stdin("a\nb\n")).unwrap();
        assert_eq!(out, "a\nb\n");
        let err = r.run(Cmd::new("/bin/sh").args(["-c", "echo boom; exit 3"])).unwrap_err().to_string();
        assert!(err.contains("boom"), "{err}");
        drop(r);
        let logs: Vec<String> = rx.iter().filter_map(|e| if let Event::Log(s) = e { Some(s) } else { None }).collect();
        assert!(logs.contains(&"a".to_string()) && logs.contains(&"err".to_string()), "{logs:?}");
    }

    #[test]
    fn quiet_keeps_stdout_out_of_the_log() {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(Mode::Real, Some(tx));
        let out = r.run(Cmd::new("/bin/sh").args(["-c", "echo secret-$((1+1))-hash"]).quiet()).unwrap();
        assert_eq!(out, "secret-2-hash\n");
        drop(r);
        assert!(rx.iter().all(|e| !matches!(e, Event::Log(s) if s.contains("secret-2-hash"))));
    }

    #[test]
    fn secret_files_are_private() {
        let d = std::env::temp_dir().join(format!("vx-runner-test-{}", std::process::id()));
        let p = d.join("k");
        let r = Runner::new(Mode::Real, None);
        r.write_secret(&p, b"pw", 0o600).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        r.write_random_key(&p, 64, 0o400).unwrap();
        assert_eq!(std::fs::read(&p).unwrap().len(), 64);
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o400);
        r.remove(&d).unwrap();
        assert!(!d.exists());
    }
}
