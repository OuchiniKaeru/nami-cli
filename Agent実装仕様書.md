# Rust AI Agent Framework 実装仕様書 v1.1

## 1. 概要

### 目的

Rustのみで動作する軽量なCLI AIエージェントを実装する。

### コンセプト

* Python不要
* 単一EXEで配布
* OpenAI互換APIを中心に設計
* Gemini対応
* MCP対応
* Skills対応
* YAML設定
* JSONLメモリ
* セッション保存
* 軽量・高速・拡張性重視

---

# 2. 対応LLM

## OpenAI互換API

同一実装で以下を利用可能とする。

* OpenAI
* Azure OpenAI
* OpenRouter
* Groq
* Anthropic
* その他OpenAI互換API

## Gemini

Geminiのみ専用Providerを実装する。

---

# 3. システム構成

```
src/

main.rs

config/
provider/
agent/
memory/
session/
skill/
mcp/
models/
utils/
```

---

# 4. ディレクトリ構成

```
src/

main.rs

config/
    mod.rs

provider/
    mod.rs
    openai.rs
    gemini.rs

agent/
    mod.rs
    executor.rs
    planner.rs
    tool_router.rs

memory/
    mod.rs
    jsonl_store.rs

session/
    mod.rs

skill/
    mod.rs
    registry.rs

    filesystem.rs
    shell.rs
    browser.rs
    search.rs
    http.rs

mcp/
    mod.rs
    client.rs

models/
    chat.rs
    config.rs
    message.rs
    metrics.rs
    tool.rs

utils/
    logger.rs
    json.rs
```

---

# 5. Config

設定ファイル

```
config/config.yaml
```

例

```yaml
provider:
  type: openrouter
  model: gpt-5

temperature: 0.2
max_tokens: 8000
max_iterations: 20

stream: true

session:
  save: true
  directory: sessions

system_prompt: |
  あなたは、優秀なAIエージェントです。
  日本語で応答すること。

rules:
  - NAMI.md

memory:
  directory: memory
  file: memory.jsonl

logging:
  directory: logs
  level: info

skills:
  - filesystem
  - shell
  - browser
  - search
  - http

mcp:
  servers:

    - name: filesystem
      transport: stdio
      command: npx
      args: [
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "/Users/username/Desktop",
        "/path/to/other/allowed/dir"
      ]
      env: {}

    - name: github
      transport: http
      url: http://localhost:3001/mcp
```

---

# 6. Agent Loop

```
User

↓

LLM

↓

Tool Call ?

├── No
│      ↓
│    Answer
│
└── Yes
       ↓
 Skill or MCP
       ↓
 Tool Result
       ↓
 LLM
```

最大反復回数

```
max_iterations
```

で制御する。

---

# 7. Provider

```
trait LlmProvider {

    async fn chat(
        &self,
        request: ChatRequest
    ) -> ChatResponse;

}
```

OpenAI互換Providerは共通化する。

Geminiのみ別実装。

---

# 8. Skills

Skillはローカル機能。

```
trait Skill {

    fn name(&self) -> String;

    async fn execute(
        &self,
        args: Value
    ) -> Value;

}
```

Registry

```
HashMap<String, Box<dyn Skill>>
```

標準Skill

* Filesystem
* Shell
* Browser
* Search
* HTTP

---

# 9. MCP

```
trait McpClient {

    async fn list_tools();

    async fn call_tool();

}
```

対応

* stdio
* Streamable HTTP

SSEはv2対応。

---

# 10. Tool Router

優先順位

```
Skill

↓

MCP

↓

Error
```

SkillとMCPを同一インターフェースで扱う。

---

# 11. Memory

保存形式

```
memory/

    memory.jsonl
```

1行1メッセージ。

例

```json
{
  "role":"assistant",
  "content":"こんにちは",
  "usage":{
      "input_tokens":120,
      "output_tokens":80,
      "total_tokens":200
  }
}
```

特徴

* appendのみ
* 高速
* Git管理しやすい
* grep可能
* jq対応

---

# 12. Session

```
sessions/

2026-06-27_21-15-02.json
```

保存内容

* Config
* Messages
* Metrics
* Tool Calls
* MCP Calls
* Errors

---

# 13. Metrics

