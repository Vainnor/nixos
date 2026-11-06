{
  description = "Java project (JDK, Gradle and Maven pinned by this flake)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      forAll = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-darwin"
      ];
    in
    {
      devShells = forAll (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          # Change to jdk17, jdk25, ... per project.
          jdk = pkgs.jdk21;
        in
        {
          default = pkgs.mkShell {
            packages = [
              jdk
              (pkgs.gradle.override { java = jdk; })
              (pkgs.maven.override { jdk_headless = jdk; })
              pkgs.jdt-language-server
            ];
            JAVA_HOME = jdk.home;
          };
        }
      );
    };
}
