use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::{debug, info};

/// Result of a PacGate compilation
pub struct CompileResult {
    pub success: bool,
    pub message: String,
    pub warnings: Vec<String>,
}

/// Wraps the `pacgate` CLI as a subprocess
pub struct PacGateRunner {
    /// Path to pacgate binary (or just "pacgate" for PATH lookup)
    binary: String,
}

impl PacGateRunner {
    pub fn new() -> Self {
        Self {
            binary: "pacgate".to_string(),
        }
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

        let result = self.run_pacgate(&tmp_file, counters, rate_limit, conntrack).await;

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
        cmd.arg("compile")
            .arg(rules_path)
            .arg("--json");

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
                let _stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    Ok(CompileResult {
                        success: true,
                        message: "Compilation successful".to_string(),
                        warnings: if stderr.is_empty() {
                            vec![]
                        } else {
                            vec![stderr]
                        },
                    })
                } else {
                    Ok(CompileResult {
                        success: false,
                        message: format!("Compilation failed: {}", stderr),
                        warnings: vec![],
                    })
                }
            }
            Err(e) => {
                // pacgate binary not found — expected during development
                Ok(CompileResult {
                    success: false,
                    message: format!("PacGate not available: {}", e),
                    warnings: vec!["pacgate binary not found in PATH".to_string()],
                })
            }
        }
    }
}
