//! `.sops.yaml` and the sops metadata of `secrets/secrets.yaml`, read and
//! edited as text (no YAML library: the file has comments, anchors and a
//! commented-out example, all of which must survive an edit untouched).
//!
//! Shape this expects (the repo's file):
//! ```yaml
//! keys:
//!   - &mac age1... # MacBook (edits secrets)
//!   # - &arc age1...   # the PC: `vx secrets host` prints it
//! creation_rules:
//!   - path_regex: secrets/[^/]+\.yaml$
//!     key_groups:
//!       - age:
//!           - *mac
//!           # - *arc
//! ```
//! A host is a recipient when its anchor (`- &<host> age1...`) is under
//! `keys:` and its alias (`- *<host>`) is in a creation rule's age list.
//! Used by the installer (a new host adds itself before its install commit)
//! and by `vx secrets add-host / rekey / ready`.

use anyhow::{Result, bail};
use regex::Regex;

/// The repo's .sops.yaml when it isn't on disk (dry runs, and the Nix build of
/// vx, which only has tools/vx). Keep in step with the real file.
pub const SAMPLE: &str = "# Who can decrypt secrets/*.yaml. After editing this file run
#   sops updatekeys secrets/secrets.yaml
keys:
  - &mac age1g8rwdq87ewjphu9s4f4dhx03sxg33a7xxs78l2rcwa9zhgvyxpnsxx056y # MacBook (edits secrets)
  # - &arc age1...   # the PC: `vx secrets host` prints it
creation_rules:
  - path_regex: secrets/[^/]+\\.yaml$
    key_groups:
      - age:
          - *mac
          # - *arc
";

pub fn is_age_key(k: &str) -> bool {
    k.len() > 10 && k.starts_with("age1") && k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

fn check_name(host: &str) -> Result<()> {
    let ok = host.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && host.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        bail!("{host:?} can't be a .sops.yaml anchor name");
    }
    Ok(())
}

fn indent(l: &str) -> usize {
    l.len() - l.trim_start().len()
}

/// (first, end) line indexes of the block under a top-level `<key>:`.
fn block(lines: &[&str], key: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| l.trim_end() == format!("{key}:"))?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, l)| !l.trim().is_empty() && indent(l) == 0 && !l.starts_with('#'))
        .map(|(i, _)| i)
        .unwrap_or(lines.len());
    Some((start + 1, end))
}

/// `- &name age1...` (or `# - &name ...` when commented): (commented, name, key).
fn anchor(l: &str) -> Option<(bool, String, String)> {
    let re = Regex::new(r"^\s*(#\s*)?-\s+&([a-z][a-z0-9-]*)\s+(\S+)").unwrap();
    let c = re.captures(l)?;
    Some((c.get(1).is_some(), c[2].to_string(), c[3].to_string()))
}

/// `- *name` (or `# - *name`): (commented, name).
fn alias(l: &str) -> Option<(bool, String)> {
    let re = Regex::new(r"^\s*(#\s*)?-\s+\*([a-z][a-z0-9-]*)\s*(#.*)?$").unwrap();
    let c = re.captures(l)?;
    Some((c.get(1).is_some(), c[2].to_string()))
}

