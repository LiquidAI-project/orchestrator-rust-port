# orchestrator-rust-port
Rust version of https://github.com/LiquidAI-project/wasmiot-orchestrator

Currently under development and not in a usable state

## Usage (development)

When using devcontainer, it assumes that the wasmiot-net exists already, so either run ./wasmiot-network-create.sh or start the services once with docker compose up. Either of them creates the network. 

There are a couple of compose files and scripts, here are their purposes:

- `compose-express.yml` starts mongodb express for easy access to database, by default accessible from http://localhost:5000 , with default credentials being admin:admin
- `compose-supervisor.yml` starts a single rust supervisor instance, useful for testing the orchestrator
- `compose.yml` is the "main" compose file, this should start the orchestrator and database. This is also what you should use to start the database before starting devcontainer. Doing that also creates the wasmiot-net, so no need to run wasmiot-network-create separately.
- `wasmiot-network-create.sh` is a helper script that creates the wasmiot-net if it doesnt exist
- `mongodb-local-*.sh` are three different scripts for starting, stopping and cleaning/resetting a local non-docker instance of mongodb. They require user to install mongodb on their system themself though.
- `orchestrator-local-start.sh` is a helper script for building/running the orchestrator locally. This is what you should be calling when using devcontainer, or when creating a build to be used on some other device (this part is not implemented yet). Add `--help` at its end to see what flags you can use, by default it builds and runs the orchestrator.
