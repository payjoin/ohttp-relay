{
  description = "NGINX build with stream support";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-23.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }: 
    flake-utils.lib.eachDefaultSystem (system: 
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.nginx-with-stream = pkgs.stdenv.mkDerivation {
          name = "nginx-with-stream";
          src = pkgs.nginx.src;

          buildInputs = [ pkgs.openssl pkgs.pcre pkgs.zlib ];

          configureFlags = [
            "--with-stream"
            "--with-stream_ssl_module"
            "--error-log-path=/dev/null"
          ];
        };

        defaultPackage = self.packages.${system}.nginx-with-stream;
      }
    );
}