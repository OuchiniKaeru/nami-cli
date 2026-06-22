# AI Agent Framework v1 Design

## Goal

Build a Rust CLI AI agent framework that satisfies the v1 specification as a local-first, single-binary-friendly foundation. It must read `agent.yaml`, persist sessions as JSON/JSONL, run Python skills, expose MCP configuration, and execute a deterministic Planner -> Coder -> Reviewer workflow through an agent runtime abstraction.

## Scope

The v1 implementation includes:

- Rust CLI named `agent`
- YAML configuration loading from `agent.yaml`, with defaults when absent
- Session creation, resume, list, show, and delete
- Human-readable session storage under `.agent/sessions/<session-id>/`
- JSONL message and event persistence
- Python skill discovery and execution from `skills/<name>/`
- MCP server configuration parsing for `stdio`, `http`, and `websocket`
- Multi-agent workflow with `planner`, `coder`, and `reviewer`
- Deterministic local runtime implementation behind a trait, ready to be replaced by AutoAgents later

The v1 implementation does not include:

- SQLite, PostgreSQL, vector databases, GUI, cloud sync, remote execution, or distributed execution
- Real LLM calls
- Real MCP transport connections
- Packaging installer automation

## Architecture

The CLI is a thin layer over focused modules:

- `config`: loads and validates `agent.yaml`
- `session`: owns `.agent` directory layout and JSON/JSONL persistence
- `event`: defines event records and event writing
- `skill`: reads `skill.yaml`, invokes `python main.py request.json`, and parses JSON output
- `mcp`: parses and lists MCP server definitions
- `runtime`: defines `AgentRuntime` and provides deterministic local execution
- `workflow`: runs Planner -> Coder -> Reviewer and records messages, events, and state

The `AgentRuntime` trait is the boundary for future AutoAgents integration. The initial implementation is deterministic and local so the framework can be tested and used without credentials or network access.

## CLI

Commands:

- `agent chat`
  - Creates a new session, writes metadata, initial state, and `SESSION_CREATED`.
- `agent chat resume <session-id>`
  - Loads an existing session and updates `updated_at`.
- `agent session list`
  - Lists known sessions sorted by creation time.
- `agent session show <session-id>`
  - Prints metadata, state, message count, and event count.
- `agent session delete <session-id>`
  - Deletes the session directory.
- `agent skill run <name> --input <json>`
  - Runs `skills/<name>/main.py` with a generated request JSON file.
- `agent workflow run <task>`
  - Creates a session and runs Planner -> Coder -> Reviewer using the configured agents.
- `agent mcp list`
  - Prints configured MCP servers and transports.

## Persistence

Each session uses this layout:

```text
.agent/
  sessions/
    <session-id>/
      metadata.json
      messages.jsonl
      events.jsonl
      state.json
      artifacts/
  logs/
  cache/
  runtime/
```

`metadata.json` contains:

```json
{
  "session_id": "...",
  "project": "...",
  "created_at": "...",
  "updated_at": "..."
}
```

Messages are one JSON record per line:

```json
{"role":"user","content":"..."}
{"role":"assistant","agent":"planner","content":"..."}
{"role":"tool","name":"github","content":"..."}
```

Events are one JSON record per line:

```json
{"type":"SESSION_CREATED","timestamp":"..."}
{"type":"TASK_ASSIGNED","agent":"planner","timestamp":"..."}
```

`state.json` stores a map of agent name to status, for example:

```json
{
  "planner": { "status": "completed" },
  "coder": { "status": "completed" },
  "reviewer": { "status": "completed" }
}
```

## Skill Execution

Skills live under `skills/<name>/` and require:

- `skill.yaml`
- `main.py`

The runner writes a request file and executes:

```bash
python main.py request.json
```

The request contains:

```json
{
  "task_id": "...",
  "input": {}
}
```

The response must be JSON:

```json
{
  "success": true,
  "result": {
    "content": "..."
  }
}
```

Failures are returned as structured Rust errors and persisted as `TOOL_FAILED` events when run through framework commands.

## MCP

MCP v1 is configuration-level support. The framework parses:

- `stdio`
- `http`
- `websocket`

The manager exposes loaded server definitions and validates required fields such as `endpoint` for network transports.

## Multi-Agent Workflow

`workflow run` records the user task, then executes:

1. `planner`
2. `coder`
3. `reviewer`

Each agent receives the original task plus previous outputs. Each result is persisted as an assistant message tagged with the agent name. Events record assignment, start, finish, and completion. State is updated after every agent.

## Testing Strategy

Tests should cover:

- Default configuration when `agent.yaml` is absent
- YAML configuration parsing for list-style and map-style MCP definitions
- Session directory creation and JSON/JSONL writes
- Session list/show/delete behavior at the storage layer
- Skill metadata parsing and Python skill execution
- MCP validation
- Workflow event, message, and state persistence

Implementation follows test-first development. Each production behavior gets a failing test before implementation.