```
TokenUsage

- input_tokens
- output_tokens
- total_tokens
- cached_tokens
- reasoning_tokens
```

```
Metrics

- iterations
- llm_calls
- tool_calls
- mcp_calls
- elapsed_ms
- usage
- cost
```

Cost

```
prompt_cost
completion_cost
total_cost
currency
```

取得可能なProviderのみ保存。

---

# 14. CLI表示

終了時

```
Session finished

Iterations      : 5
LLM Calls       : 8
Tool Calls      : 3
MCP Calls       : 1

Input Tokens    : 5,210
Output Tokens   : 3,980
Total Tokens    : 9,190

Cached Tokens   : 2,200
Reasoning       : 512

Estimated Cost  : $0.0312

Elapsed         : 12.4 sec
```

---

# 15. Logging

```
logs/

2026-06-27.log
```

記録内容

* Prompt
* Response
* Tool
* MCP
* Error
* Execution Time
* Token Usage

---

# 16. 使用クレート

| 用途       | クレート                         |
| -------- | ---------------------------- |
| 非同期      | tokio                        |
| HTTP     | reqwest                      |
| JSON     | serde / serde_json           |
| YAML     | serde_yaml                   |
| CLI      | clap                         |
| MCP      | rmcp                         |
| Error    | anyhow / thiserror           |
| Logging  | tracing / tracing-subscriber |
| UUID     | uuid                         |
| DateTime | chrono                       |

---

# 17. ディレクトリ

```
project/

config/
    config.yaml

memory/
    memory.jsonl

sessions/

logs/

src/
```

---

# 18. v1対象機能

* CLI
* OpenAI互換API
* Gemini
* Streaming
* YAML設定
* JSONLメモリ
* Session保存
* Metrics
* Logging
* Skills
* MCP
* Tool Router

---

# 19. v2予定

* Planner
* Long Memory
* Summary Memory
* Embeddings
* RAG
* VectorDB
* Parallel Tool Call
* Multi Agent
* Workflow Engine
* Plugin System
* Web UI
* GUI
* YAML Workflow
* Tool Permission
* Retry Policy
* Cache Layer

---

# 20. 設計方針

1. OpenAI互換APIを中心にProviderを抽象化する。
2. Geminiは専用Providerで差異を吸収する。
3. SkillsとMCPを共通の「Tool」として扱う。
4. MemoryはJSONL、SessionはJSONで役割を分離する。
5. すべてのLLM呼び出しについてトークン数・実行時間・コストを記録する。
6. 依存クレートを最小限に抑え、単一EXEでの配布を前提とする。
7. 将来のRAG、Planner、Multi Agentへの拡張を考慮したモジュール設計とする。

---

# 21. 実装メモ（MCP / rmcp 想定）

本節は実装を進めるにあたって仕様を補足・更新したものである。

## 21.1 rmcp の採用

MCP 接続には公式 Rust SDK の `rmcp`（v1.8.0 以降）を使用する。

```toml
[dependencies]
rmcp = { version = "1.8.0", features = [
    "client",
    "transport-child-process",
    "transport-streamable-http-client-reqwest",
] }
```

## 21.2 接続方式

### stdio（子プロセス）

```rust
let mut cmd = tokio::process::Command::new(command);
for a in args { cmd.arg(a); }
for (k, v) in env { cmd.env(k, v); }
let transport = TokioChildProcess::new(cmd)?;
let running = serve_client(ClientInfo::default(), transport).await?;
```

`ClientInfo::default()` は `ClientHandler` を実装しており、追加のハンドラクラスを用意しなくてもよい。環境変数 `env` も `cmd.env()` に反映する。

### HTTP（Streamable HTTP）

```rust
let transport = StreamableHttpClientTransport::from_uri(url);
let running = serve_client(ClientInfo::default(), transport).await?;
```

## 21.3 Tool データのマッピング

rmcp が返す `rmcp::model::Tool` は以下のフィールドを持つ。

* `name: Cow<'static, str>`
* `description: Option<Cow<'static, str>>`
* `input_schema: Arc<JsonObject>`

これをエージェント内部の `Tool { name, description, input_schema }` に変換して ToolRouter で扱う。

## 21.4 Tool 呼び出し

