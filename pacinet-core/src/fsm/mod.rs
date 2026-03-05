pub mod definition;
pub mod error;
pub mod instance;

pub use definition::{
    ActionDefinition, AlertAction, BasicAuth, CompileOptions as FsmCompileOptions,
    ConditionDefinition, CounterCondition, DeployAction, FsmDefinition, FsmKind, NodeSelector,
    RollbackAction, SimpleCondition, StateDefinition, TransitionDefinition, WebhookConfig,
};
pub use error::FsmError;
pub use instance::{
    ActionResult, FsmContext, FsmInstance, FsmInstanceStatus, FsmTransitionRecord,
    NodeActionResult, TransitionTrigger,
};

/// Parse a duration string like "5m", "30s", "1h", "2h30m" into std::time::Duration.
pub fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration string".to_string());
    }

    let mut total_secs: u64 = 0;
    let mut current_num = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else {
            if current_num.is_empty() {
                return Err(format!("unexpected '{}' without number", ch));
            }
            let n: u64 = current_num
                .parse()
                .map_err(|e| format!("invalid number: {}", e))?;
            current_num.clear();

            match ch {
                's' => total_secs += n,
                'm' => total_secs += n * 60,
                'h' => total_secs += n * 3600,
                'd' => total_secs += n * 86400,
                _ => return Err(format!("unknown duration unit: '{}'", ch)),
            }
        }
    }

    // Bare number without unit — treat as seconds
    if !current_num.is_empty() {
        return Err(format!(
            "missing duration unit for '{}' (use s/m/h/d)",
            current_num
        ));
    }

    if total_secs == 0 {
        return Err("duration must be greater than 0".to_string());
    }

    Ok(std::time::Duration::from_secs(total_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(
            parse_duration("30s").unwrap(),
            std::time::Duration::from_secs(30)
        );
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(
            parse_duration("5m").unwrap(),
            std::time::Duration::from_secs(300)
        );
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(
            parse_duration("1h").unwrap(),
            std::time::Duration::from_secs(3600)
        );
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(
            parse_duration("1h30m").unwrap(),
            std::time::Duration::from_secs(5400)
        );
    }

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(
            parse_duration("1d").unwrap(),
            std::time::Duration::from_secs(86400)
        );
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("5x").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("5").is_err());
    }
}
