use serde::Serialize;

use crate::support::config::{AgentBackendSettings, AgentBackendSettingsEntry, AgentBackendType};

#[derive(Debug, Clone)]
pub struct AgentBackendRegistry {
    settings: AgentBackendSettings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBackendsSummary {
    pub default_backend: String,
    pub backends: Vec<AgentBackendSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBackendSummary {
    pub id: String,
    pub backend_type: String,
    pub enabled: bool,
    pub default_backend: bool,
    pub command_configured: bool,
    pub timeout_seconds: u64,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub execution_mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBackendDiagnosticResult {
    pub backend_id: String,
    pub backend_type: String,
    pub enabled: bool,
    pub status: String,
    pub execution_mode: String,
    pub details: Vec<String>,
}

impl AgentBackendRegistry {
    pub fn new(settings: AgentBackendSettings) -> Self {
        Self { settings }
    }

    pub fn summary(&self) -> AgentBackendsSummary {
        AgentBackendsSummary {
            default_backend: self.settings.default_backend.clone(),
            backends: self
                .settings
                .backends
                .values()
                .map(|backend| self.backend_summary(backend))
                .collect(),
        }
    }

    pub async fn test_backend(
        &self,
        backend_id: &str,
    ) -> anyhow::Result<AgentBackendDiagnosticResult> {
        let backend = self
            .settings
            .backends
            .get(backend_id)
            .ok_or_else(|| anyhow::anyhow!("unknown agent backend {backend_id}"))?;
        if !backend.enabled {
            anyhow::bail!("agent backend {backend_id} is disabled");
        }
        match backend.backend_type {
            AgentBackendType::InternalLlm => Ok(AgentBackendDiagnosticResult {
                backend_id: backend.name.clone(),
                backend_type: backend.backend_type.as_str().to_string(),
                enabled: true,
                status: "ready".to_string(),
                execution_mode: backend.backend_type.execution_mode().to_string(),
                details: vec![
                    "Uses the configured LLM Gateway and existing structured action/result schemas."
                        .to_string(),
                    "No external CLI is executed in this diagnostic.".to_string(),
                ],
            }),
            AgentBackendType::CodexCli
            | AgentBackendType::ClaudeCodeCli
            | AgentBackendType::OpencodeCli => {
                let command_path = backend.command_path.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("agent backend {backend_id} has no configured command path")
                })?;
                let metadata = tokio::fs::metadata(command_path).await.map_err(|error| {
                    anyhow::anyhow!(
                        "failed to inspect agent backend command {}: {error}",
                        command_path.display()
                    )
                })?;
                if !metadata.is_file() {
                    anyhow::bail!(
                        "agent backend command {} is not a regular file",
                        command_path.display()
                    );
                }
                Ok(AgentBackendDiagnosticResult {
                    backend_id: backend.name.clone(),
                    backend_type: backend.backend_type.as_str().to_string(),
                    enabled: true,
                    status: "configured".to_string(),
                    execution_mode: backend.backend_type.execution_mode().to_string(),
                    details: vec![
                        "Command path exists; first-stage diagnostic does not invoke the CLI."
                            .to_string(),
                        format!(
                            "Limits: timeout={}s, maxInputBytes={}, maxOutputBytes={}.",
                            backend.timeout_seconds,
                            backend.max_input_bytes,
                            backend.max_output_bytes
                        ),
                    ],
                })
            }
        }
    }

    fn backend_summary(&self, backend: &AgentBackendSettingsEntry) -> AgentBackendSummary {
        AgentBackendSummary {
            id: backend.name.clone(),
            backend_type: backend.backend_type.as_str().to_string(),
            enabled: backend.enabled,
            default_backend: backend.name == self.settings.default_backend,
            command_configured: backend.command_path.is_some(),
            timeout_seconds: backend.timeout_seconds,
            max_input_bytes: backend.max_input_bytes,
            max_output_bytes: backend.max_output_bytes,
            execution_mode: backend.backend_type.execution_mode().to_string(),
        }
    }
}