/// Line indexes of the age list entries in creation_rules (the lines after `- age:`).
fn age_lists(lines: &[&str]) -> Vec<Vec<usize>> {
    let Some((s, e)) = block(lines, "creation_rules") else { return vec![] };
    let mut out = vec![];
    let mut i = s;
    while i < e {
        let l = lines[i];
        if l.trim_start().starts_with("- age:") || l.trim_start().starts_with("age:") {
            let base = indent(l);
            let mut v = vec![];
            let mut j = i + 1;
            while j < e {
                let m = lines[j];
                if m.trim().is_empty() {
                    j += 1;
                    continue;
                }
                if indent(m) <= base {
                    break;
                }
                if alias(m).is_some() {
                    v.push(j);
                }
                j += 1;
            }
            out.push(v);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Recipients sops encrypts for: (host, age key) for every anchor under
/// `keys:` that a creation rule's age list uses (both uncommented).
pub fn recipients(yaml: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = yaml.lines().collect();
    let used: Vec<String> = age_lists(&lines)
        .into_iter()
        .flatten()
        .filter_map(|i| alias(lines[i]))
        .filter(|(c, _)| !c)
        .map(|(_, n)| n)
        .collect();
    let Some((s, e)) = block(&lines, "keys") else { return vec![] };
    lines[s..e]
        .iter()
        .filter_map(|l| anchor(l))
        .filter(|(c, n, k)| !c && used.contains(n) && is_age_key(k))
        .map(|(_, n, k)| (n, k))
        .collect()
}

/// Make `host` a recipient with `key`: its anchor under `keys:` (a new line,
/// the commented example uncommented, or an old key replaced) and its alias in
/// the first creation rule's age list. Everything else stays byte for byte.
pub fn set_host(yaml: &str, host: &str, key: &str, comment: &str) -> Result<String> {
    check_name(host)?;
    if !is_age_key(key) {
        bail!("{key:?} isn't an age key (they start with age1)");
    }
    let comment = comment.replace('\n', " ");
    let mut lines: Vec<String> = yaml.lines().map(str::to_string).collect();

    // 1. The anchor.
    let v = lines.clone();
    let refs: Vec<&str> = v.iter().map(String::as_str).collect();
    let Some((s, e)) = block(&refs, "keys") else { bail!(".sops.yaml has no `keys:` list") };
    let entries: Vec<usize> = (s..e).filter(|&i| anchor(refs[i]).is_some()).collect();
    let mine = |want_commented: bool| {
        entries.iter().copied().find(|&i| anchor(refs[i]).is_some_and(|(c, n, _)| n == host && c == want_commented))
    };
    let pad = entries.iter().find(|&&i| anchor(refs[i]).is_some_and(|(c, _, _)| !c)).map(|&i| indent(refs[i])).unwrap_or(2);
    let line = format!("{}- &{host} {key} # {comment}", " ".repeat(pad));
    if let Some(i) = mine(false) {
        // Already there: only swap the key, keep its comment.
        let (_, _, old) = anchor(refs[i]).unwrap();
        lines[i] = refs[i].replacen(&old, key, 1);
    } else if let Some(i) = mine(true) {
        lines[i] = line;
    } else {
        // After the last uncommented entry (a commented example stays below).
        let at = entries.iter().rev().find(|&&i| anchor(refs[i]).is_some_and(|(c, _, _)| !c)).map(|&i| i + 1).unwrap_or(s);
        lines.insert(at, line);
    }

    // 2. The alias, in the first age list.
    let v = lines.clone();
    let refs: Vec<&str> = v.iter().map(String::as_str).collect();
    let lists = age_lists(&refs);
    let Some(list) = lists.first() else { bail!(".sops.yaml has no creation rule with an age list") };
    if list.iter().any(|&i| alias(refs[i]).is_some_and(|(c, n)| !c && n == host)) {
        // already used
    } else if let Some(&i) = list.iter().find(|&&i| alias(refs[i]).is_some_and(|(c, n)| c && n == host)) {
        let pad = indent(refs[i]);
        lines[i] = format!("{}- *{host}", " ".repeat(pad));
    } else {
        let last = list.iter().rev().find(|&&i| alias(refs[i]).is_some_and(|(c, _)| !c)).copied();
        let (at, pad) = match last {
            Some(i) => (i + 1, indent(refs[i])),
            None => bail!(".sops.yaml's age list has no entries to add {host} next to"),
        };
        lines.insert(at, format!("{}- *{host}", " ".repeat(pad)));
    }
    let mut out = lines.join("\n");
    if yaml.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// The age recipients secrets.yaml is encrypted for (its `sops:` metadata).
pub fn file_recipients(secrets_yaml: &str) -> Vec<String> {
    let Some(at) = secrets_yaml.lines().position(|l| l.trim_end() == "sops:") else { return vec![] };
    let re = Regex::new(r"^\s*(?:-\s+)?recipient:\s*(age1[0-9a-z]+)\s*$").unwrap();
    secrets_yaml.lines().skip(at + 1).filter_map(|l| re.captures(l)).map(|c| c[1].to_string()).collect()
}

/// Recipients .sops.yaml lists that secrets.yaml isn't encrypted for yet.
pub fn missing(sops_yaml: &str, secrets_yaml: &str) -> Vec<(String, String)> {
    let have = file_recipients(secrets_yaml);
    recipients(sops_yaml).into_iter().filter(|(_, k)| !have.contains(k)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const K1: &str = "age1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqs3290gq";
    const K2: &str = "age1zvkyg2lqzraa2lnjvqej32nkuu0ues2s82hzrye869xeexvn73equnujwj";

    /// The repo's .sops.yaml (tests run from tools/vx), else the copy above.
    fn real() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.sops.yaml");
        std::fs::read_to_string(p).unwrap_or_else(|_| SAMPLE.to_string())
    }

    #[test]
    fn sample_matches_the_real_file() {
        // Catches the copy drifting from the repo (skipped where the repo isn't there).
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.sops.yaml");
        if let Ok(s) = std::fs::read_to_string(p) {
            assert_eq!(recipients(&s), recipients(SAMPLE), "update sops::SAMPLE from .sops.yaml");
        }
    }

    #[test]
    fn reads_the_real_file() {
        let r = recipients(&real());
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!(r[0].0, "mac");
        assert!(is_age_key(&r[0].1));
    }

    #[test]
    fn arc_uncomments_the_example() {
        let y = real();
        let out = set_host(&y, "arc", K1, "arc").unwrap();
        assert!(out.contains(&format!("\n  - &arc {K1} # arc\n")), "{out}");
        assert!(out.contains("\n          - *arc\n"), "{out}");
        assert!(!out.contains("# - &arc") && !out.contains("# - *arc"));
        assert_eq!(recipients(&out).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["mac", "arc"]);
        // Only those two lines changed; comments, the mac key and the rest are untouched.
        let (a, b): (Vec<&str>, Vec<&str>) = (y.lines().collect(), out.lines().collect());
        assert_eq!(a.len(), b.len());
        assert_eq!(a.iter().zip(&b).filter(|(x, y)| x != y).count(), 2);
        assert!(out.ends_with("- *arc\n") || out.contains("- *arc\n"));
        // Again: no change. A new key for arc: only the key changes.
        assert_eq!(set_host(&out, "arc", K1, "arc").unwrap(), out);
        let again = set_host(&out, "arc", K2, "something else").unwrap();
        assert!(again.contains(&format!("  - &arc {K2} # arc\n")));
        assert!(!again.contains(K1));
        assert_eq!(again.matches("- *arc").count(), 1);
    }

    #[test]
    fn a_new_host_gets_new_lines() {
        let out = set_host(&real(), "forge", K2, "forge (vx install)").unwrap();
        let lines: Vec<&str> = out.lines().collect();
        let a = lines.iter().position(|l| l.contains("&forge")).unwrap();
        assert_eq!(lines[a], format!("  - &forge {K2} # forge (vx install)"));
        assert!(lines[a - 1].contains("&mac"), "right after the last real key");
        assert!(lines[a + 1].contains("# - &arc"), "the commented example stays below");
        let b = lines.iter().position(|l| l.trim() == "- *forge").unwrap();
        assert!(lines[b - 1].trim() == "- *mac" && lines[b + 1].trim() == "# - *arc");
        assert_eq!(lines[b], "          - *forge");
        assert_eq!(recipients(&out).len(), 2);
        // Then arc too.
        let out2 = set_host(&out, "arc", K1, "arc").unwrap();
        assert_eq!(recipients(&out2).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["mac", "forge", "arc"]);
    }

    #[test]
    fn refuses_bad_input() {
        assert!(set_host(&real(), "Arc", K1, "").is_err());
        assert!(set_host(&real(), "arc", "ssh-ed25519 AAAA", "").is_err());
        assert!(set_host("creation_rules: []\n", "arc", K1, "").is_err());
        assert!(set_host("keys:\n  - &mac age1aaaaaaaaaaaa\n", "arc", K1, "").is_err(), "no creation rule");
    }

    #[test]
    fn compares_with_the_file() {
        let secrets = format!(
            "a: ENC[x]\nsops:\n    age:\n        - enc: |\n            -----BEGIN AGE ENCRYPTED FILE-----\n            x\n            -----END AGE ENCRYPTED FILE-----\n          recipient: {K1}\n    lastmodified: \"2026\"\n    version: 3.13.3\n"
        );
        assert_eq!(file_recipients(&secrets), [K1]);
        // The repo's own secrets.yaml, when it's there.
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../secrets/secrets.yaml");
        if let Ok(s) = std::fs::read_to_string(p) {
            let r = file_recipients(&s);
            assert!(!r.is_empty() && r.iter().all(|k| is_age_key(k)), "{r:?}");
            assert!(missing(&real(), &s).is_empty(), "the repo's file is up to date for .sops.yaml");
        }
        let y = set_host(&set_host(SAMPLE, "arc", K1, "arc").unwrap(), "forge", K2, "forge").unwrap();
        let m = missing(&y, &secrets);
        assert_eq!(m.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["mac", "forge"]);
        // A recipient in the data, not in sops metadata, doesn't count.
        assert!(file_recipients(&format!("note: {K2}\nrecipient: {K2}\n")).is_empty());
    }
}
