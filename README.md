# orchestrator-rust-port
Rust version of https://github.com/LiquidAI-project/wasmiot-orchestrator
## Usage (with cargo)
Running with cargo requires installation of clang package (for example `sudo apt install clang`)

Copy the .env.example into .env, and change the variables to your liking. 

Finally, run locally with `cargo run`. When building for first time, it will download and build the supervisor module as well, which means you may have to input your github ssh-key password at some point depending on your setup.
## Development
Use `./build_frontend.sh` to update the frontend code if there are changes in the wasmiot-orchestrator-webgui repo.

Use `cargo update -p supervisor` to update the supervisor module if there are changes to it at some point.