```rust
let mut params = CallToolRequestParams::new(tool_name.to_string());
params.arguments = Some(rmcp::model::object(arguments));
let result = running.call_tool(params).await?;
```

`CallToolRequestParams` は `#[non_exhaustive]` になっており、構造体リテラルの更新構文が使えないため、可変変数を作成して `arguments` フィールドに代入する。

## 21.5 Tool Router の優先順位

ToolRouter は以下の順序で解決する。

1. Skill（ローカル機能）
2. MCP（接続先サーバー全てを対象に名前解決）
3. どちらにも該当しない場合はエラー

実行結果 `ToolResult` は `is_mcp` フラグを持ち、Metrics で `tool_calls` と `mcp_calls` を区別して記録する。

## 21.6 OpenAI 互換 Provider の Tool 形式

OpenAI 互換 API へリクエストする際、Tool / ToolCall / Message は以下の形式に変換する。

```json
{
  "type": "function",
  "function": {
    "name": "...",
    "description": "...",
    "parameters": { ... }
  }
}
```

```json
{
  "id": "call_xxx",
  "type": "function",
  "function": {
    "name": "...",
    "arguments": { ... }
  }
}
```

これにより OpenAI / OpenRouter / Azure OpenAI / Groq / Anthropic 互換レイヤーなどでツール利用が可能となる。

## 21.7 依存の修正

`reqwest` を TLS として `rustls` フィーチャーを使用する。`rustls-tls` は reqwest 0.13 系では存在しないため、`Cargo.toml` で以下のように指定する。

```toml
reqwest = { version = "0.13.4", features = ["json", "stream", "rustls"] }
```

またファイルログ出力のために `tracing-appender` を追加する。

```toml
tracing-appender = "0.2"
```

## 21.8 環境変数による設定上書き

YAML の設定に加え、以下の環境変数で実行時に上書きできる。

| 環境変数 | 設定項目 |
| --- | --- |
| `NAMI_PROVIDER_TYPE` | provider.type |
| `NAMI_PROVIDER_MODEL` | provider.model |
| `NAMI_PROVIDER_API_KEY` | provider.api_key |
| `NAMI_PROVIDER_BASE_URL` | provider.base_url |
| `NAMI_TEMPERATURE` | temperature |
| `NAMI_MAX_TOKENS` | max_tokens |
| `NAMI_MAX_ITERATIONS` | max_iterations |
| `NAMI_STREAM` | stream |

プロバイダー固有の API キー（`OPENAI_API_KEY` など）は `resolve_provider` でも補完する。

## 21.9 Skill の有効化と Tool Router

`config.yaml` の `skills` に記載された名前のみを SkillRegistry に登録する。未登録の名前は警告ログを出力して無視する。

ToolRouter は以下の優先順位で解決し、`ToolResult.is_mcp` により Metrics を `tool_calls` / `mcp_calls` に分計する。

1. Skill（ローカル）
2. MCP サーバー
3. 未解決の場合はエラー

## 21.10 rules（追加指示ファイル）

`config.yaml` の `rules` に指定されたファイル（例：`NAMI.md`）をシステムプロンプトに追加で読み込む。

```yaml
rules:
  - NAMI.md
```

読み込んだ内容は `--- Rule: {path} ---` 形式でシステムメッセージに追記する。

## 21.11 ログ出力

ログには以下の情報を出力する。

* ユーザー入力プロンプト
* LLM レスポンス（トークン使用量付き）
* Tool / MCP 呼び出し
* エラー
* 実行時間・反復回数
* 推定コスト（取得可能な場合）

`tracing` の `info` / `debug` レベルを使い、`logs/` ディレクトリへ日別ローテーションで書き出す。

## 21.12 Memory（重複書き込み防止）

`JsonlMemoryStore` は `append` 時にバッファに蓄え、`flush` 時にまとめて書き込み、書き込み後にバッファを `clear` する。これにより同じメッセージが二重に書き込まれるのを防ぐ。

## 21.13 ProviderKind のシリアライズ

