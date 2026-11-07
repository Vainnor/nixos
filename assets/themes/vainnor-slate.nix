# Vainnor slate: the palette of the homelab README and the LibreWolf userContent.css.
# base16 slots for Stylix, plus `extra` colours a few hand-styled things use.
{
  scheme = {
    scheme = "Vainnor Slate";
    author = "vainnor";
    base00 = "10141B"; # background
    # panels, bar modules. `vx theme check` WARNs: 1.07 step over base00 (wants 1.08).
    # Kept: these are the exact homelab README / LibreWolf userContent.css colours.
    base01 = "161B24";
    base02 = "1D232E"; # selection, raised surfaces
    base03 = "6C7A8F"; # comments, muted
    base04 = "8D98AB"; # dim text
    base05 = "D3D9E2"; # text
    base06 = "E8EAEE";
    base07 = "F5F7FB";
    base08 = "E5675F"; # red
    base09 = "E8803F"; # orange
    base0A = "E6A52E"; # yellow
    base0B = "94C47D"; # green
    base0C = "4FC3D0"; # cyan
    base0D = "B5A8F5"; # accent (lilac): focus, active, links
    base0E = "6F9FD8"; # blue
    base0F = "D4CDF9"; # light lilac
  };
  extra = {
    border = "35465C"; # unfocused window borders
    accent2 = "D4CDF9";
  };
}
