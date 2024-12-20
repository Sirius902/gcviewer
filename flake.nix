{
  description = "flake for gcviewer";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane, fenix, flake-parts, ... }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem = { system, ... }:
        let
          pkgs = import nixpkgs {
            inherit system;

            overlays = [ fenix.overlays.default ];
          };

          inherit (pkgs) lib;

          toolchain = fenix.packages.${system}.fromToolchainFile {
            file = ./rust-toolchain.toml;
            sha256 = "sha256-1uC3iVKIjZAtQ57qtpGIfvCPl1MTdTfWibjB37VWFPg=";
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

          unfilteredRoot = ./.;
          src = lib.fileset.toSource {
            root = unfilteredRoot;
            fileset = lib.fileset.unions [
              (craneLib.fileset.commonCargoSources unfilteredRoot)
              (lib.fileset.fileFilter
                (file: lib.any file.hasExt [ "wgsl" ])
                ./src
              )
              (lib.fileset.maybeMissing ./resource)
            ];
          };

          commonArgs = {
            inherit src;
            strictDeps = true;

            buildInputs = with pkgs; [
              libGL
              libxkbcommon
              vulkan-loader
              wayland
              xorg.libX11
              xorg.libXcursor
              xorg.libxcb
              xorg.libXi
            ];
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          gcviewer = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
            pname = "gcviewer";

            nativeBuildInputs = with pkgs; [ makeWrapper ];

            postInstall = ''
              wrapProgram $out/bin/gcviewer \
                --suffix LD_LIBRARY_PATH : ${lib.makeLibraryPath commonArgs.buildInputs}
            '';

            env.VERSION = "v${(craneLib.crateNameFromCargoToml { inherit src; }).version}";

            desktopItems = with pkgs; [
              (makeDesktopItem {
                name = "gcviewer";
                exec = "gcviewer";
                desktopName = "gcviewer";
                categories = [ "Utility" ];
              })
            ];

            meta = with lib; {
              mainProgram = "gcviewer";
              homepage = "https://github.com/Sirius902/gcviewer";
              platforms = platforms.linux;
            };
          });
        in
        with pkgs; {
          formatter = nixpkgs-fmt;

          checks = {
            inherit gcviewer;

            gcviewer-clippy = craneLib.cargoClippy (commonArgs // {
              inherit cargoArtifacts;
            });

            gcviewer-fmt = craneLib.cargoFmt {
              inherit src;
            };
          };

          packages.default = gcviewer;

          devShells.default = craneLib.devShell {
            checks = self.checks.${system};

            packages = [ ];

            env.LD_LIBRARY_PATH = lib.makeLibraryPath commonArgs.buildInputs;
          };
        };
    };
}
