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
    let
      name = "bar";
    in
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
            wrapProgram $out/bin/${name}\
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
    )
    // {
      homeModules.default =
        {
          config,
          options,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.stylix.targets."${name}";
          hasStylix = ((options ? stylix) && config.stylix.enable);
          useStylix = hasStylix && cfg.enable;
          opacity = config.stylix.opacity;
          c = config.lib.stylix.colors;
          rgb = color: [
            (lib.trivial.fromHexString c."${color}-rgb-r" / 255.)
            (lib.trivial.fromHexString c."${color}-rgb-g" / 255.)
            (lib.trivial.fromHexString c."${color}-rgb-b" / 255.)
          ];
        in
        {
          options = {
            programs."${name}".enable = lib.mkEnableOption name;

            stylix.targets."${name}" = {
              enable = lib.mkOption {
                default = config.stylix.autoEnable;
              };
              colors.enable = lib.mkOption {
                default = config.stylix.autoEnable;
              };
              opacity.enable = lib.mkOption {
                default = config.stylix.autoEnable;
              };
            };
          };

          config = lib.mkIf (config.programs.${name}.enable && useStylix) {
            home.packages = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
            xdg.configFile."${name}/config.json".text = (
              builtins.toJSON (
                {
                  primary = config.lib.stylix.colors.base0D;
                  font = config.stylix.fonts.monospace.name;
                }
                // (
                  if cfg.colors.enable && cfg.opacity.enable then
                    { background = rgb "base00" ++ [ opacity.desktop ]; }
                  else if cfg.colors.enable then
                    { background = rgb "base00"; }
                  else
                    { }
                )
              )
            );
          };
        };
      homeModules."${name}" = self.homeModules.default;
    };
}
