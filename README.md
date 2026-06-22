# Nami - Local-first AI Agent Framework

Rustで実装されたローカルファーストのAIエージェントフレームワークです。AutoAgentsをバックエンドとして使用し、柔軟なエージェント設定と実行をサポートします。

## 特徴

- **ローカルファースト**: ローカル環境でエージェントを実行
- **複数LLMプロバイダー対応**: OpenAI, Anthropic, Google, OpenRouter, Ollama, Groq, Azure OpenAI
- **柔軟な設定**: YAMLファイルでエージェントの動作をカスタマイズ
- **スキルシステム**: 再利用可能なスキルの定義と実行
- **MCP対応**: Model Context Protocolによるツール統合
- **セッション管理**: 会話履歴の永続化と管理

## インストール

### 前提条件

- Rust 1.70以上
- Cargo

### ビルド

```bash
git clone <repository-url>
cd MyAgents
cargo build --release
```

バイナリは `target/release/nami` に生成されます。

### パス設定

ビルドしたバイナリにパスを通して、どこからでも`nami`コマンドを使えるようにします：

```bash
# 現在のディレクトリにシンボリックリンクを作成
sudo ln -s $(pwd)/target/release/nami /usr/local/bin/nami

# または、 cargo bin ディレクトリにパスを追加
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

## クイックスタート

### 1. プロジェクトの初期化

新しいプロジェクトで以下を実行：

```bash
# カレントディレクトリを初期化
nami init

# または、特定のディレクトリを初期化
nami init -p /path/to/project
```

これにより、以下の構成が作成されます：

```
.nami/
├── agent.yaml          # エージェント設定
├── mcp_setting.json    # MCPサーバー設定
├── .env                # 環境変数（APIキー等）
├── .env.sample         # 環境変数のテンプレート
├── NAMI.md             # エージェントのルール
├── skills/             # スキルディレクトリ
│   └── sample/         # サンプルスキル
├── sessions/           # セッション履歴
├── cache/              # キャッシュ
├── logs/               # ログ
└── runtime/            # ランタイムデータ
```

### 2. 環境変数の設定

```bash
# .env.sampleをコピーして.envを作成
cp .nami/.env.sample .nami/.env

# エディタでAPIキーを設定
nano .nami/.env
```

### 3. エージェントとの対話

```bash
nami chat "こんにちは"
```

### 2. 設定ファイルの編集

`.nami/agent.yaml` を編集して、エージェントの動作をカスタマイズ：

```yaml
project:
  name: my-project

model:
  provider: openai        # openai, anthropic, google, openrouter, ollama, groq, azure-openai
  model: gpt-4o-mini      # 使用するモデル名
  api_key_env: OPENAI_API_KEY  # APIキーの環境変数名（オプション）
  base_url: http://localhost:11434  # カスタムエンドポイント（オプション）

system_prompt: |
  あなたは親切なAIアシスタントです。
  日本語で応答してください。

rules:
  - NAMI.md
  - custom_rules.md

skills:
  - sample

mcp:
  - filesystem
```

### 3. エージェントとの対話

```bash
# 基本的なチャット
./target/release/nami chat "こんにちは"

# セッションを再開
./target/release/nami chat --resume <session-id> "続きをお願い"

# セッション一覧
./target/release/nami session list

# セッション詳細
./target/release/nami session show <session-id>
```

## コマンドリファレンス

### 基本オプション

```bash
nami [OPTIONS] <COMMAND>

Options:
  -c, --config <PATH>  カスタム設定ファイルのパスを指定
  -h, --help           ヘルプを表示
  -V, --version        バージョンを表示
```

### チャットコマンド

```bash
nami chat [OPTIONS] [MESSAGE]

Arguments:
  [MESSAGE]  エージェントに送信するメッセージ

Options:
      --resume <SESSION_ID>  既存セッションを再開
```

**使用例:**
```bash
# メッセージを指定して実行
nami chat "今日の天気を教えて"

# セッションを再開
nami chat --resume abc123 "続きをお願い"

# インタラクティブモード（メッセージなし）
nami chat
```

### セッション管理

```bash
# セッション一覧
nami session list

# セッション詳細表示
nami session show <SESSION_ID>

# セッション削除
nami session delete <SESSION_ID>
```

### スキル実行

```bash
nami skill run <NAME> [OPTIONS]

Options:
  -i, --input <INPUT>  JSON形式の入力（デフォルト: {}）
```

**使用例:**
```bash
nami skill run my-skill -i '{"query": "test"}'
```

### ワークフロー実行

```bash
nami workflow run <TASK>
```

**使用例:**
```bash
nami workflow run "README.mdを作成してください"
```

### プロジェクト初期化

```bash
# カレントディレクトリを初期化
nami init

# 特定のディレクトリを初期化
nami init -p /path/to/project
```

**使用例:**
```bash
# 新しいプロジェクトを作成
mkdir my-project && cd my-project
nami init

