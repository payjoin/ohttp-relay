{
  description = "Build a cargo project without extra checks";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    crane,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [rust-overlay.overlays.default];
      };

      msrv = "1.63.0";
      rustVersions = with pkgs.rust-bin;
        builtins.mapAttrs (_name: rust-bin:
          rust-bin.override {
            extensions = ["rust-src" "rustfmt" "llvm-tools-preview"];
          })
        {
          msrv = stable.${msrv}.default;
          stable = stable.latest.default;
          nightly = nightly.latest.default;
        };

      craneLibVersions = builtins.mapAttrs (name: rust-bin: (crane.mkLib pkgs).overrideToolchain (_: rust-bin)) rustVersions;
      craneLib = craneLibVersions.nightly;

      nginxWithStream = pkgs.nginxMainline.overrideAttrs (oldAttrs: {
        configureFlags =
          oldAttrs.configureFlags
          ++ [
            "--with-stream"
            "--with-stream_ssl_module"
            "--error-log-path=/dev/null"
          ];
      });

      ohttp-relay = craneLib.buildPackage {
        src = pkgs.lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = path: type: builtins.match ".*\\.template$" path != null || craneLib.filterCargoSources path type;
        };
        strictDeps = true;

        buildInputs =
          [
            nginxWithStream
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
            pkgs.darwin.apple_sdk.frameworks.Security
          ];

        preBuild = ''
          export PATH=${nginxWithStream}/bin:$PATH
        '';
      };

      devShells = builtins.mapAttrs (_name: craneLib:
        craneLib.devShell {
          packages = with pkgs;
            [
              nginxWithStream
              cargo-edit
              cargo-nextest
              cargo-watch
              rust-analyzer
            ]
            ++ pkgs.lib.optionals (!pkgs.stdenv.isDarwin) [
              cargo-llvm-cov
            ];
        })
      craneLibVersions;
    in {
      checks = {
        inherit ohttp-relay;
      };

      packages.nginx-with-stream = nginxWithStream;
      packages.default = ohttp-relay;

      apps.default = flake-utils.lib.mkApp {
        drv = ohttp-relay;
      };

      devShells = devShells // {default = devShells.nightly;};

      formatter = pkgs.alejandra;
    });
}
