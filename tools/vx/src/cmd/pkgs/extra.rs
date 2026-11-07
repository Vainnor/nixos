//! `modules/apps/extra.nix`: the vx-owned module with plain package lists.
//!
//! Each list starts at a `# vx:<name>` marker line and ends at the next line
//! that starts with `]`. Inside, every line is an item (`foo`, `foo # why`),
//! a comment, or blank. Editing keeps comments: a comment line travels with
//! the item below it, a trailing comment stays on its item's line, and
//! comments after the last item stay at the end. Items are sorted and unique.

use anyhow::{Result, bail};

/// Where the file lives in the repo.
pub const PATH: &str = "modules/apps/extra.nix";

/// What `vx add` writes when the file doesn't exist yet. Already nixfmt-formatted.
pub const TEMPLATE: &str = r#"# Packages added with `vx add` and taken out with `vx remove`: plain lists,
# one nixpkgs attribute per line, kept sorted. Editing by hand is fine; vx only
# touches the lines between a `# vx:<list>` marker and its closing bracket.
# Anything that needs settings belongs in its own module instead.
{
  # User packages (home-manager): on both machines, or only the PC / the Mac.
  flake.modules.homeManager.base =
    { pkgs, ... }:
    let
      inherit (pkgs.stdenv.hostPlatform) isDarwin;
      both = with pkgs; [
        # vx:both
      ];
      arc = with pkgs; [
        # vx:arc
      ];
      mac = with pkgs; [
        # vx:mac
      ];
    in
    {
      home.packages = both ++ (if isDarwin then mac else arc);
    };

  # System packages on the PC, and unfree names vx allowed for it.
  flake.modules.nixos.arc =
    { pkgs, ... }:
    {
      environment.systemPackages = with pkgs; [
        # vx:system-arc
      ];
      unfree.allowed = [
        # vx:unfree
      ];
    };

  # System packages on the Mac.
  flake.modules.darwin.mac =
    { pkgs, ... }:
    {
      environment.systemPackages = with pkgs; [
        # vx:system-mac
      ];
    };
}
"#;

/// One list in the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum List {
    /// User packages on both machines.
    Both,
    /// User packages on the PC only.
    Arc,
    /// User packages on the Mac only.
    Mac,
    /// System packages on the PC.
    SystemArc,
    /// System packages on the Mac.
    SystemMac,
    /// Unfree package names allowed on the PC (strings).
    Unfree,
}

impl List {
    pub const ALL: [List; 6] = [
        List::Both,
        List::Arc,
        List::Mac,
        List::SystemArc,
        List::SystemMac,
        List::Unfree,
    ];
    /// The lists that hold packages (everything but `Unfree`).
    pub const PACKAGES: [List; 5] = [List::Both, List::Arc, List::Mac, List::SystemArc, List::SystemMac];

    pub fn key(self) -> &'static str {
        match self {
            List::Both => "both",
            List::Arc => "arc",
            List::Mac => "mac",
            List::SystemArc => "system-arc",
            List::SystemMac => "system-mac",
            List::Unfree => "unfree",
        }
    }

    /// Plain words for output.
    pub fn label(self) -> &'static str {
        match self {
            List::Both => "user, both machines",
            List::Arc => "user, pc only",
            List::Mac => "user, mac only",
            List::SystemArc => "system, pc",
            List::SystemMac => "system, mac",
            List::Unfree => "unfree allowed on the pc",
        }
    }

    fn marker(self) -> String {
        format!("# vx:{}", self.key())
    }

    /// Unfree names are Nix strings; the rest are attribute paths.
    fn quoted(self) -> bool {
        self == List::Unfree
    }

    /// Does this list end up on the PC?
    pub fn on_arc(self) -> bool {
        matches!(self, List::Both | List::Arc | List::SystemArc | List::Unfree)
    }

    /// Does this list end up on the Mac?
    pub fn on_mac(self) -> bool {
        matches!(self, List::Both | List::Mac | List::SystemMac)
    }
}

/// Is this a plain nixpkgs attribute path vx can write unquoted (`foo`, `python3Packages.foo`)?
pub fn valid_attr(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|part| {
            let mut c = part.chars();
            matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
                && c.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '\''))
        })
}

#[derive(Debug, Clone, PartialEq)]
struct Item {
    /// Comment lines right above the item (trimmed).
    above: Vec<String>,
    name: String,
    /// `# ...` after the item on its line.
    trailing: Option<String>,
}

#[derive(Debug)]
struct Region {
    /// Line index of the marker; items start on the next line.
    marker: usize,
    /// Line index of the closing `]`.
    close: usize,
    indent: String,
    items: Vec<Item>,
    /// Comment lines after the last item.
    tail: Vec<String>,
}

