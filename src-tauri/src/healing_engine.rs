//! Self-Healing Engine with Security Tiers
//!
//! This module implements a hybrid self-healing system that:
//! 1. Uses LLMs for diagnosis with security-tier filtered context
//! 2. Routes decisions based on confidence and action type
//! 3. Auto-executes safe, high-confidence repairs
//! 4. Escalates uncertain or sensitive actions to humans

use crate::healing_test::{
    ActionType, Diagnosis, FaultCategory, RepairAction, Severity, SystemState,
};
use crate::llm_client::{LlmClient, LlmError};
use crate::memory::SecurityTier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Decision made by the healing engine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealingDecision {
    /// Automatically execute the repair (high confidence, safe action)
    AutoExecute {
        action: RepairAction,
        reason: String,
    },
    /// Requires human approval before execution
    RequiresApproval {
        action: RepairAction,
        reason: String,
        confidence: f64,
    },
    /// Escalate to human operator (cannot be auto-fixed)
    Escalate {
        reason: String,
        suggested_team: String,
    },
    /// No action needed
    NoActionNeeded {
        reason: String,
    },
}

/// Result of LLM diagnosis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisResult {
    pub diagnosis: Diagnosis,
    pub repair: RepairAction,
    pub confidence: f64,
    pub reasoning: String,
}

/// Configuration for the healing engine
#[derive(Debug, Clone)]
pub struct HealingConfig {
    /// Minimum confidence for auto-execution
    pub auto_execute_threshold: f64,
    /// Actions that can be auto-executed
    pub safe_actions: Vec<ActionType>,
    /// Maximum security tier the LLM can see
    pub max_llm_tier: SecurityTier,
    /// Whether to require human approval for all actions
    pub human_in_loop: bool,
}

impl Default for HealingConfig {
    fn default() -> Self {
        Self {
            auto_execute_threshold: 0.8,
            safe_actions: vec![
                ActionType::RestartService,
                ActionType::ReloadConfig,
                ActionType::ClearCache,
                ActionType::RotateLogs,
                ActionType::NoAction,
            ],
            max_llm_tier: SecurityTier::Guarded, // LLM sees Open + Guarded (summaries)
            human_in_loop: false,
        }
    }
}

impl HealingConfig {
    /// Strict mode: all actions require human approval
    pub fn strict() -> Self {
        Self {
            human_in_loop: true,
            ..Default::default()
        }
    }

    /// Permissive mode: lower threshold, more auto-execution
    pub fn permissive() -> Self {
        Self {
            auto_execute_threshold: 0.6,
            safe_actions: vec![
                ActionType::RestartService,
                ActionType::ReloadConfig,
                ActionType::ClearCache,
                ActionType::RotateLogs,
                ActionType::IncreaseLimit,
                ActionType::KillProcess,
                ActionType::NoAction,
            ],
            ..Default::default()
        }
    }
}

/// The self-healing engine
pub struct HealingEngine {
    llm: LlmClient,
    config: HealingConfig,
}

impl HealingEngine {
    pub fn new(llm: LlmClient, config: HealingConfig) -> Self {
        Self { llm, config }
    }

    /// Analyze a fault and decide what to do
    pub fn analyze(&self, state: &SystemState) -> Result<(DiagnosisResult, HealingDecision), HealingError> {
        // 1. Build context with security tier filtering
        let context = self.build_context(state);

        // 2. Get LLM diagnosis
        let diagnosis_result = self.get_diagnosis(&context)?;

        // 3. Make decision based on confidence and action type
        let decision = self.make_decision(&diagnosis_result);

        Ok((diagnosis_result, decision))
    }

