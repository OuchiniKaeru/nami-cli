use crate::models::tool::Tool;
use anyhow::Context;
use rmcp::model::{CallToolRequestParams, ClientInfo, Tool as RmcpTool};
use rmcp::service::{serve_client, RunningService, RoleClient};
use rmcp::transport::child_process::TokioChildProcess;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct McpConnection {
    pub name: String,
    pub running: RunningService<RoleClient, ClientInfo>,
}

impl McpConnection {
    pub async fn list_tools(&self) -> anyhow::Result<Vec<RmcpTool>> {
        Ok(self.running.list_all_tools().await?)
    }

    pub async fn call_tool(
        &self,
        params: CallToolRequestParams,
    ) -> anyhow::Result<rmcp::model::CallToolResult> {
        Ok(self.running.call_tool(params).await?)
    }
}

/// MCP クライアント。
/// 接続された各サーバーのツール一覧を取得し、ツール名から対象サーバー名を引けるようにする。
pub struct McpClient {
    pub connections: RwLock<Vec<McpConnection>>,
    /// ツール名 → サーバー名 のインデックス。
    /// `list_all_tools` 呼び出し時に更新される。
    pub tool_index: RwLock<HashMap<String, String>>,
}

impl McpClient {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(Vec::new()),
            tool_index: RwLock::new(HashMap::new()),
        }
    }

    pub async fn connect_stdio(
        &self,
        name: impl Into<String>,
        command: impl AsRef<str>,
        args: &[impl AsRef<str>],
        env: Option<std::collections::HashMap<String, String>>,
    ) -> anyhow::Result<()> {
        let name = name.into();
        let mut cmd = tokio::process::Command::new(command.as_ref());
        for a in args {
            cmd.arg(a.as_ref());
        }
        if let Some(vars) = env {
            for (k, v) in vars {
                cmd.env(k, v);
            }
        }
        let transport = TokioChildProcess::new(cmd)
            .with_context(|| format!("failed to spawn MCP stdio process: {}", name))?;
        let running = serve_client(ClientInfo::default(), transport)
            .await
            .with_context(|| format!("failed to start MCP stdio client: {}", name))?;

        self.connections.write().await.push(McpConnection { name, running });
        Ok(())
    }

    pub async fn connect_http(
        &self,
        name: impl Into<String>,
        url: impl AsRef<str>,
    ) -> anyhow::Result<()> {
        let name = name.into();
        let url = url.as_ref();
        let transport =
            rmcp::transport::StreamableHttpClientTransport::from_uri(url.to_string());
        let running = serve_client(ClientInfo::default(), transport)
            .await
            .with_context(|| format!("failed to start MCP http client: {}", name))?;

        self.connections.write().await.push(McpConnection { name, running });
        Ok(())
    }

    /// 接続されているすべての MCP サーバーからツール一覧を取得する。
    /// 同時に `tool_index`（ツール名 → サーバー名）を更新する。
    pub async fn list_all_tools(&self) -> anyhow::Result<Vec<Tool>> {
        let mut tools = Vec::new();
        let mut index = HashMap::new();
        for conn in self.connections.read().await.iter() {
            let result = conn.list_tools().await?;
            for t in result {
                let name = t.name.to_string();
                index.insert(name.clone(), conn.name.clone());
                tools.push(Tool {
                    name,
                    description: t.description.as_ref().map(|d| d.to_string()),
                    input_schema: Some(serde_json::to_value(t.input_schema).unwrap_or_default()),
                });
            }
        }
        *self.tool_index.write().await = index;
        Ok(tools)
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<Value> {
        for conn in self.connections.read().await.iter() {
            if conn.name == server_name {
                let mut params = CallToolRequestParams::new(tool_name.to_string());
                params.arguments = Some(rmcp::model::object(arguments));
                let result = conn.call_tool(params).await?;
                return Ok(serde_json::to_value(result)?);
            }
        }
        anyhow::bail!("MCP server not found: {}", server_name)
    }

    /// ツール名から対象 MCP サーバーを特定して呼び出す。
    /// `list_all_tools` によって構築されたインデックスを参照する。
    /// インデックスに存在しない場合は、全サーバーにフォールバックする。
    pub async fn call_tool_by_name(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<Value> {
        // まずインデックスからサーバー名を特定して呼び出す。
        if let Some(server_name) = self.tool_index.read().await.get(tool_name).cloned() {
            return self.call_tool(&server_name, tool_name, arguments).await;
        }

        // インデックスが未構築の場合は既存のフォールバック動作を行う。
        for conn in self.connections.read().await.iter() {
            let mut params = CallToolRequestParams::new(tool_name.to_string());
            params.arguments = Some(rmcp::model::object(arguments.clone()));
            if let Ok(result) = conn.call_tool(params).await {
                return Ok(serde_json::to_value(result)?);
            }
        }
        anyhow::bail!("MCP tool not found on any server: {}", tool_name)
    }
}