`ProviderKind` は `#[serde(rename_all = "lowercase")]` のみを指定し、`ProviderConfig.kind` フィールドが YAML の `provider.type` に対応する。`#[serde(tag = "type")]` を付与すると内部タグ付き enum として扱われ、文字列値からの復元に失敗するため注意する。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAI,
    OpenRouter,
    // ...
}
```

## 21.14 OpenAI 互換 Provider のデフォルト base_url

`base_url` が未指定の場合、プロバイダー種別に応じた众所知のエンドポイントをデフォルトとする。

| Provider | デフォルト base_url |
| --- | --- |
| OpenAI | `https://api.openai.com/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |
| Groq | `https://api.groq.com/openai/v1` |
| Anthropic | `https://api.anthropic.com/v1` |
| AzureOpenAI | 必須（config で指定） |

## 22. `nami init` コマンド

`nami init` を実行すると、実行中の EXE ファイルがあるディレクトリに `.nami/` フォルダを作成し、以下のファイル・ディレクトリを生成する。

- `.env`
- `.env.sample`
- `NAMI.md`
- `config/config.yaml`
- `logs/`
- `memory/`
- `sessions/`
- `skills/`

既に存在するファイルは上書きせず、新規作成時のみデフォルトテンプレートを書き込む。

デフォルトの `config.yaml` では `session.directory`、`memory.directory`、`memory.file`、`logging.directory` を相対パスで指定する。これらは `load()` 時に設定ファイルの基準ディレクトリから解決され、`.nami/sessions` や `.nami/logs` などとして扱われる。

## 23. 設定ファイルの解決順序

`nami` を実行する際、設定ファイルは以下の順序で解決する。

1. `--config` / `-c` で明示的に指定されたパス
2. カレントディレクトリの `.nami/config/config.yaml`
3. `.nami/` が存在しなければ、カレントディレクトリに `.nami/config/` を作成する
4. カレントディレクトリの `config/` フォルダ内の YAML ファイル（存在する場合はその中で辞書順先頭）
5. 実行ファイルがあるディレクトリの `.nami/config/config.yaml`

このため、プロジェクト固有の `.nami/config/config.yaml` または従来の `config/config.yaml` があればそちらが優先され、どちらもなければ `nami init` で生成された実行ファイル配置場所の設定が使われる。

また、設定ファイル読み込み時にその配置からプロジェクト基準ディレクトリ（`base_dir`）を決定し、`rules` および各種ディレクトリ設定の相対パスを `base_dir` からのパスに解決する。

`.env` もまず `base_dir/.env` を読み込み、存在しなければカレントディレクトリの `.env` を読み込む。

## 24. セッション ID の出力

`nami` の実行終了時に、保存されたセッションの ID を最後に出力する。

```
Session ID      : 2026-06-27_12-34-56
```

セッション保存が無効（`--no-session` または `session.save: false`）の場合は `none` を出力する。

## 25. ターミナルでのストリーミング表示

`config.yaml` の `stream: true`（または環境変数 `NAMI_STREAM=true`）の場合、OpenAI 互換 Provider は SSE ストリームを受信しつつ、到着した内容をリアルタイムでターミナルに表示する（`stdout`）。

ストリーミング表示中にログ出力と混在しないよう、コンソールログは `stderr` に出力する。ストリーミング完了後は改行を出力し、その後にセッションサマリーを表示する。

### usage の取得

リクエストに `stream_options: { include_usage: true }` を含めることで、対応する OpenAI 互換 Provider からストリーム終了時に `usage` を受け取る。取得できた場合は `ChatResponse.usage` に反映され、セッションサマリーのトークン数として表示される。取得できない Provider では 0 のままとなる。

### ツール呼び出しの表示

ストリーミング中に `delta.tool_calls` が到着した場合、関数 `name` が確定したタイミングで以下のように表示し、以降の `arguments` delta をそのまま追記する。`stream: false` 時も、Executor が Tool Router に実行を依頼する前に同じ形式で表示する。

```
[tool_call: shell] {"command": "echo ..."}
```

### 思考（thinking / reasoning）の表示

`delta.reasoning_content`、`delta.thinking`、`delta.reasoning` のいずれかが含まれる場合、ストリーミング時はリアルタイムに、非ストリーミング時は LLM レスポンス受信後に `<thinking>` / `</thinking>` で囲って表示する。

```
<thinking>
...思考内容...
</thinking>
```

