use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Parsed JSON output from `pacgate compile --json`
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PacGateOutput {
    pub success: bool,
    #[serde(default)]
    pub rules_count: Option<u32>,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub generated: Option<Vec<PacGateGenerated>>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PacGateGenerated {
    pub file: String,
    #[serde(default)]
    pub size: Option<u64>,
}

/// Result of a PacGate compilation
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompileResult {
    pub success: bool,
    pub message: String,
    pub warnings: Vec<String>,
    pub rules_count: Option<u32>,
    pub output_dir: Option<String>,
}

/// Backend abstraction for PacGate — allows real or mock execution
#[allow(dead_code)]
pub enum PacGateBackend {
    Real(PacGateRunner),
    Mock { should_succeed: bool },
}

impl PacGateBackend {
    pub async fn compile(
        &self,
        rules_yaml: &str,
        counters: bool,
        rate_limit: bool,
        conntrack: bool,
    ) -> Result<CompileResult> {
        match self {
            PacGateBackend::Real(runner) => {
                runner
                    .compile(rules_yaml, counters, rate_limit, conntrack)
                    .await
            }
            PacGateBackend::Mock { should_succeed } => {
                if *should_succeed {
                    Ok(CompileResult {
                        success: true,
                        message: "Mock compilation successful".to_string(),
                        warnings: vec![],
                        rules_count: Some(3),
                        output_dir: Some("/tmp/mock-output".to_string()),
                    })
                } else {
                    Ok(CompileResult {
                        success: false,
                        message: "Mock compilation failed: syntax error".to_string(),
                        warnings: vec![],
                        rules_count: None,
                        output_dir: None,
                    })
                }
            }
        }
    }
}

/// Wraps the `pacgate` CLI as a subprocess
pub struct PacGateRunner {
    /// Path to pacgate binary (or just "pacgate" for PATH lookup)
    binary: String,
}

impl Default for PacGateRunner {
    fn default() -> Self {
        Self {
            binary: "pacgate".to_string(),
        }
    }
}

impl PacGateRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compile YAML rules using PacGate
    pub async fn compile(
        &self,
        rules_yaml: &str,
        counters: bool,
        rate_limit: bool,
        conntrack: bool,
    ) -> Result<CompileResult> {
        // Write YAML to a temp file
        let tmp_dir = std::env::temp_dir();
        let tmp_file = tmp_dir.join(format!("pacinet-rules-{}.yaml", uuid::Uuid::new_v4()));

        tokio::fs::write(&tmp_file, rules_yaml)
            .await
            .context("Failed to write temp rules file")?;

        debug!(path = %tmp_file.display(), "Wrote rules to temp file");

        let result = self
            .run_pacgate(&tmp_file, counters, rate_limit, conntrack)
            .await;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&tmp_file).await;

        result
    }

    async fn run_pacgate(
        &self,
        rules_path: &PathBuf,
        counters: bool,
        rate_limit: bool,
        conntrack: bool,
    ) -> Result<CompileResult> {
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.arg("compile").arg(rules_path).arg("--json");

        if counters {
            cmd.arg("--counters");
        }
        if rate_limit {
            cmd.arg("--rate-limit");
        }
        if conntrack {
            cmd.arg("--conntrack");
        }

        info!(binary = %self.binary, "Running PacGate compile");

        let output = cmd.output().await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    // Try to parse JSON output
                    let (rules_count, output_dir) =
                        match serde_json::from_str::<PacGateOutput>(&stdout) {
                            Ok(parsed) => {
                                debug!(?parsed, "Parsed PacGate JSON output");
                                (parsed.rules_count, parsed.output_dir)
                            }
                            Err(e) => {
                                warn!("Failed to parse PacGate JSON output: {}", e);
                                (None, None)
                            }
                        };

                    Ok(CompileResult {
                        success: true,
                        message: "Compilation successful".to_string(),
                        warnings: if stderr.is_empty() {
                            vec![]
                        } else {
                            vec![stderr]
                        },
                        rules_count,
                        output_dir,
                    })
                } else {
                    Ok(CompileResult {
                        success: false,
                        message: format!("Compilation failed: {}", stderr),
                        warnings: vec![],
                        rules_count: None,
                        output_dir: None,
                    })
                }
            }
            Err(e) => {
                // pacgate binary not found — expected during development
                Ok(CompileResult {
                    success: false,
                    message: format!("PacGate not available: {}", e),
                    warnings: vec!["pacgate binary not found in PATH".to_string()],
                    rules_count: None,
                    output_dir: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pacgate_json_output() {
        let json = r#"{
            "success": true,
            "rules_count": 5,
            "output_dir": "/tmp/pacgate-out",
            "generated": [
                {"file": "filter.v", "size": 1024},
                {"file": "counters.v", "size": 512}
            ]
        }"#;

        let output: PacGateOutput = serde_json::from_str(json).unwrap();
        assert!(output.success);
        assert_eq!(output.rules_count, Some(5));
        assert_eq!(output.output_dir.as_deref(), Some("/tmp/pacgate-out"));
        assert_eq!(output.generated.as_ref().unwrap().len(), 2);
        assert_eq!(output.generated.as_ref().unwrap()[0].file, "filter.v");
    }

    #[test]
    fn test_parse_minimal_json_output() {
        let json = r#"{"success": true}"#;
        let output: PacGateOutput = serde_json::from_str(json).unwrap();
        assert!(output.success);
        assert_eq!(output.rules_count, None);
        assert_eq!(output.output_dir, None);
    }

    #[test]
    fn test_parse_failure_json_output() {
        let json = r#"{"success": false, "message": "Invalid YAML syntax"}"#;
        let output: PacGateOutput = serde_json::from_str(json).unwrap();
        assert!(!output.success);
        assert_eq!(output.message.as_deref(), Some("Invalid YAML syntax"));
    }

    #[tokio::test]
    async fn test_mock_backend_success() {
        let backend = PacGateBackend::Mock {
            should_succeed: true,
        };
        let result = backend
            .compile("rules: []", false, false, false)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.rules_count, Some(3));
        assert!(result.output_dir.is_some());
    }

    #[tokio::test]
    async fn test_mock_backend_failure() {
        let backend = PacGateBackend::Mock {
            should_succeed: false,
        };
        let result = backend
            .compile("rules: []", false, false, false)
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.message.contains("failed"));
        assert_eq!(result.rules_count, None);
    }
}