fn split_comment(line: &str) -> (&str, Option<&str>) {
    // Items never contain `#` (attribute paths or simple strings), so the
    // first `#` starts the comment.
    match line.find('#') {
        Some(i) => (line[..i].trim(), Some(line[i..].trim())),
        None => (line.trim(), None),
    }
}

fn find(lines: &[&str], list: List) -> Result<Region> {
    let marker = list.marker();
    let Some(start) = lines.iter().position(|l| l.trim() == marker) else {
        bail!(
            "{} has no `{marker}` line; put it back (or delete the file and let vx recreate it)",
            PATH
        );
    };
    let indent: String = lines[start].chars().take_while(|c| c.is_whitespace()).collect();
    let mut items = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        let t = line.trim();
        if t.starts_with(']') {
            return Ok(Region {
                marker: start,
                close: i,
                indent,
                items,
                tail: pending,
            });
        }
        if t.is_empty() {
            continue;
        }
        if t.starts_with("# vx:") {
            bail!("{} list `{}` is missing its closing `]`", PATH, list.key());
        }
        let (code, comment) = split_comment(t);
        if code.is_empty() {
            pending.push(comment.unwrap_or_default().to_string());
            continue;
        }
        let words: Vec<&str> = code.split_whitespace().collect();
        let n = words.len();
        for (j, w) in words.into_iter().enumerate() {
            let name = if list.quoted() { w.trim_matches('"') } else { w };
            items.push(Item {
                above: if j == 0 {
                    std::mem::take(&mut pending)
                } else {
                    Vec::new()
                },
                name: name.to_string(),
                trailing: if j + 1 == n { comment.map(str::to_string) } else { None },
            });
        }
    }
    bail!("{} list `{}` is missing its closing `]`", PATH, list.key())
}

fn sort_key(name: &str) -> (String, String) {
    (name.to_ascii_lowercase(), name.to_string())
}

/// Sort and drop duplicates (the first one wins, its comments are kept).
fn normalize(items: &mut Vec<Item>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|it| seen.insert(it.name.clone()));
    items.sort_by_key(|a| sort_key(&a.name));
}

fn render(text: &str, lines: &[&str], r: &Region, list: List) -> String {
    let mut out: Vec<String> = lines[..=r.marker].iter().map(|s| s.to_string()).collect();
    for it in &r.items {
        for c in &it.above {
            out.push(format!("{}{c}", r.indent));
        }
        let name = if list.quoted() {
            format!("\"{}\"", it.name)
        } else {
            it.name.clone()
        };
        match &it.trailing {
            Some(c) => out.push(format!("{}{name} {c}", r.indent)),
            None => out.push(format!("{}{name}", r.indent)),
        }
    }
    for c in &r.tail {
        out.push(format!("{}{c}", r.indent));
    }
    out.extend(lines[r.close..].iter().map(|s| s.to_string()));
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// The names in one list, in file order.
pub fn entries(text: &str, list: List) -> Result<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    Ok(find(&lines, list)?.items.into_iter().map(|i| i.name).collect())
}

/// Add names to a list. Returns the new text and the names that were new.
pub fn insert(text: &str, list: List, names: &[String]) -> Result<(String, Vec<String>)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut r = find(&lines, list)?;
    let mut added = Vec::new();
    for n in names {
        if !r.items.iter().any(|i| &i.name == n) && !added.contains(n) {
            added.push(n.clone());
            r.items.push(Item {
                above: Vec::new(),
                name: n.clone(),
                trailing: None,
            });
        }
    }
    normalize(&mut r.items);
    Ok((render(text, &lines, &r, list), added))
}

