//! Packages: try one without installing, search nixpkgs, and add or remove
//! packages in the vx-owned list module (modules/apps/extra.nix). Everything
//! uses the nixpkgs revision the flake pins (`--inputs-from <repo>`), so what
//! you search and try is what a switch would install.
//!
//! Also `vx ephemeral` (the justfile's impermanence helper).

mod extra;

use crate::ctx::{Ctx, Host};
use crate::ui;
use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use extra::List;
use owo_colors::{OwoColorize, Stream::Stdout};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Subcommand)]
pub enum Cmd {
    /// Run a package once without installing it (`vx try cowsay -- hi`)
    Try {
        /// nixpkgs attributes (or full flake refs like github:owner/repo#pkg)
        #[arg(required = true)]
        pkgs: Vec<String>,
        /// Command and arguments to run (default: the first package's main program)
        #[arg(last = true)]
        cmd: Vec<String>,
    },

    /// Search the nixpkgs the flake pins
    Search {
        /// Words to look for (all must match the name or description)
        #[arg(required = true)]
        query: Vec<String>,
        /// How many results to show
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
    },

    /// Install packages by adding them to modules/apps/extra.nix
    Add {
        /// nixpkgs attributes, like ripgrep or python3Packages.requests
        #[arg(required = true)]
        pkgs: Vec<String>,
        /// Which machine gets them
        #[arg(long, value_enum, default_value_t = Target::Both)]
        host: Target,
        /// System-wide (environment.systemPackages) instead of for your user
        #[arg(long)]
        system: bool,
        /// Switch afterwards without asking
        #[arg(short, long)]
        yes: bool,
        /// Only edit the file; don't offer to switch
        #[arg(long)]
        no_switch: bool,
    },

    /// Uninstall packages that `vx add` added
    #[command(alias = "rm")]
    Remove {
        /// nixpkgs attributes to take out
        #[arg(required = true)]
        pkgs: Vec<String>,
        /// Switch afterwards without asking
        #[arg(short, long)]
        yes: bool,
        /// Only edit the file; don't offer to switch
        #[arg(long)]
        no_switch: bool,
    },

    /// List the packages added with vx, by machine
    Pkgs,

    /// List files on the wiped root that will be lost at reboot (see also `vx persist-check`)
    #[command(hide = true)]
    Ephemeral,
}

/// Which machine `vx add` installs for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Target {
    /// The PC
    Arc,
    /// The Mac
    Mac,
    /// Both machines
    Both,
}

pub fn run(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Try { pkgs, cmd } => try_pkgs(ctx, &pkgs, &cmd),
        Cmd::Search { query, limit } => search(ctx, &query, limit),
        Cmd::Add {
            pkgs,
            host,
            system,
            yes,
            no_switch,
        } => add(ctx, &pkgs, host, system, yes, no_switch),
        Cmd::Remove { pkgs, yes, no_switch } => remove(ctx, &pkgs, yes, no_switch),
        Cmd::Pkgs => list(ctx),
        Cmd::Ephemeral => ephemeral(ctx),
    }
}

// ── nixpkgs at the pinned revision ─────────────────────────────────────

const LINUX: &str = "x86_64-linux";
const DARWIN: &str = "aarch64-darwin";

fn this_system(ctx: &Ctx) -> &'static str {
    match ctx.host {
        Host::NixOs => LINUX,
        Host::Mac => DARWIN,
    }
}

/// `nix ... --inputs-from <repo>`: `nixpkgs#x` resolves to the flake's locked nixpkgs.
fn nix_pinned(ctx: &Ctx, sub: &str) -> Result<Command> {
    let repo = ctx.repo()?;
    let mut c = ctx.nix();
    c.arg(sub).arg("--inputs-from").arg(repo);
    // Uncommitted edits (like the one `vx add` just made) are normal here.
    c.args(["--option", "warn-dirty", "false"]);
    Ok(c)
}

