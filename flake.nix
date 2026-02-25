{
  description = "Kitty scrollback viewer for Kakoune";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rust = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rust;

        # Custom source filter: include Cargo sources + rc/
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            (craneLib.filterCargoSources path type)
            || (builtins.match ".*rc/.*\\.kak$" path != null)
            || (builtins.match ".*rc/.*\\.conf$" path != null)
            || (baseNameOf path == "rc" && type == "directory");
        };

        kakoune-scrollback = craneLib.buildPackage {
          pname = "kakoune-scrollback";
          version = "0.1.0";
          inherit src;

          postInstall = ''
            install -Dm644 ${./rc/kakoune-scrollback.kak} \
              $out/share/kak/autoload/plugins/kakoune-scrollback/kakoune-scrollback.kak
          '';
        };
      in {
        packages.default = kakoune-scrollback;

        devShells.default = pkgs.mkShell {
          inputsFrom = [ kakoune-scrollback ];
          packages = with pkgs; [
            rust
            cargo-watch
            kitty
            kakoune
          ];
        };
      }
    );
}