    /// Build security-tier filtered context for LLM
    ///
    /// Tier filtering:
    /// - Open: Logs, Metrics, Errors (always visible)
    /// - Guarded: Config, Processes (visible with Guarded access)
    /// - Sealed: Never included (e.g., credentials)
    fn build_context(&self, state: &SystemState) -> String {
        let mut context = String::new();

        // Logs are Open tier - always visible
        context.push_str("=== SYSTEM LOGS ===\n");
        for log in &state.logs {
            context.push_str(&format!(
                "[{}] {} [{}]: {}\n",
                log.timestamp, log.level, log.source, log.message
            ));
        }

        // Metrics are Open tier - always visible
        context.push_str("\n=== METRICS ===\n");
        for (name, value) in &state.metrics {
            use crate::healing_test::MetricValue;
            let v = match value {
                MetricValue::Gauge(g) => format!("{:.2}", g),
                MetricValue::Counter(c) => c.to_string(),
                MetricValue::Percentage(p) => format!("{:.1}%", p),
            };
            context.push_str(&format!("{}: {}\n", name, v));
        }

        // Config and processes are Guarded tier
        if self.config.max_llm_tier >= SecurityTier::Guarded {
            context.push_str("\n=== CONFIGURATION ===\n");
            for (key, value) in &state.config {
                context.push_str(&format!("{}: {}\n", key, value));
            }

            context.push_str("\n=== PROCESSES ===\n");
            for proc in &state.processes {
                context.push_str(&format!(
                    "PID {} - {} [{}] mem={}MB cpu={:.1}%\n",
                    proc.pid, proc.name, proc.state, proc.memory_mb, proc.cpu_percent
                ));
            }
        }

        // Errors summary (Open tier)
        if !state.errors.is_empty() {
            context.push_str("\n=== ERRORS ===\n");
            for err in &state.errors {
                context.push_str(&format!("- {}\n", err));
            }
        }

        context
    }

    /// Get diagnosis from LLM with confidence score
    fn get_diagnosis(&self, context: &str) -> Result<DiagnosisResult, HealingError> {
        let prompt = format!(
            r#"You are a self-healing OS agent. Analyze the system state and provide a diagnosis.

IMPORTANT: Also assess your CONFIDENCE (0.0 to 1.0) based on:
- How clear are the symptoms? (ambiguous = lower confidence)
- Is this a common pattern you recognize? (novel = lower confidence)
- Could there be multiple causes? (uncertain = lower confidence)

ALLOWED REPAIR ACTIONS:
- RestartService(target): Restart a service
- ReloadConfig(target): Reload configuration
- ClearCache(target): Clear a cache
- IncreaseLimit(target): Increase a resource limit
- RotateLogs(path): Rotate/clean logs
- KillProcess(pid): Kill a process
- UpdateConfig(key, value): Update a config value
- NoAction: No action needed (false alarm or within normal bounds)
- Escalate(team): Escalate to human operator

WHEN TO ESCALATE:
- External infrastructure issues (network, DNS, third-party services)
- Security issues (certificate expiry, auth failures)
- Issues requiring human judgment
- Low confidence in diagnosis

SYSTEM STATE:
{context}

Respond in this exact JSON format:
{{
  "diagnosis": {{
    "root_cause": "description of root cause",
    "affected_component": "component name",
    "severity": "Low|Medium|High|Critical"
  }},
  "repair": {{
    "action": "ActionName",
    "target": "target",
    "parameters": {{}}
  }},
  "confidence": 0.85,
  "reasoning": "Brief explanation of your diagnosis logic"
}}

Analyze and respond:"#,
            context = context
        );

        let response = self.llm.complete(&prompt).map_err(HealingError::LlmError)?;

        // Parse the response
        self.parse_diagnosis(&response.content)
    }

    /// Parse LLM response into DiagnosisResult
    fn parse_diagnosis(&self, content: &str) -> Result<DiagnosisResult, HealingError> {
        // Extract JSON from response
        let json_str = extract_json(content);

        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| HealingError::ParseError(format!("Invalid JSON: {}", e)))?;

        // Parse diagnosis
        let diag = parsed.get("diagnosis")
            .ok_or_else(|| HealingError::ParseError("Missing diagnosis".to_string()))?;

