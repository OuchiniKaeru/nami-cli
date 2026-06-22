# AI Agent Framework v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI agent framework with YAML config, JSON/JSONL sessions, Python skills, MCP config support, and deterministic Planner -> Coder -> Reviewer workflow.

**Architecture:** The CLI delegates to focused modules for config, sessions, events, skills, MCP, runtime, and workflow. External agent execution is isolated behind an `AgentRuntime` trait so AutoAgents can replace the deterministic runtime later.

**Tech Stack:** Rust, tokio, serde, serde_json, serde_yaml, clap, anyhow, uuid, chrono, tempfile for tests.

---

### Task 1: Project Skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`

- [ ] **Step 1: Create the crate manifest and module shell**

Create a binary crate named `agent` with a library target. Include dependencies from the specification plus `async-trait` and `tempfile`.

- [ ] **Step 2: Add empty modules through `src/lib.rs`**

Expose `config`, `event`, `mcp`, `runtime`, `session`, `skill`, and `workflow`.

- [ ] **Step 3: Run `cargo test`**

Expected: compile succeeds with no tests.

### Task 2: Configuration

**Files:**
- Create: `src/config.rs`
- Test: inline unit tests in `src/config.rs`

- [ ] **Step 1: Write failing tests**

Test default config loading when `agent.yaml` is absent, project/model parsing, agent definitions, list-style MCP, and map-style MCP.

- [ ] **Step 2: Implement `Config::load_from`**

Read `agent.yaml` from a project root when present, otherwise return defaults.

- [ ] **Step 3: Run config tests**

Expected: all config tests pass.

### Task 3: Session Storage and Events

**Files:**
- Create: `src/event.rs`
- Create: `src/session.rs`
- Test: inline unit tests in `src/session.rs`

- [ ] **Step 1: Write failing tests**

Test session creation, metadata persistence, message append, event append, state read/write, session listing, showing counts, and deletion.

- [ ] **Step 2: Implement storage**

Create `.agent` directories, write pretty JSON where appropriate, append JSONL records, and count JSONL lines.

- [ ] **Step 3: Run session tests**

Expected: all session tests pass.

### Task 4: MCP Manager

**Files:**
- Create: `src/mcp.rs`
- Test: inline unit tests in `src/mcp.rs`

- [ ] **Step 1: Write failing tests**

Test listing configured MCP servers and rejecting `http`/`websocket` entries without endpoints.

- [ ] **Step 2: Implement manager**

Expose `McpManager::from_config`, `servers`, and `validate`.

- [ ] **Step 3: Run MCP tests**

Expected: all MCP tests pass.

### Task 5: Python Skills

**Files:**
- Create: `src/skill.rs`
- Test: inline unit tests in `src/skill.rs`

- [ ] **Step 1: Write failing tests**

Test skill metadata parsing and executing a temporary Python skill that returns JSON.

- [ ] **Step 2: Implement skill runner**

Write request JSON, invoke `python main.py request.json`, capture stdout/stderr, and parse `SkillResponse`.

- [ ] **Step 3: Run skill tests**

Expected: all skill tests pass, or fail clearly if Python is unavailable.

### Task 6: Runtime and Workflow

**Files:**
- Create: `src/runtime.rs`
- Create: `src/workflow.rs`
- Test: inline unit tests in `src/workflow.rs`

- [ ] **Step 1: Write failing tests**

Test that `workflow run` persists the user message, three assistant messages, assignment events, completion events, and completed state for planner/coder/reviewer.

- [ ] **Step 2: Implement deterministic runtime and workflow**

Add `AgentRuntime`, `AgentInput`, `AgentOutput`, `DeterministicRuntime`, and `WorkflowRunner`.

- [ ] **Step 3: Run workflow tests**

Expected: workflow tests pass.

### Task 7: CLI

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Wire clap commands**

Add `chat`, `chat resume`, `session list/show/delete`, `skill run`, `workflow run`, and `mcp list`.

- [ ] **Step 2: Run full verification**

Run `cargo fmt`, `cargo test`, and `cargo check`.

Expected: formatting succeeds, tests pass, and compile check succeeds.
