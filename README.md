# nami-cli

Rust のみで動作する軽量な CLI AI エージェントです。
OpenAI 互換 API と Gemini に対応し、Skill（ローカル機能）と MCP（Model Context Protocol）の両方をツールとして使うことができます。

## 主な機能

- OpenAI 互換 API（OpenAI / OpenRouter / Azure OpenAI / Groq / Anthropic / その他）
- Gemini
- Skill（filesystem / shell / browser / search / http）
- MCP 接続（stdio / HTTP）
- YAML 設定
- JSONL メモリ
- セッション保存
- トークン数・実行時間・コスト推定のメトリクス
- ファイルログ出力

## 前提条件

- Rust 1.75 以降
- `cargo` が利用可能であること
- MCP stdio サーバーを使う場合は Node.js / npx などが必要な場合があります

## インストール

```bash
git clone <repository>
cd nami-cli
cargo build --release
```

ビルドが完了すると、`target/release/nami-cli` が生成されます。

## 設定

ビルドされたnamiを任意のフォルダで利用します。
```bash
nami init
```
設定ファイルは `config/config.yaml` または `.nami/config/config.yaml` です。初回利用時に `nami init` を実行すると、実行ファイルがある場所に `.nami/` を作成し、雛形の設定ファイルや `.env`、`NAMI.md`、各種ディレクトリを生成します。

設定ファイルの探索順序は以下の通りです。

1. `--config` / `-c` で指定されたパス
2. カレントディレクトリの `.nami/config/config.yaml`
3. カレントディレクトリの `config/*.yaml`
4. 実行ファイルがある場所の `.nami/config/config.yaml`

雛形は `config/config.yaml` または `nami init` で生成される `.nami/config/config.yaml` に同梱されています。

```yaml
provider:
  type: openrouter
  model: gpt-4o-mini
  api_key: OPENROUTER_API_KEY      # 環境変数で指定
  # base_url: https://...  # 省略時は provider 種別に応じたデフォルト URL

temperature: 0.2
max_tokens: 4000
max_iterations: 10
stream: false

session:
  save: true
  directory: sessions

system_prompt: |
  あなたは優秀なAIエージェントです。
  日本語で応答すること。

rules:
  - NAMI.md

memory:
  directory: memory
  file: memory/memory.jsonl

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
      args: ["-y", "@modelcontextprotocol/server-filesystem", "."]
      env: {}
```

### プロバイダー種別

`provider.type` に指定できる値は以下の通りです。

| 値 | 説明 | デフォルト base_url |
| --- | --- | --- |
| `openai` | OpenAI | `https://api.openai.com/v1` |
| `openrouter` | OpenRouter | `https://openrouter.ai/api/v1` |
| `azure_openai` | Azure OpenAI Service | 必須（config で指定） |
| `groq` | Groq | `https://api.groq.com/openai/v1` |
| `anthropic` | Anthropic（OpenAI 互換エンドポイント経由） | `https://api.anthropic.com/v1` |
| `gemini` | Google Gemini | `https://generativelanguage.googleapis.com/v1beta` |
| `custom` | 任意の OpenAI 互換 API | `https://api.openai.com/v1` |

## 環境変数

YAML の設定に加え、以下の環境変数で上書きできます。

| 環境変数 | 設定項目 |
| --- | --- |
| `NAMI_PROVIDER_TYPE` | `provider.type` |
| `NAMI_PROVIDER_MODEL` | `provider.model` |
| `NAMI_PROVIDER_API_KEY` | `provider.api_key` |
| `NAMI_PROVIDER_BASE_URL` | `provider.base_url` |
| `NAMI_TEMPERATURE` | `temperature` |
| `NAMI_MAX_TOKENS` | `max_tokens` |
| `NAMI_MAX_ITERATIONS` | `max_iterations` |
| `NAMI_STREAM` | `stream` |

また、プロバイダー固有の API キーは以下の環境変数からも読み込まれます。

- `OPENAI_API_KEY`
- `OPENROUTER_API_KEY`
- `GROQ_API_KEY`
- `ANTHROPIC_API_KEY`
- `AZURE_OPENAI_API_KEY`
- `GOOGLE_API_KEY`

## 実行例