/// Run nix and return stdout. Its stderr (progress, nixpkgs' evaluation
/// warnings about aliases) is only shown when it fails.
fn quiet_output(ctx: &Ctx, c: &mut Command) -> Result<String> {
    if ctx.verbose {
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        ui::dim(&format!("$ {} {}", c.get_program().to_string_lossy(), args.join(" ")));
    }
    let out = c.stdin(Stdio::null()).output().context("could not start nix")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = err.lines().filter(|l| !l.starts_with("evaluation warning:")).collect();
        eprintln!("{}", lines[lines.len().saturating_sub(15)..].join("\n"));
        bail!("nix failed ({})", out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What vx needs to know about one attribute.
#[derive(Deserialize, Debug, Default)]
#[serde(default)]
struct Info {
    found: bool,
    /// It's a package (not a package set like python3Packages).
    drv: bool,
    /// Evaluating it threw (removed, renamed, insecure, ...).
    error: bool,
    unfree: bool,
    available: bool,
    broken: bool,
    main: String,
}

/// Look up attributes in the pinned nixpkgs for one system, in one evaluation.
fn lookup(ctx: &Ctx, system: &str, names: &[String]) -> Result<BTreeMap<String, Info>> {
    let list = names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(" ");
    let apply = format!(
        r#"ps: let
  lib = ps.lib;
  one = n: let
    path = lib.splitString "." n;
    r = builtins.tryEval (let
      p = lib.getAttrFromPath path ps;
      info = if !lib.isDerivation p then {{ found = true; drv = false; }} else {{
        found = true;
        drv = true;
        unfree = p.meta.unfree or false;
        available = lib.meta.availableOn ps.stdenv.hostPlatform p;
        broken = p.meta.broken or false;
        main = p.meta.mainProgram or (lib.getName p);
      }};
    in builtins.deepSeq info info);
  in if !lib.hasAttrByPath path ps then {{ found = false; }}
    else if r.success then r.value else {{ found = true; error = true; }};
in lib.genAttrs [ {list} ] one"#
    );
    let mut c = nix_pinned(ctx, "eval")?;
    c.arg(format!("nixpkgs#legacyPackages.{system}"))
        .args(["--json", "--apply", &apply]);
    let out = quiet_output(ctx, &mut c).context("could not read the pinned nixpkgs")?;
    serde_json::from_str(&out).context("unexpected output from nix eval")
}

/// The unfree package names (as `lib.getName` sees them) that building these
/// attributes needs on `system`, including unfree dependencies like
/// `discord-unwrapped`. Nixpkgs only names one refused package per
/// evaluation, so this allows it and tries again until everything evaluates.
fn unfree_closure(ctx: &Ctx, system: &str, names: &[String]) -> Result<Vec<String>> {
    let mut allowed: Vec<String> = Vec::new();
    if names.is_empty() {
        return Ok(allowed);
    }
    let attrs = names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(" ");
    let refused =
        regex::Regex::new(r"Refusing to evaluate package '([^']+)'[^\n]*because it (has an unfree license|[^\n]*)")
            .expect("valid regex");
    for _ in 0..20 {
        let allow = allowed.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(" ");
        let apply = format!(
            r#"ps: let
  lib = ps.lib;
  allowed = [ {allow} ];
  pk = import ps.path {{
    system = "{system}";
    config.allowUnfreePredicate = p: builtins.elem (lib.getName p) allowed;
  }};
in map (n: (lib.getAttrFromPath (lib.splitString "." n) pk).drvPath) [ {attrs} ]"#
        );
        let mut c = nix_pinned(ctx, "eval")?;
        c.arg(format!("nixpkgs#legacyPackages.{system}"))
            .args(["--json", "--apply", &apply]);
        if ctx.verbose {
            ui::dim(&format!(
                "$ nix eval nixpkgs#legacyPackages.{system} (checking {} for unfree parts)",
                names.join(" ")
            ));
        }
        let out = c.stdin(Stdio::null()).output().context("could not start nix")?;
        if out.status.success() {
            return Ok(allowed);
        }
        let err = String::from_utf8_lossy(&out.stderr);
        match refused.captures(&err) {
            Some(m) if m[2].starts_with("has an unfree license") => {
                let name = drv_name(&m[1]).to_string();
                if allowed.contains(&name) {
                    break;
                }
                allowed.push(name);
            }
            Some(m) => bail!("nixpkgs refuses {}: it {}", &m[1], &m[2]),
            None => {
                let last = err.lines().rev().find(|l| l.contains("error:")).unwrap_or("").trim();
                bail!("{} doesn't evaluate on {system}: {last}", names.join(" "));
            }
        }
    }
    bail!("couldn't work out which unfree packages {} needs", names.join(" "))
}

/// `discord-unwrapped-1.0.168` -> `discord-unwrapped` (like builtins.parseDrvName).
fn drv_name(full: &str) -> &str {
    let b = full.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'-' && b.get(i + 1).is_some_and(|c| !c.is_ascii_alphabetic()) {
            return &full[..i];
        }
    }
    full
}

// ── try ────────────────────────────────────────────────────────────────

fn installable(p: &str) -> String {
    if p.contains('#') {
        p.to_string()
    } else {
        format!("nixpkgs#{p}")
    }
}

fn try_pkgs(ctx: &Ctx, pkgs: &[String], cmd: &[String]) -> Result<()> {
    let plain: Vec<String> = pkgs.iter().filter(|p| !p.contains('#')).cloned().collect();
    for p in &plain {
        if !extra::valid_attr(p) {
            bail!("`{p}` isn't a nixpkgs attribute name");
        }
    }
    let info = if plain.is_empty() {
        BTreeMap::new()
    } else {
        lookup(ctx, this_system(ctx), &plain)?
    };
    let mut unfree = false;
    for p in &plain {
        let i = &info[p];
        check_found(p, i)?;
        if !i.available {
            bail!("{p} isn't available on {}", this_system(ctx));
        }
        unfree |= i.unfree;
    }

    let mut argv: Vec<String> = cmd.to_vec();
    // `vx try hello -- --greeting=hi`: flags go to the package's own program.
    if argv.is_empty() || argv[0].starts_with('-') {
        match pkgs[0].contains('#') {
            // A foreign flake ref: let `nix run` find its program.
            true if pkgs.len() == 1 => {
                let mut c = nix_pinned(ctx, "run")?;
                c.arg(&pkgs[0]).arg("--").args(&argv);
                return exec(ctx, c);
            }
            true => bail!("say which command to run: vx try {} -- <command>", pkgs.join(" ")),
            false => argv.insert(0, info[&pkgs[0]].main.clone()),
        }
    }

    let mut c = nix_pinned(ctx, "shell")?;
    if unfree {
        ui::warn("unfree package; allowing it for this run");
        c.env("NIXPKGS_ALLOW_UNFREE", "1").arg("--impure");
    }
    c.args(pkgs.iter().map(|p| installable(p))).arg("-c").args(&argv);
    exec(ctx, c)
}

/// Hand the terminal over to the command (its exit code becomes ours).
fn exec(ctx: &Ctx, mut c: Command) -> Result<()> {
    use std::os::unix::process::CommandExt;
    if ctx.verbose {
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        ui::dim(&format!("$ {} {}", c.get_program().to_string_lossy(), args.join(" ")));
    }
    // Stay in the caller's directory so `vx try jq -- jq . file.json` works.
    c.current_dir(std::env::current_dir()?);
    let err = c.exec();
    Err(err).context("could not start nix")
}

fn check_found(name: &str, i: &Info) -> Result<()> {
    if !i.found {
        bail!(
            "nixpkgs has no `{name}` (look with: vx search {})",
            name.rsplit('.').next().unwrap_or(name)
        );
    }
    if i.error {
        bail!("`{name}` is in nixpkgs but doesn't evaluate (removed, renamed or marked insecure)");
    }
    if !i.drv {
        bail!("`{name}` is a package set, not a package (pick one inside it, like {name}.<pkg>)");
    }
    Ok(())
}

// ── search ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Hit {
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
}