        let diagnosis = Diagnosis {
            root_cause: diag.get("root_cause")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            affected_component: diag.get("affected_component")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            severity: match diag.get("severity").and_then(|v| v.as_str()) {
                Some("Low") => Severity::Low,
                Some("Medium") => Severity::Medium,
                Some("High") => Severity::High,
                Some("Critical") => Severity::Critical,
                _ => Severity::Medium,
            },
            confidence: parsed.get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5),
        };

        // Parse repair action
        let repair = parsed.get("repair")
            .ok_or_else(|| HealingError::ParseError("Missing repair".to_string()))?;

        let action_str = repair.get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("Escalate");

        let action_type = parse_action_type(action_str)
            .ok_or_else(|| HealingError::ParseError(format!("Unknown action: {}", action_str)))?;

        let repair_action = RepairAction {
            action_type,
            target: repair.get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            parameters: repair.get("parameters")
                .and_then(|p| p.as_object())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
        };

        let confidence = parsed.get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        let reasoning = parsed.get("reasoning")
            .and_then(|v| v.as_str())
            .unwrap_or("No reasoning provided")
            .to_string();

        Ok(DiagnosisResult {
            diagnosis,
            repair: repair_action,
            confidence,
            reasoning,
        })
    }

    /// Decide whether to auto-execute, require approval, or escalate
    fn make_decision(&self, result: &DiagnosisResult) -> HealingDecision {
        // Always require human approval in strict mode
        if self.config.human_in_loop {
            return HealingDecision::RequiresApproval {
                action: result.repair.clone(),
                reason: "Human-in-loop mode enabled".to_string(),
                confidence: result.confidence,
            };
        }

        // NoAction is always safe
        if result.repair.action_type == ActionType::NoAction {
            return HealingDecision::NoActionNeeded {
                reason: result.reasoning.clone(),
            };
        }

        // Escalate actions always go to humans
        if result.repair.action_type == ActionType::Escalate {
            return HealingDecision::Escalate {
                reason: result.reasoning.clone(),
                suggested_team: result.repair.target.clone(),
            };
        }

        // Check if action is in safe list
        let is_safe_action = self.config.safe_actions.contains(&result.repair.action_type);

        // Check confidence threshold
        let is_high_confidence = result.confidence >= self.config.auto_execute_threshold;

        if is_safe_action && is_high_confidence {
            HealingDecision::AutoExecute {
                action: result.repair.clone(),
                reason: format!(
                    "Confidence {:.0}% >= {:.0}% threshold, safe action type",
                    result.confidence * 100.0,
                    self.config.auto_execute_threshold * 100.0
                ),
            }
        } else {
            let reason = if !is_safe_action {
                format!("{:?} is not in safe action list", result.repair.action_type)
            } else {
                format!(
                    "Confidence {:.0}% < {:.0}% threshold",
                    result.confidence * 100.0,
                    self.config.auto_execute_threshold * 100.0
                )
            };

            HealingDecision::RequiresApproval {
                action: result.repair.clone(),
                reason,
                confidence: result.confidence,
            }
        }
    }
}

/// Errors from the healing engine
#[derive(Debug)]
pub enum HealingError {
    LlmError(LlmError),
    ParseError(String),
    ExecutionError(String),
}

impl std::fmt::Display for HealingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealingError::LlmError(e) => write!(f, "LLM error: {}", e),
            HealingError::ParseError(e) => write!(f, "Parse error: {}", e),
            HealingError::ExecutionError(e) => write!(f, "Execution error: {}", e),
        }
    }
}

/// Extract JSON from LLM response (handles markdown code blocks)
fn extract_json(content: &str) -> String {
    let content = content.trim();

    // Try to find ```json ... ``` block
    if let Some(start) = content.find("```json") {
        if let Some(end) = content[start + 7..].find("```") {
            return content[start + 7..start + 7 + end].trim().to_string();
        }
    }

    // Try to find ``` ... ``` block
    if let Some(start) = content.find("```") {
        if let Some(end) = content[start + 3..].find("```") {
            return content[start + 3..start + 3 + end].trim().to_string();
        }
    }

    // Try to find { ... } block
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            return content[start..=end].to_string();
        }
    }

    content.to_string()
}

