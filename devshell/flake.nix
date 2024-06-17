{
  inputs = {
    nixpkgs = {
      url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    };

    flake-utils = {
      url = "github:numtide/flake-utils";
    };

    foundry = {
      url = "github:shazow/foundry.nix/monthly";
    };
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    , foundry
    , ...
    } @ inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            foundry.overlay
          ];
          config.permittedInsecurePackages = [
          ];
          # config.allowBroken = true;
        };
      in
      {
        devShells = {
          default = pkgs.mkShell {
            # Native project dependencies
            nativeBuildInputs = with pkgs; [
              # Git
              git

              # Rust
              rustup
              # cargo-nextest
              # cargo-expand
              # cargo-make

              # Solidity
              solc-select
              foundry-bin # forge, cast, anvil, chisel

              # Libs
              libiconv

              # Wasm stuff
              wasmtime
              cargo-component
            ] ++ lib.optionals stdenv.isDarwin (with darwin.apple_sdk.frameworks; [
              SystemConfiguration
            ]);

            # Libraries for the local builder
            buildInputs = with pkgs; [
            ];

            shellHook = ''
              export PATH="$HOME/.cargo/bin:$PATH"
            '';
          };
        };
      }
    );
}
