{
  description = "open-subtitle — keyless, agnostic subtitle engine (find, score, download, sync, translate, transcribe)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  # Prebuilt closures are pushed to the grok-insider cachix cache by CI, so consumers
  # never compile open-subtitle. (Reuses the same cache as open-media.)
  nixConfig = {
    extra-substituters = [
      "https://grok-insider.cachix.org"
      "https://nix-community.cachix.org"
    ];
    extra-trusted-public-keys = [
      "grok-insider.cachix.org-1:ZxLVOxJ1CjdY3vQl1I99qCtwNZwIU4+/QwqSvntB/5w="
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
    ];
  };

  outputs = { self, nixpkgs }:
    let
      # Nix/cachix targets x86_64-linux only (like open-media). Prebuilt binaries
      # for aarch64-linux, macOS, and Windows come from the GitHub Release matrix
      # (cargo-zigbuild / native runners), not from Nix.
      systems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      # One package builds both binaries (`ost`, `ostd`), the C-ABI library
      # (`libopensubtitle`), and bundles the mpv plugin. `default` aliases it.
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          lib = nixpkgs.lib;
          version = (lib.importTOML ./Cargo.toml).workspace.package.version;

          open-subtitle = pkgs.rustPlatform.buildRustPackage {
            pname = "open-subtitle";
            inherit version;
            src = ./.;

            # No git dependencies in the workspace, so the lockfile alone is
            # enough — no outputHashes needed.
            cargoLock.lockFile = ./Cargo.lock;

            # Build the binaries and the FFI library. TLS is rustls + ring, so
            # there is NO openssl/aws-lc/cmake/bindgen native build glue: the
            # stdenv `cc` (for ring) is all that's required.
            cargoBuildFlags = [ "-p" "os-cli" "-p" "os-daemon" "-p" "os-ffi" ];

            # Hermetic tests run in CI's `rust` job; skip here to keep the build
            # lean (no wiremock/test-only compiles).
            doCheck = false;

            # buildRustPackage installs the two binaries automatically. Add the
            # FFI library + header, and the mpv plugin asset.
            postInstall = ''
              mkdir -p $out/lib $out/include $out/share/open-subtitle/mpv
              find target -path '*release*' \
                \( -name 'libopensubtitle.so' -o -name 'libopensubtitle.a' \) \
                -exec cp {} $out/lib/ \;
              cp crates/os-ffi/include/opensubtitle.h $out/include/
              cp crates/os-mpv/open-subtitle.lua $out/share/open-subtitle/mpv/
            '';

            meta = {
              description = "Keyless, agnostic subtitle engine: ost (CLI) + ostd (daemon) + libopensubtitle";
              homepage = "https://github.com/grok-insider/open-subtitle";
              license = lib.licenses.mit;
              mainProgram = "ost";
              platforms = systems;
            };
          };
        in
        {
          inherit open-subtitle;
          default = open-subtitle;
        });

      # Home Manager module: installs `ost`/`ostd` (prebuilt from the cache), and
      # optionally deploys the mpv plugin and/or runs `ostd` as a user service.
      #
      # Secrets (`~/.config/open-subtitle/config.toml`) are intentionally NOT
      # managed here — keys must never enter the Nix store. Keyless providers
      # work out of the box; configure the rest at runtime with `ost`.
      homeManagerModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.programs.open-subtitle;
          pkgsFor = self.packages.${pkgs.stdenv.hostPlatform.system};
        in
        {
          options.programs.open-subtitle = {
            enable = lib.mkEnableOption "open-subtitle (ost/ostd)";
            package = lib.mkOption {
              type = lib.types.package;
              default = pkgsFor.default;
              defaultText = lib.literalExpression "open-subtitle.packages.\${system}.default";
              description = "The open-subtitle package providing `ost` and `ostd`.";
            };
            languages = lib.mkOption {
              type = lib.types.listOf lib.types.str;
              default = [ "en" ];
              description = "Default subtitle language preference (used by the mpv plugin config).";
            };
            mpv.enable = lib.mkEnableOption "deploy the open-subtitle mpv plugin into ~/.config/mpv";
            daemon = {
              enable = lib.mkEnableOption "run ostd (the HTTP/JSON daemon) as a systemd user service";
              address = lib.mkOption {
                type = lib.types.str;
                default = "127.0.0.1:4110";
                description = "Address ostd binds to (OSTD_ADDR).";
              };
            };
          };

          config = lib.mkIf cfg.enable (lib.mkMerge [
            { home.packages = [ cfg.package ]; }

            (lib.mkIf cfg.mpv.enable {
              xdg.configFile."mpv/scripts/open-subtitle.lua".source =
                "${cfg.package}/share/open-subtitle/mpv/open-subtitle.lua";
              xdg.configFile."mpv/script-opts/open-subtitle.conf".text = ''
                ost_path=${cfg.package}/bin/ost
                languages=${lib.concatStringsSep "," cfg.languages}
              '';
            })

            (lib.mkIf cfg.daemon.enable {
              systemd.user.services.ostd = {
                Unit = {
                  Description = "open-subtitle daemon (ostd)";
                  After = [ "network.target" ];
                };
                Service = {
                  ExecStart = "${cfg.package}/bin/ostd";
                  Environment = [ "OSTD_ADDR=${cfg.daemon.address}" ];
                  Restart = "on-failure";
                };
                Install.WantedBy = [ "default.target" ];
              };
            })
          ]);
        };

      # Dev shell: the Rust toolchain (+ mpv for plugin testing). ring needs only
      # the stdenv C compiler — no cmake/clang.
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            name = "open-subtitle-dev";
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              mpv
            ];
          };
        });
    };
}