/// Parse action type string to ActionType enum
fn parse_action_type(s: &str) -> Option<ActionType> {
    let lower = s.to_lowercase().replace("_", "").replace(" ", "");
    match lower.as_str() {
        "restartservice" | "restart" => Some(ActionType::RestartService),
        "reloadconfig" | "reload" => Some(ActionType::ReloadConfig),
        "clearcache" | "cacheclear" => Some(ActionType::ClearCache),
        "increaselimit" | "raiselimit" => Some(ActionType::IncreaseLimit),
        "rotatelogs" | "logrotate" => Some(ActionType::RotateLogs),
        "killprocess" | "kill" => Some(ActionType::KillProcess),
        "updateconfig" => Some(ActionType::UpdateConfig),
        "noaction" | "none" => Some(ActionType::NoAction),
        "escalate" => Some(ActionType::Escalate),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_no_action() {
        let result = DiagnosisResult {
            diagnosis: Diagnosis {
                root_cause: "Normal operation".to_string(),
                affected_component: "system".to_string(),
                severity: Severity::Low,
                confidence: 0.9,
            },
            repair: RepairAction {
                action_type: ActionType::NoAction,
                target: String::new(),
                parameters: HashMap::new(),
            },
            confidence: 0.9,
            reasoning: "System operating normally".to_string(),
        };

        let config = HealingConfig::default();
        let decision = make_decision_standalone(&result, &config);

        assert!(matches!(decision, HealingDecision::NoActionNeeded { .. }));
    }

    #[test]
    fn test_decision_auto_execute() {
        let result = DiagnosisResult {
            diagnosis: Diagnosis {
                root_cause: "Memory leak".to_string(),
                affected_component: "service".to_string(),
                severity: Severity::High,
                confidence: 0.9,
            },
            repair: RepairAction {
                action_type: ActionType::RestartService,
                target: "my-service".to_string(),
                parameters: HashMap::new(),
            },
            confidence: 0.9,
            reasoning: "Clear memory leak pattern".to_string(),
        };

        let config = HealingConfig::default();
        let decision = make_decision_standalone(&result, &config);

        assert!(matches!(decision, HealingDecision::AutoExecute { .. }));
    }

    #[test]
    fn test_decision_requires_approval_low_confidence() {
        let result = DiagnosisResult {
            diagnosis: Diagnosis {
                root_cause: "Unknown issue".to_string(),
                affected_component: "service".to_string(),
                severity: Severity::Medium,
                confidence: 0.5,
            },
            repair: RepairAction {
                action_type: ActionType::RestartService,
                target: "my-service".to_string(),
                parameters: HashMap::new(),
            },
            confidence: 0.5,
            reasoning: "Uncertain diagnosis".to_string(),
        };

        let config = HealingConfig::default();
        let decision = make_decision_standalone(&result, &config);

        assert!(matches!(decision, HealingDecision::RequiresApproval { .. }));
    }

    // Standalone decision function for testing without LLM
    fn make_decision_standalone(result: &DiagnosisResult, config: &HealingConfig) -> HealingDecision {
        if config.human_in_loop {
            return HealingDecision::RequiresApproval {
                action: result.repair.clone(),
                reason: "Human-in-loop mode enabled".to_string(),
                confidence: result.confidence,
            };
        }

        if result.repair.action_type == ActionType::NoAction {
            return HealingDecision::NoActionNeeded {
                reason: result.reasoning.clone(),
            };
        }

        if result.repair.action_type == ActionType::Escalate {
            return HealingDecision::Escalate {
                reason: result.reasoning.clone(),
                suggested_team: result.repair.target.clone(),
            };
        }

        let is_safe_action = config.safe_actions.contains(&result.repair.action_type);
        let is_high_confidence = result.confidence >= config.auto_execute_threshold;

        if is_safe_action && is_high_confidence {
            HealingDecision::AutoExecute {
                action: result.repair.clone(),
                reason: format!(
                    "Confidence {:.0}% >= {:.0}% threshold",
                    result.confidence * 100.0,
                    config.auto_execute_threshold * 100.0
                ),
            }
        } else {
            HealingDecision::RequiresApproval {
                action: result.repair.clone(),
                reason: if !is_safe_action {
                    format!("{:?} not in safe list", result.repair.action_type)
                } else {
                    format!("Low confidence: {:.0}%", result.confidence * 100.0)
                },
                confidence: result.confidence,
            }
        }
    }
}