# 既存のプロジェクトを初期化
cd existing-project
nami init
```

### MCP管理

```bash
# MCPサーバー一覧
nami mcp list
```

## 設定ファイル詳細

### カスタム設定ファイルの使用

デフォルトの `.nami/agent.yaml` の代わりに、任意の設定ファイルを使用できます：

```bash
nami --config /path/to/custom/agent.yaml chat "Hello"
```

### 設定項目

#### project
```yaml
project:
  name: my-project  # プロジェクト名（セッション管理で使用）
```

#### model
```yaml
model:
  provider: openai           # LLMプロバイダー
  model: gpt-4o-mini         # モデル名
  api_key_env: OPENAI_API_KEY  # APIキーの環境変数名
  base_url: http://...       # カスタムエンドポイント（オプション）
```

**対応プロバイダー:**
- `openai` - OpenAI API
- `anthropic` - Anthropic Claude
- `google` / `gemini` - Google Gemini
- `openrouter` - OpenRouter
- `ollama` - ローカルのOllama
- `groq` - Groq
- `azure-openai` - Azure OpenAI

#### system_prompt
エージェントのシステムプロンプト。ファイルパスを指定することも可能：

```yaml
system_prompt: |
  あなたは優秀なアシスタントです。

# またはファイルから読み込み
system_prompt: prompts/assistant.md
```

#### rules
エージェントが従うべきルール。ファイルパスを指定：

```yaml
rules:
  - NAMI.md
  - coding_rules.md
  - safety_guidelines.md
```

#### skills
使用するスキルのリスト：

```yaml
skills:
  - sample
  - github
  - browser
```

#### mcp
Model Context Protocolサーバーの設定：

```yaml
# 簡易形式（名前のみ）
mcp:
  - filesystem
  - github

# 詳細形式
mcp:
  filesystem:
    transport: stdio
    endpoint: npx @modelcontextprotocol/server-filesystem
    timeout: 30
  github:
    transport: http
    endpoint: http://localhost:8080
```

**トランスポートタイプ:**
- `stdio` - 標準入出力（デフォルト）
- `http` - HTTP接続
- `websocket` - WebSocket接続

## 環境変数

APIキーは環境変数、または `.env` ファイルで設定：

### .envファイル

プロジェクトルートまたは `.nami/.env` に配置：

```bash
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
OPENROUTER_API_KEY=sk-or-...
```

### 環境変数の優先順位

1. システム環境変数
2. `.env` ファイルの値

## スキルシステム

スキルは `.nami/skills/<skill-name>/SKILL.md` に定義：

```
.nami/
└── skills/
    └── sample/
        └── SKILL.md
```

### SKILL.md の形式

```markdown
---
name: Custom Skill Name
description: スキルの説明
version: 1.0.0
tools:
  - tool1
  - tool2
---

# スキルの本文

スキルの詳細な説明と使用方法。
```

## セッション管理

セッションは `.nami/sessions/` に保存：

```
.nami/
└── sessions/
    └── <session-id>/
        ├── metadata.json
        └── messages.jsonl
```

### セッションの永続化

- 自動的に会話履歴が保存
- `--resume` で過去のセッションを再開
- セッションIDで管理

## ワークフロー

ワークフローは複数のエージェントを連携させてタスクを実行：

```bash
nami workflow run "複雑なタスクの説明"
```

ワークフローは自動的に：
1. プランナーエージェントがタスクを分解
2. コーダーエージェントが実装
3. レビュアーエージェントが検証

## トラブルシューティング

### APIキーエラー

```
error: missing required environment variable OPENAI_API_KEY
```

**解決策:**
```bash
# .envファイルを作成
echo "OPENAI_API_KEY=sk-..." > .nami/.env

# または環境変数を設定
export OPENAI_API_KEY=sk-...
```

### 設定ファイルが見つからない

```
warning: failed to read agent.yaml
```

**解決策:**
- パスが正しいか確認
- ファイルの権限を確認
- YAMLの構文エラーがないか確認

### モデルが見つからない

```
error: unsupported provider 'xxx'
```

**解決策:**
- `model.provider` の値を確認
- 対応プロバイダー一覧を参照

## 開発

### テスト実行

```bash
cargo test
```

### ビルド

```bash
cargo build          # デバッグビルド
cargo build --release  # リリースビルド
```

### クリーン

```bash
cargo clean
```

## アーキテクチャ

```
nami/
├── src/
│   ├── main.rs          # エントリーポイント、CLI
│   ├── config.rs        # 設定管理
│   ├── runtime.rs       # ランタイム（AutoAgents/Deterministic）
│   ├── session.rs       # セッション管理
│   ├── skill.rs         # スキルシステム
│   ├── workflow.rs      # ワークフロー実行
│   ├── mcp.rs           # MCP統合
│   └── event.rs         # イベント記録
├── .nami/
│   ├── agent.yaml       # エージェント設定
│   ├── mcp_setting.json # MCP設定
│   ├── .env             # 環境変数
│   ├── skills/          # スキル定義
│   └── sessions/        # セッション履歴
└── Cargo.toml
```

## ライセンス

MIT

## 貢献

プルリクエストを歓迎します。

## サポート

問題や質問は、GitHub Issuesまでお願いします。