fn search(ctx: &Ctx, query: &[String], limit: usize) -> Result<()> {
    ui::dim("searching the pinned nixpkgs (the first search per update takes a while)");
    let mut c = nix_pinned(ctx, "search")?;
    c.arg("nixpkgs").args(query).arg("--json");
    let out = quiet_output(ctx, &mut c)?;
    let hits: BTreeMap<String, Hit> = serde_json::from_str(&out).context("unexpected output from nix search")?;
    let mut rows: Vec<(String, Hit)> = hits.into_iter().map(|(k, h)| (strip_system(&k), h)).collect();
    if rows.is_empty() {
        ui::info("nothing found");
        return Ok(());
    }
    let words: Vec<String> = query.iter().map(|w| w.to_lowercase()).collect();
    rows.sort_by_key(|(attr, _)| words.iter().map(|w| rank(attr, w)).min());

    let total = rows.len();
    let shown = &rows[..total.min(limit)];
    let w = shown
        .iter()
        .map(|(a, h)| a.len() + h.version.len() + 1)
        .max()
        .unwrap_or(0);
    let width = term_width();
    for (attr, h) in shown {
        let pad = w.saturating_sub(attr.len() + h.version.len() + 1);
        let room = width.saturating_sub(w + 4);
        println!(
            "{} {}{}  {}",
            attr.if_supports_color(Stdout, |t| t.bold()),
            h.version.if_supports_color(Stdout, |t| t.dimmed()),
            " ".repeat(pad),
            truncate(&h.description, room),
        );
    }
    if total > shown.len() {
        ui::dim(&format!("{} more (show them with -n {total})", total - shown.len()));
    }
    ui::dim("install with: vx add <name>   or run once: vx try <name>");
    Ok(())
}