/// Take a name out of a list. Returns the new text and whether it was there.
/// Comment lines above the removed item go with it.
pub fn remove(text: &str, list: List, name: &str) -> Result<(String, bool)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut r = find(&lines, list)?;
    let before = r.items.len();
    r.items.retain(|i| i.name != name);
    let found = r.items.len() != before;
    if !found {
        return Ok((text.to_string(), false));
    }
    normalize(&mut r.items);
    Ok((render(text, &lines, &r, list), true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn template_has_every_list_empty() {
        for l in List::ALL {
            assert_eq!(entries(TEMPLATE, l).unwrap(), Vec::<String>::new(), "{l:?}");
        }
    }

    #[test]
    fn template_round_trips() {
        // Rewriting an untouched list must not change a byte.
        for l in List::ALL {
            let (t, added) = insert(TEMPLATE, l, &[]).unwrap();
            assert!(added.is_empty());
            assert_eq!(t, TEMPLATE, "{l:?}");
        }
        // Add then remove gives the template back.
        let (t, _) = insert(TEMPLATE, List::Both, &s(&["ripgrep", "hello"])).unwrap();
        let (t, _) = insert(&t, List::Unfree, &s(&["discord"])).unwrap();
        let (t, _) = remove(&t, List::Both, "hello").unwrap();
        let (t, _) = remove(&t, List::Both, "ripgrep").unwrap();
        let (t, _) = remove(&t, List::Unfree, "discord").unwrap();
        assert_eq!(t, TEMPLATE);
    }

    #[test]
    fn insert_sorts_and_formats() {
        let (t, added) = insert(TEMPLATE, List::Both, &s(&["zoxide", "Bat", "jq"])).unwrap();
        assert_eq!(added, s(&["zoxide", "Bat", "jq"]));
        assert!(t.contains(
            "      both = with pkgs; [\n        # vx:both\n        Bat\n        jq\n        zoxide\n      ];\n"
        ));
        assert_eq!(entries(&t, List::Both).unwrap(), s(&["Bat", "jq", "zoxide"]));
        // Other lists untouched.
        assert!(entries(&t, List::Arc).unwrap().is_empty());
    }

    #[test]
    fn insert_dedupes() {
        let (t, _) = insert(TEMPLATE, List::Arc, &s(&["jq", "jq"])).unwrap();
        let (t, added) = insert(&t, List::Arc, &s(&["jq", "fd"])).unwrap();
        assert_eq!(added, s(&["fd"]));
        assert_eq!(entries(&t, List::Arc).unwrap(), s(&["fd", "jq"]));
    }

    #[test]
    fn hand_written_duplicates_collapse() {
        let text = TEMPLATE.replace(
            "        # vx:mac\n",
            "        # vx:mac\n        jq\n        jq # again\n",
        );
        let (t, _) = insert(&text, List::Mac, &[]).unwrap();
        assert_eq!(entries(&t, List::Mac).unwrap(), s(&["jq"]));
    }

    #[test]
    fn unfree_is_quoted() {
        let (t, _) = insert(TEMPLATE, List::Unfree, &s(&["discord", "steam"])).unwrap();
        assert!(t.contains("        # vx:unfree\n        \"discord\"\n        \"steam\"\n      ];"));
        assert_eq!(entries(&t, List::Unfree).unwrap(), s(&["discord", "steam"]));
    }

    #[test]
    fn remove_works_and_reports_missing() {
        let (t, _) = insert(TEMPLATE, List::SystemArc, &s(&["htop", "iotop"])).unwrap();
        let (t2, found) = remove(&t, List::SystemArc, "nope").unwrap();
        assert!(!found);
        assert_eq!(t2, t);
        let (t3, found) = remove(&t, List::SystemArc, "htop").unwrap();
        assert!(found);
        assert_eq!(entries(&t3, List::SystemArc).unwrap(), s(&["iotop"]));
    }

    #[test]
    fn comments_survive() {
        let text = TEMPLATE.replace(
            "        # vx:both\n",
            "        # vx:both\n        # for the homelab\n        zellij # terminal multiplexer\n        btop\n        # keep this note\n",
        );
        let (t, _) = insert(&text, List::Both, &s(&["age"])).unwrap();
        assert!(t.contains(
            "        # vx:both\n        age\n        btop\n        # for the homelab\n        zellij # terminal multiplexer\n        # keep this note\n      ];"
        ));
        // Removing an item takes the comment above it, keeps the rest.
        let (t, _) = remove(&t, List::Both, "zellij").unwrap();
        assert!(t.contains("        # vx:both\n        age\n        btop\n        # keep this note\n      ];"));
        assert!(!t.contains("homelab"));
    }

    #[test]
    fn several_per_line_are_split() {
        let text = TEMPLATE.replace("        # vx:arc\n", "        # vx:arc\n        jq fd # tools\n");
        assert_eq!(entries(&text, List::Arc).unwrap(), s(&["jq", "fd"]));
        let (t, _) = insert(&text, List::Arc, &[]).unwrap();
        assert!(t.contains("        # vx:arc\n        fd # tools\n        jq\n      ];"));
    }

    #[test]
    fn dotted_attrs_and_validation() {
        assert!(valid_attr("python3Packages.requests"));
        assert!(valid_attr("_1password-gui"));
        assert!(valid_attr("nerd-fonts.jetbrains-mono"));
        assert!(!valid_attr(""));
        assert!(!valid_attr("foo bar"));
        assert!(!valid_attr("1foo"));
        assert!(!valid_attr("foo."));
        assert!(!valid_attr("\"x\""));
        assert!(!valid_attr("nixpkgs#foo"));
    }

    #[test]
    fn missing_marker_or_bracket_is_an_error() {
        let no_marker = TEMPLATE.replace("        # vx:mac\n", "");
        assert!(entries(&no_marker, List::Mac).is_err());
        let open = "  x = [\n    # vx:both\n    jq\n    # vx:arc\n  ];\n";
        assert!(entries(open, List::Both).is_err());
    }
}