### 通常実行

```bash
# 設定ファイルのデフォルトを使用
nami "こんにちは"

# 設定ファイルを指定
nami -c config/myconfig.yaml "RustでFizzBuzzを書いて"

# セッションを保存しない
nami --no-session "今日の天気を調べて"
```

### API キーを環境変数で渡す例

```bash
export OPENROUTER_API_KEY="sk-or-..."
nami "Rustの学習ロードマップを教えて"
```

### Gemini を使う例

```bash
export GOOGLE_GENERATIVE_AI_API_KEY="..."
nami -c config/gemini.yaml "こんにちは"
```

`config/gemini.yaml` の例:

```yaml
provider:
  type: gemini
  model: gemini-1.5-flash

temperature: 0.2
max_tokens: 4000
max_iterations: 10
```

### ストリーミングを有効にする

```yaml
stream: true
```

OpenAI 互換 Provider で `stream: true` にすると、SSE 経由でレスポンスを受信し、その内容をリアルタイムにターミナル上に表示します。対応する Provider ではストリーム終了時に `usage` も取得でき、途中の tool_call や thinking / reasoning も表示されます。

### 過去セッションを再開する

```bash
# セッション ID で再開
nami --resume 2026-06-27_21-15-02 "続きをお願い"

# パスを直接指定
nami --resume sessions/2026-06-27_21-15-02.json "続きをお願い"
```

## Skill 一覧

`config.yaml` の `skills` で有効化できます。

| Skill | 説明 |
| --- | --- |
| `filesystem` | ファイル・ディレクトリの読み書き・一覧（`operation`: `list` / `read` / `write`） |
| `shell` | シェルコマンド実行 |
| `browser` | 指定 URL の HTML を取得 |
| `search` | DuckDuckGo lite で Web 検索 |
| `http` | GET / POST / PUT / DELETE リクエスト送信 |

## MCP 接続

`mcp.servers` に接続先を追加すると、Skill と同じようにツールとして利用できます。

### stdio

```yaml
mcp:
  servers:
    - name: filesystem
      transport: stdio
      command: npx
      args: ["-y", "@modelcontextprotocol/server-filesystem", "."]
      env: {}
```

### HTTP（Streamable HTTP）

```yaml
mcp:
  servers:
    - name: github
      transport: http
      url: http://localhost:3001/mcp
```

MCP ツールと Skill で名前が衝突した場合、Skill が優先されます。

## 出力されるファイル

```
project/
├── config/
│   └── config.yaml
├── memory/
│   └── memory.jsonl        # 1行1メッセージで追記
├── sessions/
│   └── 2026-06-27_21-15-02.json
└── logs/
    └── nami-cli.log        # 日別ローテーション
```

## ログの確認

標準出力と `logs/nami-cli.log` の両方に以下が出力されます。

- ユーザー入力プロンプト
- LLM レスポンスとトークン使用量
- Tool / MCP 呼び出し
- エラー
- 実行時間・反復回数

## セッションの保存

`session.save: true` の場合、実行結果は `sessions/YYYY-MM-DD_HH-MM-SS.json` に保存されます。保存内容には以下が含まれます。

- 実行時の設定
- メッセージ履歴
- メトリクス
- Tool / MCP 呼び出し履歴
- エラー
- タイトル（先頭 user メッセージから自動生成、未設定時は最大先頭 200 文字）

## セッション管理コマンド

`nami session` でセッションの管理ができます。

```bash
# セッション一覧（JSON）
nami session list --json

# セッション詳細（JSON）
nami session show <session_id> --json

# セッション削除
nami session delete <session_id>

# セッション名変更
nami session rename <session_id> <タイトル>

# セッションを Markdown でエクスポート
nami session export <session_id> markdown
```

## 開発

```bash
# コンパイル確認
cargo check

# テスト実行
cargo test

# フォーマット
cargo fmt

# Clippy
cargo clippy
```

## 注意事項

- `shell` Skill は指定されたコマンドをそのまま実行します。信頼できない入力に対しては注意してください。
- API キーは環境変数で渡すことを推奨します。`config.yaml` に直接記述する場合は、ファイルを誤ってコミットしないようにしてください。

## ライセンス

MIT
