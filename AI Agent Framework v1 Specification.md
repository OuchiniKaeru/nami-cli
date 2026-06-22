# AI Agent Framework v1 Specification

## 1. Goals

本システムは以下を目的とする。

* ローカルファースト
* 単一EXE配布
* 高速起動
* Multi-Agent
* MCP対応
* A2A対応
* Python Skill拡張
* プロジェクト単位管理
* セッション永続化
* 人間が読める保存形式

v1ではデータベースを使用しない。

永続化は JSON / JSONL に統一する。

---

# 2. Architecture

```text
┌─────────────────────────────┐
│         agent.exe           │
│          Rust               │
└─────────────┬───────────────┘
              │
              ▼

┌─────────────────────────────┐
│         AutoAgents          │
│      Runtime Engine         │
└─────────────┬───────────────┘
              │

 ┌────────────┼─────────────┐
 ▼            ▼             ▼

Session    MCP Manager   Skill Manager

             │
             ▼

      Python Skill Runtime
```

---

# 3. Technology Stack

## Agent Core

Language:

Rust

主要ライブラリ:

```toml
tokio
serde
serde_json
serde_yaml
clap
anyhow
uuid
chrono
```

---

## Agent Runtime

AutoAgents

---

## Skill Runtime

Language:

Python

推奨:

```text
uv
pydantic
mcp
httpx
```

---

# 4. Project Layout

```text
project/

├── agent.yaml

├── skills/

├── prompts/

├── memory/

└── .agent/

    ├── sessions/

    ├── cache/

    ├── logs/

    └── runtime/
```

---

# 5. Session Storage

Sessionごとにディレクトリを生成する。

```text
.agent/

└── sessions/

    └── 01JYABCD1234/

        ├── metadata.json

        ├── messages.jsonl

        ├── events.jsonl

        ├── state.json

        └── artifacts/
```

---

# 6. metadata.json

```json
{
  "session_id": "01JYABCD1234",
  "project": "ai-kanban",
  "created_at": "2026-06-20T10:00:00Z",
  "updated_at": "2026-06-20T10:30:00Z"
}
```

---

# 7. messages.jsonl

会話履歴を保存する。

1行1レコード。

```json
{"role":"user","content":"READMEを書いて"}
{"role":"assistant","content":"READMEのドラフトを作成します"}
{"role":"tool","name":"github","content":"README.md generated"}
```

---

# 8. events.jsonl

イベント履歴を保存する。

```json
{"type":"SESSION_CREATED"}
{"type":"TASK_CREATED"}
{"type":"TASK_ASSIGNED"}
{"type":"TOOL_STARTED"}
{"type":"TOOL_FINISHED"}
{"type":"TASK_COMPLETED"}
```

---

# 9. state.json

現在状態を保持する。

```json
{
  "planner": {
    "status": "running"
  },
  "coder": {
    "status": "waiting"
  }
}
```

---

# 10. Artifact Storage

生成物を保存する。

```text
artifacts/

├── README.md

├── architecture.md

└── generated/
```

---

# 11. Session Commands

## New Session

```bash
agent chat
```

新規Session生成。

---

## Resume Session

```bash
agent chat resume <session-id>
```

---

## List Sessions

```bash
agent session list
```

---

## Show Session

```bash
agent session show <session-id>
```

---

## Delete Session

```bash
agent session delete <session-id>
```

---

# 12. YAML Configuration

## agent.yaml

```yaml
project:
  name: ai-kanban

model:
  provider: gemini
  model: gemini-3

skills:
  - github
  - browser

mcp:
  - filesystem
  - github

agents:

  planner:
    model: gemini
    skills:
      - browser

  coder:
    model: claude
    skills:
      - github

  reviewer:
    model: gemini
    skills:
      - github
```

---

# 13. Skill System

## Skill Structure

```text
skills/

└── github/

    ├── skill.yaml

    └── main.py
```

---

## skill.yaml

```yaml
name: github
version: 0.1.0

description: GitHub operations

permissions:
  network: true
  filesystem: false
```

---

## Execution

Agent Core:

```bash
python main.py request.json
```

---

Input:

```json
{
  "task_id": "123",
  "input": {
    "query": "create README"
  }
}
```

Output:

```json
{
  "success": true,
  "result": {
    "content": "README created"
  }
}
```

---

# 14. MCP Manager

サポート:

```text
stdio
http
websocket
```

設定例:

```yaml
mcp:

  filesystem:
    transport: stdio

  github:
    transport: http
    endpoint: http://localhost:8080
```

---

# 15. Multi-Agent

v1では以下をサポート。

```text
Planner
Coder
Reviewer
```

ワークフロー:

```text
User

 ↓

Planner

 ↓

Coder

 ↓

Reviewer

 ↓

Result
```

---

# 16. Event Bus

内部イベント形式:

```json
{
  "event": "TOOL_STARTED",
  "agent": "coder",
  "timestamp": "..."
}
```

利用目的:

* ログ
* 状態管理
* 将来のGUI

---

# 17. Logging

保存場所:

```text
.agent/logs/
```

形式:

```json
{
  "level":"INFO",
  "message":"Task completed"
}
```

---

# 18. Future v2

以下はv1対象外。

* SQLite
* PostgreSQL
* Vector DB
* GUI
* Remote Agent
* Cloud Sync
* Distributed Execution

---

# 19. Success Criteria

v1完了条件

✓ AutoAgentsでAgent実行

✓ YAML読込

✓ Python Skill実行

✓ MCP利用

✓ Session保存

✓ Session再開

✓ Multi-Agent実行

✓ JSONLイベント保存

✓ 単一EXE配布

```
```
