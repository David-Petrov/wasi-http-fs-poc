# Goal of the PoC
This is an attempt at combining the `wasi:http/proxy` world with `wasi:filesystem` in order to implement a http server with filesystem access within a wasm component (`plugin1`), which can then be embeded in a host environment (`host`).

The plugin part is a modification of [this example](https://github.com/sunfishcode/hello-wasi-http).

The host part is a modification of [this test case](https://github.com/bytecodealliance/wasmtime/blob/9f29c6e92629a8552f57fa6b2cec1371bc34f9e8/crates/wasi-http/tests/all/main.rs#L205).

# Usage
## Necessary dependencies

- Using nix:
[Install nix](https://github.com/DeterminateSystems/nix-installer) and then `nix develop` from the project root (`.envrc` for `direnv` is also provided; for easily achieving essentialy the same).
- Manually:
Aside from the rust toolchain defined in `rust-toolchain.toml` (automatically picked up and downloaded by `rustup`), you'll need to install [`cargo-component`](https://github.com/bytecodealliance/cargo-component)

## Build & run
1. Build the plugin using `cargo-component`:
   ```
   cd ./plugin1
   cargo component build
   ```
2. Run the host binary, which loads the built plugin component and sends a simple ping request to it:
   ```
   cd ./host
   cargo run
   ```
