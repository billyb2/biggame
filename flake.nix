{
  description = "Build a cargo workspace";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-overlay.follows = "rust-overlay";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs =
    { self, nixpkgs, crane, flake-utils, advisory-db, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        inherit (pkgs) lib;

        rustToolchain =
          pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        src = craneLib.cleanCargoSource ./.;

        packages = with pkgs;
          [
            alsa-lib
            alsa-lib.dev

            mesa
            xorg.libX11
            xorg.libXinerama
            xorg.libXrandr
            xorg.libXcursor
            xorg.libXi
            mesa.dev
            mesa
            libglvnd.dev
            xorg.libXxf86vm
            wayland
            vulkan-tools
            vulkan-headers
            vulkan-loader
            vulkan-validation-layers
            libxkbcommon
            libxkbcommon.dev
            openssl.dev
            openssl
            pkg-config
            sccache
	    updog
          ] ++ lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];

        # Common arguments can be set here to avoid repeating them later
        commonArgs = {
          inherit src;
          strictDeps = true;

          nativeBuildInputs = packages;
        };

        # Build *just* the cargo dependencies (of the entire workspace),
        # so we can reuse all of that work (e.g. via cachix) when running in CI
        # It is *highly* recommended to use something like cargo-hakari to avoid
        # cache misses when building individual top-level-crates
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        individualCrateArgs = commonArgs // {
          inherit cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml { inherit src; }) version;
          # NB: we disable tests since we'll run them all via cargo-nextest
          doCheck = false;
        };

        fileSetForCrate = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./client
            ./server
            ./shared
          ];
        };

        # Build the top-level crates of the workspace as individual derivations.
        # This allows consumers to only depend on (and build) only what they need.
        # Though it is possible to build the entire workspace as a single derivation,
        # so this is left up to you on how to organize things
        game = craneLib.buildPackage (individualCrateArgs // {
          CARGO_BUILD_TARGET = "wasm32-unknown-unknown";

          pname = "bbmg-game";
          cargoExtraArgs = "-p bbmg-game";
          src = fileSetForCrate;
        });
        server = craneLib.buildPackage (individualCrateArgs // {
          pname = "bbmg-server";
          cargoExtraArgs = "-p bbmg-server";
          src = fileSetForCrate;
        });
      in {
        checks = {
          # Build the crates as part of `nix flake check` for convenience
          inherit game server;

          # Run clippy (and deny all warnings) on the workspace source,
          # again, reusing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          my-workspace-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          my-workspace-doc =
            craneLib.cargoDoc (commonArgs // { inherit cargoArtifacts; });

          # Check formatting
          my-workspace-fmt = craneLib.cargoFmt { inherit src; };

          my-workspace-toml-fmt = craneLib.taploFmt {
            src = pkgs.lib.sources.sourceFilesBySuffices src [ ".toml" ];
            # taplo arguments can be further customized below as needed
            # taploExtraArgs = "--config ./taplo.toml";
          };

          # Audit dependencies
          my-workspace-audit = craneLib.cargoAudit { inherit src advisory-db; };

          # Audit licenses
          my-workspace-deny = craneLib.cargoDeny { inherit src; };

          # Run tests with cargo-nextest
          # Consider setting `doCheck = false` on other crate derivations
          # if you do not want the tests to run twice
          my-workspace-nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });

          # Ensure that cargo-hakari is up to date
          my-workspace-hakari = craneLib.mkCargoDerivation {
            inherit src;
            pname = "my-workspace-hakari";
            cargoArtifacts = null;
            doInstallCargoArtifacts = false;

            buildPhaseCargoCommand = ''
              cargo hakari generate --diff  # workspace-hack Cargo.toml is up-to-date
              cargo hakari manage-deps --dry-run  # all workspace crates depend on workspace-hack
              cargo hakari verify
            '';

            nativeBuildInputs = [ pkgs.cargo-hakari ];
          };
        };

        packages = { inherit game server; };

        apps = {
          game = flake-utils.lib.mkApp { drv = game; };
          server = flake-utils.lib.mkApp { drv = server; };
        };

        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};

          buildInputs = packages;
          nativeBuildInputs = packages;

          LD_LIBRARY_PATH = builtins.concatStringsSep ":" [
            "${pkgs.xorg.libX11}/lib"
            "${pkgs.xorg.libXi}/lib"
            "${pkgs.libGL}/lib"
            "${pkgs.libxkbcommon}/lib"
          ];

          # Additional dev-shell environment variables can be set directly
          # MY_CUSTOM_DEVELOPMENT_VAR = "something else";

          packages = with pkgs; [ cargo-watch wasm-bindgen-cli ];
        };
      });
}