/// `legacyPackages.x86_64-linux.foo.bar` -> `foo.bar`
fn strip_system(key: &str) -> String {
    let mut parts = key.splitn(3, '.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("legacyPackages" | "packages"), Some(_), Some(rest)) => rest.to_string(),
        _ => key.to_string(),
    }
}

/// Exact name first, then names that start with the query, then names that
/// contain it, then description-only matches; shorter and top-level first.
fn rank(attr: &str, q: &str) -> (u8, usize, usize, String) {
    let a = attr.to_lowercase();
    let leaf = a.rsplit('.').next().unwrap_or(&a).to_string();
    let tier = if a == q || leaf == q {
        0
    } else if a.starts_with(q) || leaf.starts_with(q) {
        1
    } else if a.contains(q) {
        2
    } else {
        3
    };
    (tier, a.matches('.').count(), a.len(), a)
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

fn term_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(100)
}

// ── add / remove / list ────────────────────────────────────────────────

fn extra_path(ctx: &Ctx) -> Result<std::path::PathBuf> {
    Ok(ctx.repo()?.join(extra::PATH))
}

fn read_extra(ctx: &Ctx) -> Result<Option<String>> {
    let p = extra_path(ctx)?;
    match std::fs::read_to_string(&p) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("could not read {}", p.display())),
    }
}

fn write_extra(ctx: &Ctx, text: &str, new: bool) -> Result<()> {
    let p = extra_path(ctx)?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&p, text).with_context(|| format!("could not write {}", p.display()))?;
    if new {
        // Flakes only see files git knows about.
        let mut c = ctx.cmd("git");
        c.args(["add", "--intent-to-add", extra::PATH]);
        if ctx.probe(&mut c).is_none() {
            ui::warn(&format!("couldn't `git add` {}; do it before switching", extra::PATH));
        } else {
            ui::info(&format!(
                "created {} (tracked in git so the flake sees it)",
                extra::PATH
            ));
        }
    }
    Ok(())
}

/// The lists `vx add` writes to.
fn targets(host: Target, system: bool) -> Vec<List> {
    match (host, system) {
        (Target::Both, false) => vec![List::Both],
        (Target::Arc, false) => vec![List::Arc],
        (Target::Mac, false) => vec![List::Mac],
        (Target::Both, true) => vec![List::SystemArc, List::SystemMac],
        (Target::Arc, true) => vec![List::SystemArc],
        (Target::Mac, true) => vec![List::SystemMac],
    }
}

