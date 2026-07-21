{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      naersk,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
        nativeBuildInputs = with pkgs; [
          pkg-config
          makeBinaryWrapper
        ];
        buildInputs = with pkgs; [
          fontconfig
          libxkbcommon
          wayland
          vulkan-loader
        ];
      in
      {
        packages.default = naersk-lib.buildPackage {
          src = ./.;
          inherit nativeBuildInputs buildInputs;
          postInstall = ''
            wrapProgram $out/bin/bar \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath buildInputs}"
          '';
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;
          buildInputs =
            buildInputs
            ++ (with pkgs; [
              cargo
              rustc
              rustfmt
              pre-commit
            ]);
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
        };
      }
    );
}
