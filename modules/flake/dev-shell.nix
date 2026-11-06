# `nix develop` in this repo (works on the Mac too) and `nix fmt`.
{
  perSystem =
    { pkgs, ... }:
    {
      formatter = pkgs.nixfmt-tree;
      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          nixd
          nixfmt
          nix-output-monitor
          git
          just
          sops
          age
          ssh-to-age
        ];
      };
    };
}
