{
  description = "Build a cargo project without extra checks";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-23.11";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nginx-with-stream = {
      url = "path:./nginx-with-stream";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, nginx-with-stream, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        craneLib = crane.lib.${system};

        nginxWithStream = nginx-with-stream.packages.${system}.nginx-with-stream;

        ohttp-relay = craneLib.buildPackage {
          src = craneLib.cleanCargoSource (craneLib.path ./.);
          strictDeps = true;

          buildInputs = [
            nginxWithStream
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
            pkgs.darwin.apple_sdk.frameworks.Security
          ];

          preBuild = ''
            export PATH=${nginxWithStream}/bin:$PATH
          '';

          # Additional environment variables can be set directly
          # MY_CUSTOM_VAR = "some value";
        };
      in
      {
        checks = {
          inherit ohttp-relay;
        };

        packages.default = ohttp-relay;

        apps.default = flake-utils.lib.mkApp {
          drv = ohttp-relay;
        };

        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};

          # Additional dev-shell environment variables can be set directly
          # MY_CUSTOM_DEVELOPMENT_VAR = "something else";

          # Extra inputs can be added here; cargo and rustc are provided by default.
          packages = [
            nginxWithStream
          ];
        };
      });
}
