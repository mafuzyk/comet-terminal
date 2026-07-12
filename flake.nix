{
  description = "☄️ Comet Terminal — a modern, GPU-accelerated terminal emulator built in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        rustPlatform = pkgs.rustPlatform;

        comet = rustPlatform.buildRustPackage {
          pname = "comet-terminal";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          cargoBuildFlags = [ "--package" "comet" ];
          cargoTestFlags = [ "--workspace" ];

          nativeBuildInputs = with pkgs; [
            pkg-config
            makeWrapper
          ];

          buildInputs =
            with pkgs;
            [
              libxkbcommon
              fontconfig
              freetype
            ]
            ++ lib.optionals stdenv.isLinux [
              wayland
              vulkan-loader
              libglvnd
            ];

          preFixup = ''
            wrapProgram $out/bin/comet \
              --prefix LD_LIBRARY_PATH : ${
                with pkgs;
                lib.makeLibraryPath [
                  vulkan-loader
                  libglvnd
                  wayland
                  libxkbcommon
                ]
              }
          '';

          meta = with pkgs.lib; {
            description = "A modern, GPU-accelerated terminal emulator built in Rust";
            homepage = "https://github.com/mafuzyk/comet-terminal";
            license = licenses.mit;
            platforms = platforms.linux;
            maintainers = [ ];
            mainProgram = "comet";
          };
        };
      in
      {
        packages.default = comet;

        devShells.default = pkgs.mkShell {
          inputsFrom = [ comet ];
          packages = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
          ];
        };

        apps.default = {
          type = "app";
          program = "${comet}/bin/comet";
        };
      }
    );
}
