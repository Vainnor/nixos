# Claude Code with two logins: ~/.claude (Claude Ultra, the default) and
# ~/.claude-max (Claude Max). The second dir links skills, plugins, projects,
# agents, commands and settings.json to ~/.claude, so only the login differs.
#
# Logins stay manual, one per dir (fish: use $HOME, `~` after `=` doesn't expand):
#   claude auth login
#   env CLAUDE_CONFIG_DIR=$HOME/.claude-max claude auth login
#
# T3 Code (dev/t3.nix) gets one provider instance per dir; see providerInstances.
#
# Agent sessions (T3, `claude`) start in a project folder but their Bash tool
# doesn't load direnv, so a SessionStart hook exports the folder's direnv
# environment into the session (the project's devShell: compilers, libraries).
{
  flake.modules.nixos.dev.unfree.allowed = [ "claude-code" ];

  flake.modules.homeManager.dev =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      inherit (pkgs.stdenv.hostPlatform) isLinux;
      home = config.home.homeDirectory;
      shared = [
        "skills"
        "plugins"
        "projects"
        "agents"
        "commands"
      ];
      link = p: config.lib.file.mkOutOfStoreSymlink "${home}/.claude/${p}";
      jq = lib.getExe pkgs.jq;

      # Exports the project's direnv environment into a Claude session.
      direnvHook = pkgs.writeShellScript "claude-direnv-env" ''
        [ -n "''${CLAUDE_ENV_FILE:-}" ] || exit 0
        command -v direnv >/dev/null || exit 0
        direnv export bash 2>/dev/null >> "$CLAUDE_ENV_FILE" || true
      '';
    in
    lib.mkIf isLinux {
      # The Mac installs Claude Code with Homebrew (hosts/mac/homebrew.nix).
      home.packages = [ pkgs.claude-code ];

      home.file = lib.mkMerge (
        map (p: { ".claude-max/${p}".source = link p; }) shared
        ++ [ { ".claude-max/settings.json".source = link "settings.json"; } ]
      );

      # The link targets must exist, and settings.json stays a plain file that
      # Claude (and you) can edit; the hook is added once, never duplicated.
      home.activation.claudeDirs = lib.hm.dag.entryBefore [ "checkLinkTargets" ] ''
        run mkdir -p ${lib.concatMapStringsSep " " (p: "${home}/.claude/${p}") shared}
        s=${home}/.claude/settings.json
        [ -s "$s" ] || echo '{}' > "$s"
        if ! ${jq} -e '[.hooks.SessionStart[]?.hooks[]?.command] | any(test("claude-direnv-env"))' "$s" >/dev/null 2>&1; then
          tmp=$(mktemp)
          ${jq} --arg cmd "${direnvHook}" \
            '.hooks.SessionStart = ((.hooks.SessionStart // []) + [{"hooks": [{"type": "command", "command": $cmd}]}])' \
            "$s" > "$tmp" && run mv "$tmp" "$s"
        else
          # Point an existing hook at this generation's script.
          tmp=$(mktemp)
          ${jq} --arg cmd "${direnvHook}" \
            '(.hooks.SessionStart[]?.hooks[]? | select(.command | test("claude-direnv-env")) | .command) = $cmd' \
            "$s" > "$tmp" && run mv "$tmp" "$s"
        fi
      '';

      # T3 Code's provider instances, merged into its settings (its other
      # settings are T3's own and stay as they are).
      # Both T3 homes: the desktop app's (~/.t3) and the background server's
      # (~/.t3-server, dev/t3.nix), which reads it when it starts.
      home.activation.t3ProviderInstances = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        for t3home in ${home}/.t3 ${home}/.t3-server; do
          f=$t3home/userdata/settings.json
          run mkdir -p $t3home/userdata
          [ -s "$f" ] || echo '{}' > "$f"
          tmp=$(mktemp)
          ${jq} --arg max "${home}/.claude-max" '
            .providerInstances = ((.providerInstances // {}) + {
              "claudeAgent": {"driver": "claudeAgent", "displayName": "Claude Ultra", "enabled": true, "config": {}},
              "claudeAgent_max": {"driver": "claudeAgent", "displayName": "Claude Max", "enabled": true, "config": {"homePath": $max}}
            })' "$f" > "$tmp" && run mv "$tmp" "$f"
        done
      '';

      home.sessionVariables.CLAUDE_CODE_DISABLE_AUTOUPDATER = "1"; # Nix updates it
    };
}
