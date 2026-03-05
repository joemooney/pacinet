use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{debug, info, warn};

#[derive(Debug, Deserialize)]
pub struct PacGateCompileOutput {
    pub status: String,
    #[serde(default)]
    pub rules_count: Option<u32>,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PacGateLintOutput {
    #[serde(default)]
    findings: Vec<PacGateLintFinding>,
}

#[derive(Debug, Deserialize)]
struct PacGateLintFinding {
    #[serde(default)]
    level: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

/// Result of a PacGate compilation.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompileResult {
    pub success: bool,
    pub message: String,
    pub warnings: Vec<String>,
    pub rules_count: Option<u32>,
    pub output_dir: Option<String>,
}

/// Backend abstraction for PacGate — allows real or mock execution.
#[allow(dead_code)]
pub enum PacGateBackend {
    Real(PacGateRunner),
    Mock { should_succeed: bool },
}

impl PacGateBackend {
    pub async fn compile(
        &self,
        rules_yaml: &str,
        options: &pacinet_proto::CompileOptions,
    ) -> Result<CompileResult> {
        match self {
            PacGateBackend::Real(runner) => runner.compile(rules_yaml, options).await,
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

/// Wraps the `pacgate` CLI as a subprocess.
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

    pub async fn compile(
        &self,
        rules_yaml: &str,
        options: &pacinet_proto::CompileOptions,
    ) -> Result<CompileResult> {
        let tmp_dir = std::env::temp_dir();
        let tmp_file = tmp_dir.join(format!("pacinet-rules-{}.yaml", uuid::Uuid::new_v4()));

        tokio::fs::write(&tmp_file, rules_yaml)
            .await
            .context("Failed to write temp rules file")?;

        debug!(path = %tmp_file.display(), "Wrote rules to temp file");

        let result = self.run_with_preflight(&tmp_file, options).await;

        let _ = tokio::fs::remove_file(&tmp_file).await;
        result
    }

    async fn run_with_preflight(
        &self,
        rules_path: &PathBuf,
        options: &pacinet_proto::CompileOptions,
    ) -> Result<CompileResult> {
        // Semantic preflight: validate then lint before compile.
        self.run_validate(rules_path).await?;
        let lint_warnings = self.run_lint(rules_path, options).await?;

        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.arg("compile").arg(rules_path).arg("--json");
        apply_compile_flags(&mut cmd, options);

        info!(binary = %self.binary, "Running PacGate compile");

        let output = cmd.output().await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if !output.status.success() {
                    return Ok(CompileResult {
                        success: false,
                        message: format!("Compilation failed: {}", stderr),
                        warnings: lint_warnings,
                        rules_count: None,
                        output_dir: None,
                    });
                }

                let parsed: PacGateCompileOutput = serde_json::from_str(&stdout)
                    .with_context(|| format!("Failed to parse pacgate compile JSON: {}", stdout))?;

                if parsed.status != "ok" {
                    return Ok(CompileResult {
                        success: false,
                        message: format!("PacGate compile returned status={}.", parsed.status),
                        warnings: lint_warnings,
                        rules_count: parsed.rules_count,
                        output_dir: parsed.output_dir,
                    });
                }

                let mut warnings = lint_warnings;
                warnings.extend(parsed.warnings);
                if !stderr.is_empty() {
                    warnings.push(stderr);
                }

                Ok(CompileResult {
                    success: true,
                    message: "Compilation successful".to_string(),
                    warnings,
                    rules_count: parsed.rules_count,
                    output_dir: parsed.output_dir,
                })
            }
            Err(e) => Ok(CompileResult {
                success: false,
                message: format!("PacGate not available: {}", e),
                warnings: vec!["pacgate binary not found in PATH".to_string()],
                rules_count: None,
                output_dir: None,
            }),
        }
    }

    async fn run_validate(&self, rules_path: &PathBuf) -> Result<()> {
        let output = tokio::process::Command::new(&self.binary)
            .arg("validate")
            .arg(rules_path)
            .arg("--json")
            .output()
            .await
            .context("Failed to execute pacgate validate")?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("pacgate validate failed: {}", stderr);
    }

    async fn run_lint(
        &self,
        rules_path: &PathBuf,
        options: &pacinet_proto::CompileOptions,
    ) -> Result<Vec<String>> {
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.arg("lint").arg(rules_path).arg("--json");
        if options.dynamic {
            cmd.arg("--dynamic")
                .arg("--dynamic-entries")
                .arg(options.dynamic_entries.max(1).to_string());
        }
        if options.width > 0 {
            cmd.arg("--width").arg(options.width.to_string());
        }
        if !options.target.is_empty() {
            cmd.arg("--target").arg(&options.target);
        }
        if options.ptp {
            cmd.arg("--ptp");
        }
        if options.rss || options.rss_queues != 4 {
            cmd.arg("--rss")
                .arg("--rss-queues")
                .arg(options.rss_queues.max(1).to_string());
        }
        if options.int_enabled || options.int_switch_id != 0 {
            cmd.arg("--int")
                .arg("--int-switch-id")
                .arg(options.int_switch_id.to_string());
        }

        let output = cmd
            .output()
            .await
            .context("Failed to execute pacgate lint")?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("pacgate lint failed: {}", stderr);
        }

        let parsed: PacGateLintOutput = match serde_json::from_str(&stdout) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "Could not parse pacgate lint JSON output");
                return Ok(vec![]);
            }
        };

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        for finding in parsed.findings {
            let line = if finding.code.is_empty() {
                finding.message
            } else {
                format!("{}: {}", finding.code, finding.message)
            };
            match finding.level.as_str() {
                "error" => errors.push(line),
                "warning" => warnings.push(line),
                _ => {}
            }
        }

        if !errors.is_empty() {
            anyhow::bail!("pacgate lint errors: {}", errors.join(" | "));
        }

        Ok(warnings)
    }
}

fn apply_compile_flags(cmd: &mut tokio::process::Command, options: &pacinet_proto::CompileOptions) {
    if options.counters {
        cmd.arg("--counters");
    }
    if options.rate_limit {
        cmd.arg("--rate-limit");
    }
    if options.conntrack {
        cmd.arg("--conntrack");
    }
    if options.axi {
        cmd.arg("--axi");
    }

    let ports = options.ports.max(1);
    if ports > 1 {
        cmd.arg("--ports").arg(ports.to_string());
    }

    if !options.target.is_empty() {
        cmd.arg("--target").arg(&options.target);
    }

    if options.dynamic {
        cmd.arg("--dynamic");
        cmd.arg("--dynamic-entries")
            .arg(options.dynamic_entries.max(1).to_string());
    }
    if options.width > 0 {
        cmd.arg("--width").arg(options.width.to_string());
    }
    if options.ptp {
        cmd.arg("--ptp");
    }
    if options.rss || options.rss_queues != 4 {
        cmd.arg("--rss")
            .arg("--rss-queues")
            .arg(options.rss_queues.max(1).to_string());
    }
    if options.int_enabled || options.int_switch_id != 0 {
        cmd.arg("--int")
            .arg("--int-switch-id")
            .arg(options.int_switch_id.to_string());
    }
}