fn add(ctx: &Ctx, pkgs: &[String], host: Target, system: bool, yes: bool, no_switch: bool) -> Result<()> {
    for p in pkgs {
        if !extra::valid_attr(p) {
            bail!("`{p}` isn't a nixpkgs attribute name (vx add takes names like ripgrep or python3Packages.requests)");
        }
    }
    let lists = targets(host, system);
    let arc = lists.iter().any(|l| l.on_arc());
    let mac = lists.iter().any(|l| l.on_mac());

    // Check every package against the pinned nixpkgs, for each machine it goes to.
    let mut unfree_names = Vec::new();
    for (on, sys, machine) in [(arc, LINUX, "the pc"), (mac, DARWIN, "the mac")] {
        if !on {
            continue;
        }
        let info = lookup(ctx, sys, pkgs)?;
        for p in pkgs {
            let i = &info[p];
            check_found(p, i)?;
            if !i.available {
                bail!(
                    "{p} isn't available on {machine} ({sys}); try --host {}",
                    if sys == LINUX { "mac" } else { "arc" }
                );
            }
            if i.broken {
                bail!("{p} is marked broken in this nixpkgs");
            }
        }
        let unfree = unfree_closure(ctx, sys, pkgs)?;
        if sys == DARWIN && !unfree.is_empty() {
            bail!(
                "the mac config allows no unfree packages, and this needs {} (use homebrew, or --host arc)",
                unfree.join(", ")
            );
        }
        unfree_names.extend(unfree);
    }

    let existing = read_extra(ctx)?;
    let new_file = existing.is_none();
    let mut text = existing.unwrap_or_else(|| extra::TEMPLATE.to_string());
    let mut changed = false;
    let on_both = extra::entries(&text, List::Both)?;
    for l in &lists {
        // Already on both machines makes a one-machine add pointless.
        let (skip, want): (Vec<String>, Vec<String>) = pkgs
            .iter()
            .cloned()
            .partition(|p| matches!(l, List::Arc | List::Mac) && on_both.contains(p));
        for p in &skip {
            ui::info(&format!("{p} is already on both machines"));
        }
        let (t, added) = extra::insert(&text, *l, &want)?;
        text = t;
        for p in &want {
            if added.contains(p) {
                ui::ok(&format!("added {p} ({})", l.label()));
                changed = true;
            } else {
                ui::info(&format!("{p} is already there ({})", l.label()));
            }
        }
    }
    if !unfree_names.is_empty() {
        let (t, added) = extra::insert(&text, List::Unfree, &unfree_names)?;
        text = t;
        if !added.is_empty() {
            ui::info(&format!("allowed unfree on the pc: {}", added.join(", ")));
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    write_extra(ctx, &text, new_file)?;
    for p in pkgs {
        let hits = defined_elsewhere(ctx, p)?;
        if !hits.is_empty() {
            ui::dim(&format!("note: {p} is also mentioned in:"));
            print_hits(&hits);
        }
    }
    offer_switch(ctx, arc, mac, yes, no_switch)
}

fn remove(ctx: &Ctx, pkgs: &[String], yes: bool, no_switch: bool) -> Result<()> {
    let Some(mut text) = read_extra(ctx)? else {
        for p in pkgs {
            explain_elsewhere(ctx, p)?;
        }
        bail!("nothing was added with vx yet ({} doesn't exist)", extra::PATH);
    };
    let (mut arc, mut mac, mut changed) = (false, false, false);
    let mut gone = Vec::new();
    for p in pkgs {
        let mut found = false;
        for l in List::PACKAGES {
            let (t, hit) = extra::remove(&text, l, p)?;
            if hit {
                text = t;
                found = true;
                arc |= l.on_arc();
                mac |= l.on_mac();
                ui::ok(&format!("removed {p} ({})", l.label()));
            }
        }
        if found {
            changed = true;
            gone.push(p.clone());
        } else {
            explain_elsewhere(ctx, p)?;
        }
    }
    if !changed {
        bail!("nothing to remove");
    }

    // Drop unfree names nothing on the pc needs any more.
    let allowed = extra::entries(&text, List::Unfree)?;
    if !allowed.is_empty() {
        let mut still = Vec::new();
        for l in [List::Both, List::Arc, List::SystemArc] {
            still.extend(extra::entries(&text, l)?);
        }
        still.sort();
        still.dedup();
        let needed = unfree_closure(ctx, LINUX, &still)?;
        for name in allowed.iter().filter(|n| !needed.contains(n)) {
            let (t, _) = extra::remove(&text, List::Unfree, name)?;
            text = t;
            ui::info(&format!("no longer allowing unfree {name}"));
        }
    }

    write_extra(ctx, &text, false)?;
    for p in &gone {
        let hits = defined_elsewhere(ctx, p)?;
        if !hits.is_empty() {
            ui::warn(&format!("{p} is still mentioned elsewhere, so it may stay installed:"));
            print_hits(&hits);
        }
    }
    offer_switch(ctx, arc, mac, yes, no_switch)
}

/// Not in extra.nix: say where it comes from instead, if anywhere.
fn explain_elsewhere(ctx: &Ctx, p: &str) -> Result<()> {
    let hits = defined_elsewhere(ctx, p)?;
    if hits.is_empty() {
        ui::warn(&format!("{p} isn't in {} or anywhere else in modules/", extra::PATH));
    } else {
        ui::warn(&format!(
            "{p} wasn't added by vx, so vx can't remove it; edit where it's set:"
        ));
        print_hits(&hits);
    }
    Ok(())
}

fn print_hits(hits: &[(String, usize)]) {
    const MAX: usize = 8;
    for (file, line) in hits.iter().take(MAX) {
        ui::dim(&format!("  {file}:{line}"));
    }
    if hits.len() > MAX {
        ui::dim(&format!("  and {} more", hits.len() - MAX));
    }
}

/// Lines under modules/ (other than extra.nix) that mention the package as a
/// whole word, ignoring comments.
fn defined_elsewhere(ctx: &Ctx, pkg: &str) -> Result<Vec<(String, usize)>> {
    let repo = ctx.repo()?;
    let re = word_regex(pkg);
    let mut hits = Vec::new();
    for entry in walkdir::WalkDir::new(repo.join("modules")).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "nix") {
            continue;
        }
        let rel = rel(repo, path);
        if rel == extra::PATH {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            let code = line.split('#').next().unwrap_or("");
            if mentions(&re, code) {
                hits.push((rel.clone(), n + 1));
            }
        }
    }
    Ok(hits)
}

