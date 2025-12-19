## About

This is a fully functional version of the [wasmiot orchestrator](https://github.com/LiquidAI-project/wasmiot-orchestrator ) written in rust. There are some differences between the two versions, that make them incompatible with each other. That means that you shouldnt try to switch from the original javascript version to rust version (or vice versa) without creating a new enviroment (new database, supervisors etc).

The main differences:
- Rust version of orchestrator has a different version of deployment datastructure, making it incompatible with the original wasmiot orchestrator as well as original wasmiot supervisor (the python version, found [here](https://github.com/LiquidAI-project/wasmiot-supervisor))
- Rust version uses revised user interface compared to the original one. It provides some functionalities that the original doesnt. The revised UI (often referred to as just "webgui" here) is also available to use with the original javascript orchestrator [here](https://github.com/LiquidAI-project/wasmiot-orchestrator-webgui) with some caveats. Read the instructions there carefully, as they tell how to roll back to a compatible version and how to set it up outside docker.
- Original javascript version of orchestrator supports some functionality that the rust version doesnt, mainly the failover functions. In the rust version that is partially implemented, to the point where the rust version keeps track of active and inactive devices.


## How to use

The easiest way to start using the rust orchestrator is to just download the [test enviroment](https://github.com/LiquidAI-project/wasmiot-test-env) repository, as that automatically sets up everything thats necessary. 

Without that, you have to set up your own database and supervisors. The orchestrator is functional without supervisors, but they are needed to be able to execute anything.

This repository has helper scripts to get you started. First, you should create a unique docker network to use the orchestrator in. Run the `./wasmiot-network-create.sh` script to do just that. Without doing that, you must adjust the network details in `.env` file as well as any docker compose files.

Then, you should copy the `.env.example` into `.env`, and adjust the variables in it to your liking.

After that, start the mongodb in the freshly created network by running `docker compose -f compose-mongo.yml up`. If you like, you can also start the mongo express for easier database exploration by running `docker compose -f compose-express.yml up`. This is not necessary for the orchestrator to work, but useful with development and debugging.

After the database is running, you have two options.

First option is starting the orchestrator locally (outside docker) by running `./orchestrator-local-start.sh`. This will however require you to handle all requirements yourself. To see what needs to be installed for orchestrator, check the `Dockerfile` for how final runtime image is built. Most important items are avahi and dbus. 

Second (recommended) option is starting the orchestartor in a container. It should suffice to just run `docker compose up --build` to get that working.

At this point, everything should be running correctly. By default, the orchestrator can be found at `http://localhost:3000`, and the mongo express in `http://localhost:5000` (with default credentials found in compose-express.yml).

Orchestrator has an API through which it can be used. It is explained in the API section. Orchestrator also has the web interface, and further instructions/examples on how that is meant to be used can be found in the [test enviroment](https://github.com/LiquidAI-project/wasmiot-test-env) repository README.

## Known issues

There is an unsolved issue with some raspberry pis, where the automatic device discovery doesnt work. For those situations, supervisors have a functionality to force manual registration.

Another issue happens with device discovery and docker. Sometimes, the device discovery starts failing (happens more with devcontainers) after rebooting the computer or container. One fix is to recreate the container, and another is to try running `rm -f /run/dbus/pid`. This issue relates to something that happens with dbus, and remains unsolved.

## Development

The easiest way is to use the devcontainer provided. Before starting it, you should create the wasmiot-network to docker by running `./wasmiot-network-create`, as the devcontainer assumes it already exists. 

Once the devcontainer is up and running, you might want to also start the mongodb and mongo express, as the database is needed to run the orchestrator, and express makes debugging much much easier. Instructions for these are in the **How to use** section of this README. 

Once the devcontainer is running, and database is up, start the orchestrator by running `./orchestrator-local-start.sh`. You can also run the orchestrator with cargo, but at least on first run you should use the script as it also builds the frontend UI. If the frontend needs to be rebuilt (like after updating the [wasmiot-orchestrator-webgui](https://github.com/LiquidAI-project/wasmiot-orchestrator-webgui) submodule), use the script with a flag `./orchestrator-local-start.sh --force-frontend-build`. By default, the script doesnt build the orchestrator in release mode as it takes too much time, but if you want to do that, run the command with flag `./orchestrator-local-start.sh --release`.

## API

Base URL: `http://<host>:<PUBLIC_PORT>` (default `http://localhost:3000`)

### **Device identity and health**

<details>
<summary>GET <code>/.well-known/wasmiot-device-description</code></summary>

Returns a machine-readable device description for the orchestrator (platform info + supported supervisor interfaces).
</details>

<details>
<summary>GET <code>/.well-known/wot-thing-description</code></summary>

Returns a W3C Web of Things Thing Description (WoT TD) for the orchestrator.
</details>

<details>
<summary>GET <code>/health</code></summary>

Returns a system-level health report for the orchestrator (CPU, memory, storage, network, uptime).
</details>


### **Devices**

<details>
<summary>GET <code>/file/device</code></summary>

Returns a list of all known devices from the database.
</details>

<details>
<summary>DELETE <code>/file/device</code></summary>

Deletes **all** known devices from the database.
</details>

<details>
<summary>GET <code>/file/device/{device_name}</code></summary>

Returns details for a single device by its **name**.

**Path params**
- `device_name` (string): Device name in the database.
</details>

<details>
<summary>DELETE <code>/file/device/{device_name}</code></summary>

Deletes a single device by its **name**.

**Path params**
- `device_name` (string): Device name in the database.
</details>

<details>
<summary>POST <code>/file/device/discovery/reset</code></summary>

Triggers a single mDNS scan immediately (does not wait for the periodic scan).
</details>

<details>
<summary>POST <code>/file/device/discovery/register</code></summary>

Manually registers a device (used by supervisors that want to register without relying on mDNS).

- **JSON payload**: [Manual device registration schema](schemas/manual-device-registration.schema.json)
</details>


### **Supervisor logs**

<details>
<summary>GET <code>/device/logs</code></summary>

Returns stored supervisor logs from the database.
</details>

<details>
<summary>POST <code>/device/logs</code></summary>

Stores a supervisor log entry into the database.

- The request body must be sent as `application/x-www-form-urlencoded` and include a field named `logData`, whose value is a JSON string matching the supervisor-log schema.
- **JSON payload**: [Supervisor log schema](schemas/supervisor-log.schema.json)
</details>


### **Modules**

<details>
<summary>POST <code>/file/module</code></summary>

Creates a new module record by uploading a WebAssembly (`.wasm`) file and a module name.

**Content-Type**
- `multipart/form-data`

**Multipart fields**
- `name` (text, required): User-defined name for the module.
- `<wasm file field>` (file, required): A `.wasm` file upload.
  - The file part must have `Content-Type: application/wasm`.

**Notes**
- Exactly one wasm file is required (the server picks the first multipart file where `mimetype == "application/wasm"`).
- The response contains the created module id.
</details>

<details>
<summary>GET <code>/file/module</code></summary>

Returns a list of all modules.
</details>

<details>
<summary>DELETE <code>/file/module</code></summary>

Deletes **all** modules.
</details>

<details>
<summary>GET <code>/file/module/{module_id}</code></summary>

Returns a specific module by id.

**Path params**
- `module_id` (string): Module id.
</details>

<details>
<summary>DELETE <code>/file/module/{module_id}</code></summary>

Deletes a specific module by id.

**Path params**
- `module_id` (string): Module id.
</details>

<details>
<summary>POST <code>/file/module/{module_id}/upload</code></summary>

Uploads/updates a module description and optional mount files for a specific module.

**Path params**
- `module_id` (string): Module ObjectId hex string, or module name.

**Content-Type**
- `multipart/form-data`

**Multipart format**
This endpoint expects a very specific field naming convention. Fields are parsed from keys that look like:

- `{func}[method]` (text): HTTP method for the function (default: `GET`).
- `{func}[output]` (text): Output type for the function.
  - If the function defines an output mount (stage `output`) with a non-`application/octet-stream` media type, that media type is used as output type instead.
- `{func}[param0]`, `{func}[param1]`, ... (text): Parameter types (all are treated as required query params in generated OpenAPI).

Mounts are described using indexed entries:
- `{func}[mounts][0][name]` (text): Mount name (must match an uploaded file field name, if stage is `deployment`).
- `{func}[mounts][0][stage]` (text): Mount stage. Expected values: `deployment`, `execution`, `output`.

**Multipart files**
- Any non-wasm file parts are treated as module data files (mount files) and are saved to disk.
- File part name (field name) is important: mounts refer to files by this **field name**.

**Validation rules**
- If a function declares a mount with `stage=deployment`, the corresponding file must be included in the multipart upload (except for the special `wasmiot_init` function exception).
</details>

<details>
<summary>GET <code>/file/module/{module_id}/description</code></summary>

Returns the module description for a specific module.

**Path params**
- `module_id` (string): Module id.
</details>

<details>
<summary>GET <code>/file/module/{module_id}/wasm</code></summary>

Returns the module’s `.wasm` binary.

**Path params**
- `module_id` (string): Module id.
</details>

<details>
<summary>GET <code>/file/module/{module_id}/{file_name}</code></summary>

Returns an additional data file associated with the module (by file name).

**Path params**
- `module_id` (string): Module id.
- `file_name` (string): File name as stored/associated with the module.
</details>


### **Deployments / Manifests**

<details>
<summary>GET <code>/file/manifest</code></summary>

Returns a list of all deployments/manifests.
</details>

<details>
<summary>POST <code>/file/manifest</code></summary>

Creates a new deployment/manifest.

- **JSON payload**: [Create deployment schema](schemas/create-deployment.schema.json)
</details>

<details>
<summary>DELETE <code>/file/manifest</code></summary>

Deletes **all** deployments/manifests.
</details>

<details>
<summary>GET <code>/file/manifest/{deployment_id}</code></summary>

Returns a specific deployment/manifest.

**Path params**
- `deployment_id` (string): Deployment/manifest id.
</details>

<details>
<summary>POST <code>/file/manifest/{deployment_id}</code></summary>

Deploys a specific deployment/manifest to the relevant supervisor(s) (pushes required files / configuration).

**Path params**
- `deployment_id` (string): Deployment/manifest id.

**Input**
- TODO: document expected behavior/payload (see `http_deploy`)
</details>

<details>
<summary>PUT <code>/file/manifest/{deployment_id}</code></summary>

Updates a specific deployment/manifest. **Not implemented**.
</details>

<details>
<summary>DELETE <code>/file/manifest/{deployment_id}</code></summary>

Deletes a specific deployment/manifest.

**Path params**
- `deployment_id` (string): Deployment/manifest id.
</details>


### **Execution**

<details>
<summary>POST <code>/execute/{deployment_id}</code></summary>

Executes a deployment/manifest (assumes it has been deployed previously).

**Path params**
- `deployment_id` (string): Deployment/manifest id.

**Input**
- Input is dynamic, required keys come from the first step of the deployment’s generated endpoint definition (`fullManifest.sequence[0].endpoint.request.parameters`). Optional file parts come from the first step’s `requestBody` (if present) and its multipart schema.
</details>


### **Data source cards**

<details>
<summary>GET <code>/dataSourceCards</code></summary>

Returns all data source cards.
</details>

<details>
<summary>POST <code>/dataSourceCards</code></summary>

Creates a new data source card.

**Content-Type**
- `application/json`

**Body**
A JSON object that contains:

- `asset`: array (**required**, must contain at least one element)
  - `asset[0].title`: string (optional, defaults to `"unknown"`)
  - `asset[0].relation`: array (optional)

Each object in `asset[0].relation` is expected to have:
- `type`: string
- `value`: string

From `relation`, the orchestrator looks up values by `type`:

- `type == "type"` → datasource type (defaults to `"unknown"`)
- `type == "risk-level"` → risk level (defaults to `"unknown"`)
- `type == "nodeid"` → **required**, must be a valid MongoDB ObjectId hex string

All other fields in the JSON payload are ignored.
</details>

<details>
<summary>DELETE <code>/dataSourceCards</code></summary>

Deletes **all** data source cards.
</details>

<details>
<summary>DELETE <code>/dataSourceCards/{node_id}</code></summary>

Deletes a specific data source card by node id.

**Path params**
- `node_id` (string): Node id (MongoDB ObjectId hex string)
</details>


### **Module cards**

<details>
<summary>GET <code>/moduleCards</code></summary>

Returns all module cards.

**Query params (optional)**
- `after` (string, RFC3339): Return only cards received after this timestamp (e.g. `2025-08-12T12:00:00Z`).
</details>

<details>
<summary>POST <code>/moduleCards</code></summary>

Creates a new module card.

**Content-Type**
- `application/json`

**Body**
Expects an ODRL-like JSON object containing:

- `permission`: array (**required**, must contain at least one element)
  - `permission[0].target`: string (**required**)  
    - Must be a valid MongoDB ObjectId hex string (this becomes `moduleid`)
  - `permission[0].action`: string (**required**)  
    - Stored as the card `name`
  - `permission[0].constraint`: array (**required**)

From `permission[0].constraint`, the orchestrator looks up values by `leftOperand` and takes `rightOperand`:

- `leftOperand == "risk-level"` → `risk_level` (defaults to `""`)
- `leftOperand == "input-type"` → `input_type` (defaults to `""`)
- `leftOperand == "output-risk"` → `output_risk` (defaults to `""`)

All other fields in the JSON payload are ignored.

**Notes**
- Cards are inserted as new documents (not upserted). Posting the same data multiple times creates multiple cards.
</details>

<details>
<summary>DELETE <code>/moduleCards</code></summary>

Deletes **all** module cards.
</details>

<details>
<summary>DELETE <code>/moduleCards/{card_id}</code></summary>

Deletes a module card by **module id**.

**Path params**
- `card_id` (string): Module ObjectId hex string (this is matched against `moduleid` in stored cards)
</details>


### **Node cards**

<details>
<summary>GET <code>/nodeCards</code></summary>

Returns all node cards.

**Query params (optional)**
- `after` (string, RFC3339): Return only cards received after this timestamp (e.g. `2025-08-12T12:00:00Z`).
</details>

<details>
<summary>POST <code>/nodeCards</code></summary>

Creates or updates a node card (upsert by `nodeid`).

**Content-Type**
- `application/json`

**Body**
Expects a JSON object containing:

- `asset`: array (**required**, must contain at least one element)
  - `asset[0].title`: string (optional, defaults to `"unknown"`) → stored as `name`
  - `asset[0].uid`: string (optional, defaults to `"unknown"`) → stored as `nodeid`
  - `asset[0].relation`: array (optional)
    - Zone is extracted from the first relation where:
      - `type == "memberOf"`
      - `value` is used as the `zone` (defaults to `"unknown"`)

All other fields in the JSON payload are ignored.

**Notes**
- The database write is an upsert: an existing card with the same `nodeid` is replaced.
</details>

<details>
<summary>DELETE <code>/nodeCards</code></summary>

Deletes **all** node cards.
</details>

<details>
<summary>DELETE <code>/nodeCards/{card_id}</code></summary>

Deletes a node card by `nodeid`.

**Path params**
- `card_id` (string): Node id (matched against `nodeid` in stored cards)
</details>


### **Zones and risk levels**

<details>
<summary>GET <code>/zoneRiskLevels</code></summary>

Returns the current zone ↔ risk-level configuration.

**Response**
- `zones`: array of objects:
  - `zone` (string)
  - `allowed_risk_levels` (string[])
- `riskLevels`: object or `null`:
  - `levels` (string[])
  - `last_updated` (RFC3339 timestamp)
</details>

<details>
<summary>POST <code>/zoneRiskLevels</code></summary>

Creates/updates the zone and risk-level configuration.

**Content-Type**
- `application/json`

**Body**
Expects an ODRL-like JSON object containing:

- `permission`: array (optional)

Each `permission[i]` is interpreted as:
- `permission[i].target`: string → treated as a **risk level** value (defaults to `"unknown"` if missing)
- `permission[i].constraint`: array (optional)

Zones are extracted from constraints where:
- `leftOperand == "zone"`
- `rightOperand` can be:
  - a string → single zone
  - an array of strings → multiple zones

For each extracted zone, the risk level from `permission[i].target` is added to that zone’s `allowed_risk_levels`.

Additionally, the orchestrator stores a separate `riskLevels` document:
- `levels`: unique set of all risk levels observed from `permission[*].target`
- `last_updated`: server time

**Notes**
- Posting updates will upsert per-zone documents.
- Zones and the riskLevels list are persisted into the same collection.
</details>

<details>
<summary>DELETE <code>/zoneRiskLevels</code></summary>

Deletes **all** zones and risk-level configuration.
</details>


### **Deployment certificates**

<details>
<summary>GET <code>/deploymentCertificates</code></summary>

Returns all deployment certificates (generated by orchestrator, not user-created).
</details>

<details>
<summary>DELETE <code>/deploymentCertificates</code></summary>

Deletes **all** deployment certificates.
</details>

<details>
<summary>DELETE <code>/deploymentCertificates/{deployment_id}</code></summary>

Deletes a specific deployment certificate.

**Path params**
- `deployment_id` (string): Deployment id.
</details>


### **Import / Export**

<details>
<summary>GET <code>/export</code></summary>

Exports current orchestrator setup into the `init` folder format.
</details>

<details>
<summary>GET <code>/import</code></summary>

Imports orchestrator setup from the `init` folder.
</details>


### **Misc**

<details>
<summary>POST <code>/postResult</code></summary>

Placeholder endpoint (currently returns an empty array). Intended for posting intermediary results in longer chains.
</details>
