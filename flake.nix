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
      name = "paperwing";
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
          meta.mainProgram = name;
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
              clippy
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
          name = "paperwing";
          cfgStylix = config.stylix.targets."${name}";
          cfg = config.programs."${name}";
          hasStylix = ((options ? stylix) && config.stylix.enable);
          useStylix = hasStylix && cfgStylix.enable;
          c = config.lib.stylix.colors;
          rgb = color: [
            (builtins.fromJSON c."${color}-dec-r")
            (builtins.fromJSON c."${color}-dec-g")
            (builtins.fromJSON c."${color}-dec-b")
          ];
        in
        {
          options = {
            programs."${name}" = {
              enable = lib.mkEnableOption name;

              primaryColor = lib.mkOption {
                default = if useStylix && cfgStylix.colors.enable then rgb "base07" else null;
                type = lib.types.nullOr (lib.types.listOf lib.types.float);
              };

              backgroundColor = lib.mkOption {
                default = if useStylix && cfgStylix.colors.enable then rgb "base00" else null;
                type = lib.types.nullOr (lib.types.listOf lib.types.float);
              };

              opacity = lib.mkOption {
                default = if useStylix && cfgStylix.opacity.enable then config.stylix.opacity.desktop else null;
                type = lib.types.nullOr lib.types.float;
              };

              font = lib.mkOption {
                default = if useStylix then config.stylix.fonts.monospace.name else null;
                type = lib.types.nullOr lib.types.str;
              };

              display = lib.mkOption {
                type = lib.types.nullOr lib.types.str;
                default = null;
              };
            };

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

          config = lib.mkIf cfg.enable {
            home.packages = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
            xdg.configFile."${name}/config.json".text = (
              builtins.toJSON (
                (if cfg.primaryColor != null then { primary = cfg.primaryColor; } else { })
                // (
                  if cfg.font != null then
                    {
                      font = cfg.font;
                    }
                  else
                    { }
                )
                // (
                  if cfg.backgroundColor != null && cfg.opacity != null then
                    { background = cfg.backgroundColor ++ [ cfg.opacity ]; }
                  else if cfg.backgroundColor != null then
                    { background = cfg.backgroundColor; }
                  else
                    { }
                )
                // (if cfg.display != null then { display = cfg.display; } else { })
              )
            );
          };
        };
    };
}