fn word_regex(pkg: &str) -> regex::Regex {
    // Not part of a longer name, and not an option path like `programs.foo.enable`
    // unless the package is itself dotted.
    regex::Regex::new(&format!(
        r"(?:^|[^A-Za-z0-9_'.-]|pkgs\.)(?P<n>{})(?:$|[^A-Za-z0-9_'-])",
        regex::escape(pkg)
    ))
    .expect("valid regex")
}

/// A mention of the package, not an unrelated attribute of the same name (`discord = "";`).
fn mentions(re: &regex::Regex, code: &str) -> bool {
    re.captures_iter(code).any(|c| {
        let rest = code[c.name("n").map_or(0, |m| m.end())..].trim_start();
        !(rest.starts_with('=') && !rest.starts_with("=="))
    })
}

fn rel(repo: &Path, p: &Path) -> String {
    p.strip_prefix(repo).unwrap_or(p).to_string_lossy().into_owned()
}

fn offer_switch(ctx: &Ctx, arc: bool, mac: bool, yes: bool, no_switch: bool) -> Result<()> {
    let here = match ctx.host {
        Host::NixOs => arc,
        Host::Mac => mac,
    };
    let other = match ctx.host {
        Host::NixOs => mac.then_some("mac"),
        Host::Mac => arc.then_some("arc"),
    };
    if let Some(o) = other {
        ui::dim(&format!(
            "for the {o}: commit and push, then switch there (vx @{o} switch)"
        ));
    }
    if !here || no_switch {
        return Ok(());
    }
    if !yes && !ui::confirm("switch now?") {
        ui::dim("later: vx switch");
        return Ok(());
    }
    let exe = std::env::current_exe().context("could not find the vx binary")?;
    let mut c = Command::new(exe);
    c.arg("--repo").arg(&ctx.repo);
    if ctx.verbose {
        c.arg("-v");
    }
    c.arg("switch");
    ctx.run(&mut c)
}

