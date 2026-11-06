# Flake-level plumbing: enables `flake.modules.<class>.<name>` (the dendritic
# storage for NixOS / home-manager modules) and declares supported systems.
{ inputs, ... }:
{
  imports = [ inputs.flake-parts.flakeModules.modules ];

  # x86_64-linux for the PC; aarch64-darwin so the dev shell works on the Mac.
  systems = [
    "x86_64-linux"
    "aarch64-darwin"
  ];
}
