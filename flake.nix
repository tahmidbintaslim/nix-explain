{
  description = "Plain-English explanations and fixes for Nix error traces";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
    in
    {
      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "nix-explain";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
            src = pkgs.lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = "Plain-English explanations and fixes for Nix error traces";
              homepage = "https://github.com/tahmidbintaslim/nix-explain";
              license = pkgs.lib.licenses.mit;
              mainProgram = "nix-explain";
            };
          };
        });

      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in {
          default = pkgs.mkShell {
            packages = with pkgs; [ cargo rustc clippy rustfmt rust-analyzer ];
          };
        });
    };
}