fn list(ctx: &Ctx) -> Result<()> {
    let Some(text) = read_extra(ctx)? else {
        ui::info("nothing added with vx yet (vx add <pkg>)");
        return Ok(());
    };
    let mut any = false;
    for l in List::ALL {
        let items = extra::entries(&text, l)?;
        if items.is_empty() {
            continue;
        }
        any = true;
        ui::heading(l.label());
        for i in items {
            println!("  {i}");
        }
    }
    if !any {
        ui::info(&format!("{} has no packages yet (vx add <pkg>)", extra::PATH));
    }
    Ok(())
}

// ── ephemeral ──────────────────────────────────────────────────────────

fn ephemeral(ctx: &Ctx) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let mut c = ctx.sudo("find");
    c.args(["/", "-xdev", "-type", "f"]);
    for p in ["/proc/*", "/sys/*", "/run/*", "/tmp/*", "/etc/static/*"] {
        c.args(["-not", "-path", p]);
    }
    if ctx.verbose {
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        ui::dim(&format!("$ sudo {}", args.join(" ")));
    }
    // find exits non-zero on unreadable paths; the list is still what we want.
    let out = c.stderr(Stdio::null()).output().context("could not start sudo")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut files: Vec<&str> = text.lines().collect();
    if files.is_empty() && !out.status.success() {
        bail!("find failed ({})", out.status);
    }
    files.sort_unstable();
    let mut stdout = std::io::stdout().lock();
    for f in files {
        use std::io::Write;
        if writeln!(stdout, "{f}").is_err() {
            break; // e.g. piped into head
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_system_prefix() {
        assert_eq!(strip_system("legacyPackages.x86_64-linux.ripgrep"), "ripgrep");
        assert_eq!(
            strip_system("legacyPackages.aarch64-darwin.python3Packages.requests"),
            "python3Packages.requests"
        );
        assert_eq!(strip_system("weird"), "weird");
    }

    #[test]
    fn ranks_exact_then_prefix() {
        let mut v = vec!["ripgrep-all", "python3Packages.ripgrep", "ripgrep", "fzf"];
        v.sort_by_key(|a| rank(a, "ripgrep"));
        assert_eq!(v, ["ripgrep", "python3Packages.ripgrep", "ripgrep-all", "fzf"]);
    }

    #[test]
    fn word_match() {
        let re = word_regex("jq");
        assert!(mentions(&re, "        jq"));
        assert!(mentions(&re, "home.packages = [ pkgs.jq ];"));
        assert!(mentions(&re, "x = \"${pkgs.jq}/bin/jq\";"));
        assert!(!mentions(&re, "        jql"));
        assert!(!mentions(&re, "        gojq"));
        assert!(!mentions(&re, "  programs.jq.enable = true;"));
        assert!(!mentions(&re, "  jq = \"\";"));
        let re = word_regex("ripgrep");
        assert!(mentions(&re, "    ripgrep.enable = true;"));
        let re = word_regex("rustup");
        assert!(mentions(&re, "++ lib.optional isLinux rustup;"));
    }

    #[test]
    fn add_targets() {
        assert_eq!(targets(Target::Both, false), [List::Both]);
        assert_eq!(targets(Target::Both, true), [List::SystemArc, List::SystemMac]);
        assert_eq!(targets(Target::Mac, true), [List::SystemMac]);
    }

    #[test]
    fn drv_names() {
        assert_eq!(drv_name("discord-unwrapped-1.0.168"), "discord-unwrapped");
        assert_eq!(drv_name("steam-1.0"), "steam");
        assert_eq!(drv_name("teamspeak6-client-6.0.0"), "teamspeak6-client");
        assert_eq!(drv_name("hello"), "hello");
    }

    #[test]
    fn truncates_descriptions() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a longer description", 8), "a longe…");
    }
}
