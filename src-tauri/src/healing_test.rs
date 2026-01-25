//! Self-Healing OS Test Harness
//!
//! This module provides a framework for testing whether an LLM can correctly
//! diagnose and repair system faults on the FIRST TRY given proper context.
//!
//! NOTE: All fault scenarios in this module are SIMULATED for testing purposes.
//! They do not represent real system failures - they are synthetic test cases
//! designed to measure LLM accuracy for system healing tasks.

use crate::memory::SecurityTier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A simulated system fault for testing LLM diagnosis accuracy
/// NOTE: These are SYNTHETIC test scenarios, not real system states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultScenario {
    /// Unique identifier for this scenario
    pub id: String,
    /// Human-readable description
    pub name: String,
    /// The fault category
    pub category: FaultCategory,
    /// Simulated system state (what the LLM sees as context)
    pub system_state: SystemState,
    /// The correct diagnosis (ground truth)
    pub correct_diagnosis: Diagnosis,
    /// The correct repair action (ground truth)
    pub correct_repair: RepairAction,
    /// Difficulty level (for stratified testing)
    pub difficulty: Difficulty,
}

/// Categories of faults for testing
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FaultCategory {
    /// Memory-related issues (leaks, exhaustion)
    Memory,
    /// Configuration errors
    Config,
    /// Storage/disk issues
    Storage,
    /// Network connectivity issues
    Network,
    /// Service crash/restart needed
    ServiceCrash,
    /// Resource contention
    ResourceContention,
    /// Security-related (but within safe repair scope)
    Security,
}

/// Difficulty levels for fault scenarios
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Ord, PartialOrd, Eq)]
pub enum Difficulty {
    /// Single symptom, obvious fix
    Easy,
    /// Multiple symptoms, clear pattern
    Medium,
    /// Ambiguous symptoms, requires reasoning
    Hard,
    /// Multiple interacting faults
    Expert,
}

/// Simulated system state provided as context to the LLM
/// NOTE: This is SYNTHETIC data for testing, not real system state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemState {
    /// Simulated log entries (tier 0 - public)
    pub logs: Vec<LogEntry>,
    /// Simulated metrics (tier 0 - public)
    pub metrics: HashMap<String, MetricValue>,
    /// Simulated config state (tier 1 - internal)
    pub config: HashMap<String, serde_json::Value>,
    /// Simulated process list (tier 1 - internal)
    pub processes: Vec<ProcessInfo>,
    /// Any error messages visible
    pub errors: Vec<String>,
}

/// A simulated log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub message: String,
}

/// A simulated metric value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    Gauge(f64),
    Counter(u64),
    Percentage(f64),
}

/// Simulated process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub state: String,
    pub memory_mb: u64,
    pub cpu_percent: f64,
}

/// The diagnosis an LLM should produce
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnosis {
    /// Root cause identification
    pub root_cause: String,
    /// Affected component
    pub affected_component: String,
    /// Severity assessment
    pub severity: Severity,
    /// Confidence (for ground truth, always 1.0)
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// A repair action the LLM should propose
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepairAction {
    /// The action type (from a predefined playbook)
    pub action_type: ActionType,
    /// Target of the action
    pub target: String,
    /// Parameters for the action
    pub parameters: HashMap<String, String>,
}

/// Predefined safe action types (the "playbook")
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActionType {
    /// Restart a service
    RestartService,
    /// Reload configuration
    ReloadConfig,
    /// Clear cache
    ClearCache,
    /// Increase resource limit
    IncreaseLimit,
    /// Rotate logs
    RotateLogs,
    /// Kill process
    KillProcess,
    /// Update config value
    UpdateConfig,
    /// No action needed (false alarm)
    NoAction,
    /// Escalate to human
    Escalate,
}

/// Result of testing an LLM's response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub scenario_id: String,
    pub diagnosis_correct: bool,
    pub repair_correct: bool,
    pub diagnosis_partial_credit: f64,
    pub repair_partial_credit: f64,
    pub llm_diagnosis: Option<Diagnosis>,
    pub llm_repair: Option<RepairAction>,
    pub response_time_ms: u64,
}

/// The test harness that manages scenarios and evaluates LLM responses
pub struct HealingTestHarness {
    scenarios: Vec<FaultScenario>,
    results: Vec<TestResult>,
}

impl HealingTestHarness {
    pub fn new() -> Self {
        Self {
            scenarios: Self::create_builtin_scenarios(),
            results: Vec::new(),
        }
    }

    /// Create the built-in test scenarios
    /// NOTE: All scenarios are SYNTHETIC/SIMULATED for testing purposes
    fn create_builtin_scenarios() -> Vec<FaultScenario> {
        vec![
            // === EASY: Single symptom, obvious fix ===
            FaultScenario {
                id: "mem-001".to_string(),
                name: "Memory leak in document server".to_string(),
                category: FaultCategory::Memory,
                difficulty: Difficulty::Easy,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T10:00:00Z".to_string(),
                            level: "WARN".to_string(),
                            source: "document-server".to_string(),
                            message: "Memory usage at 85%".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T10:05:00Z".to_string(),
                            level: "WARN".to_string(),
                            source: "document-server".to_string(),
                            message: "Memory usage at 90%".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T10:10:00Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "document-server".to_string(),
                            message: "Memory usage at 95%, OOM risk".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("document_server.memory_mb".to_string(), MetricValue::Gauge(1900.0)),
                        ("document_server.memory_limit_mb".to_string(), MetricValue::Gauge(2048.0)),
                        ("document_server.uptime_hours".to_string(), MetricValue::Counter(168)),
                    ]),
                    config: HashMap::new(),
                    processes: vec![
                        ProcessInfo {
                            pid: 1234,
                            name: "document-server".to_string(),
                            state: "running".to_string(),
                            memory_mb: 1900,
                            cpu_percent: 15.0,
                        },
                    ],
                    errors: vec!["Memory allocation slow".to_string()],
                },
                correct_diagnosis: Diagnosis {
                    root_cause: "Memory leak causing gradual memory exhaustion".to_string(),
                    affected_component: "document-server".to_string(),
                    severity: Severity::High,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::RestartService,
                    target: "document-server".to_string(),
                    parameters: HashMap::new(),
                },
            },

            // === MEDIUM: Multiple symptoms, clear pattern ===
            FaultScenario {
                id: "cfg-001".to_string(),
                name: "Invalid config causing connection failures".to_string(),
                category: FaultCategory::Config,
                difficulty: Difficulty::Medium,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T09:00:00Z".to_string(),
                            level: "INFO".to_string(),
                            source: "config-manager".to_string(),
                            message: "Config reloaded from /etc/app/config.yaml".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T09:00:01Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "api-gateway".to_string(),
                            message: "Connection refused to backend:8080".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T09:00:02Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "api-gateway".to_string(),
                            message: "Connection refused to backend:8080".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("api_gateway.requests_failed".to_string(), MetricValue::Counter(150)),
                        ("api_gateway.backend_connections".to_string(), MetricValue::Gauge(0.0)),
                    ]),
                    config: HashMap::from([
                        ("backend.host".to_string(), serde_json::json!("backend")),
                        ("backend.port".to_string(), serde_json::json!(8080)),
                    ]),
                    processes: vec![
                        ProcessInfo {
                            pid: 2345,
                            name: "api-gateway".to_string(),
                            state: "running".to_string(),
                            memory_mb: 256,
                            cpu_percent: 2.0,
                        },
                    ],
                    errors: vec!["Backend unreachable".to_string()],
                },
                correct_diagnosis: Diagnosis {
                    root_cause: "Config change introduced incorrect backend address".to_string(),
                    affected_component: "api-gateway".to_string(),
                    severity: Severity::High,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::ReloadConfig,
                    target: "api-gateway".to_string(),
                    parameters: HashMap::from([
                        ("rollback".to_string(), "true".to_string()),
                    ]),
                },
            },

            // === HARD: Ambiguous symptoms ===
            FaultScenario {
                id: "disk-001".to_string(),
                name: "Disk full causing cascading failures".to_string(),
                category: FaultCategory::Storage,
                difficulty: Difficulty::Hard,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T08:00:00Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "database".to_string(),
                            message: "Write failed: No space left on device".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T08:00:01Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "logger".to_string(),
                            message: "Failed to write log entry".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T08:00:02Z".to_string(),
                            level: "WARN".to_string(),
                            source: "cache-server".to_string(),
                            message: "Cache persistence disabled".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("disk.used_percent".to_string(), MetricValue::Percentage(99.8)),
                        ("disk.available_gb".to_string(), MetricValue::Gauge(0.2)),
                        ("logs.size_gb".to_string(), MetricValue::Gauge(45.0)),
                    ]),
                    config: HashMap::from([
                        ("log.retention_days".to_string(), serde_json::json!(90)),
                        ("log.max_size_gb".to_string(), serde_json::json!(50)),
                    ]),
                    processes: vec![],
                    errors: vec![
                        "Database read-only mode".to_string(),
                        "Log rotation failed".to_string(),
                    ],
                },
                correct_diagnosis: Diagnosis {
                    root_cause: "Disk space exhausted by excessive log retention".to_string(),
                    affected_component: "storage".to_string(),
                    severity: Severity::Critical,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::RotateLogs,
                    target: "/var/log".to_string(),
                    parameters: HashMap::from([
                        ("delete_older_than_days".to_string(), "7".to_string()),
                    ]),
                },
            },

            // === HARD: Red herring - high CPU is symptom, not cause ===
            // NOTE: SIMULATED scenario - the obvious answer (kill high-CPU process) is WRONG
            FaultScenario {
                id: "trap-001".to_string(),
                name: "High CPU is symptom not cause (deadlock)".to_string(),
                category: FaultCategory::ResourceContention,
                difficulty: Difficulty::Hard,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T11:00:00Z".to_string(),
                            level: "WARN".to_string(),
                            source: "worker-pool".to_string(),
                            message: "Worker thread blocked waiting for lock".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T11:00:05Z".to_string(),
                            level: "WARN".to_string(),
                            source: "scheduler".to_string(),
                            message: "Task queue backing up, 500 pending tasks".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T11:00:10Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "health-check".to_string(),
                            message: "Service unresponsive for 30 seconds".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("scheduler.cpu_percent".to_string(), MetricValue::Percentage(98.0)),
                        ("scheduler.threads_blocked".to_string(), MetricValue::Counter(12)),
                        ("scheduler.threads_total".to_string(), MetricValue::Counter(16)),
                        ("scheduler.pending_tasks".to_string(), MetricValue::Counter(500)),
                        ("worker_pool.lock_wait_ms".to_string(), MetricValue::Gauge(30000.0)),
                    ]),
                    config: HashMap::from([
                        ("scheduler.max_threads".to_string(), serde_json::json!(16)),
                        ("scheduler.task_timeout_ms".to_string(), serde_json::json!(5000)),
                    ]),
                    processes: vec![
                        ProcessInfo {
                            pid: 5555,
                            name: "scheduler".to_string(),
                            state: "running".to_string(),
                            memory_mb: 512,
                            cpu_percent: 98.0,
                        },
                    ],
                    errors: vec![
                        "Request timeout".to_string(),
                        "Thread pool exhausted".to_string(),
                    ],
                },
                // TRAP: The obvious answer is KillProcess or IncreaseLimit for CPU
                // CORRECT: The high CPU is from spin-waiting on a deadlock - restart clears it
                correct_diagnosis: Diagnosis {
                    root_cause: "Deadlock causing threads to spin-wait, manifesting as high CPU".to_string(),
                    affected_component: "scheduler".to_string(),
                    severity: Severity::Critical,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::RestartService,
                    target: "scheduler".to_string(),
                    parameters: HashMap::new(),
                },
            },

            // === HARD: False alarm - looks bad but is normal ===
            // NOTE: SIMULATED scenario - the system is actually healthy
            FaultScenario {
                id: "trap-002".to_string(),
                name: "False alarm - batch job spike is normal".to_string(),
                category: FaultCategory::ResourceContention,
                difficulty: Difficulty::Hard,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T02:00:00Z".to_string(),
                            level: "INFO".to_string(),
                            source: "cron".to_string(),
                            message: "Starting nightly batch job: report-generator".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T02:00:05Z".to_string(),
                            level: "INFO".to_string(),
                            source: "report-generator".to_string(),
                            message: "Processing 50000 records".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T02:01:00Z".to_string(),
                            level: "WARN".to_string(),
                            source: "monitor".to_string(),
                            message: "High memory usage detected".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("system.memory_percent".to_string(), MetricValue::Percentage(85.0)),
                        ("system.cpu_percent".to_string(), MetricValue::Percentage(75.0)),
                        ("report_generator.records_processed".to_string(), MetricValue::Counter(25000)),
                        ("report_generator.expected_records".to_string(), MetricValue::Counter(50000)),
                        ("system.time".to_string(), MetricValue::Gauge(2.0)), // 2 AM
                    ]),
                    config: HashMap::from([
                        ("batch.schedule".to_string(), serde_json::json!("0 2 * * *")),
                        ("batch.memory_limit_percent".to_string(), serde_json::json!(90)),
                    ]),
                    processes: vec![
                        ProcessInfo {
                            pid: 6666,
                            name: "report-generator".to_string(),
                            state: "running".to_string(),
                            memory_mb: 3400,
                            cpu_percent: 75.0,
                        },
                    ],
                    errors: vec![],
                },
                // TRAP: Looks like memory/CPU problem needing intervention
                // CORRECT: This is a scheduled batch job running within limits - no action needed
                correct_diagnosis: Diagnosis {
                    root_cause: "Normal scheduled batch job - resource usage within configured limits".to_string(),
                    affected_component: "report-generator".to_string(),
                    severity: Severity::Low,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::NoAction,
                    target: "".to_string(),
                    parameters: HashMap::new(),
                },
            },

            // === EXPERT: Multi-fault with wrong obvious culprit ===
            // NOTE: SIMULATED scenario - two faults, the visible one is secondary
            FaultScenario {
                id: "expert-001".to_string(),
                name: "Cache failure masking database connection exhaustion".to_string(),
                category: FaultCategory::ResourceContention,
                difficulty: Difficulty::Expert,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T14:00:00Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "cache".to_string(),
                            message: "Redis connection failed: ETIMEDOUT".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T14:00:01Z".to_string(),
                            level: "WARN".to_string(),
                            source: "api".to_string(),
                            message: "Cache miss, falling back to database".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T14:00:02Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "database".to_string(),
                            message: "Connection pool exhausted (max: 50)".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T14:00:03Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "api".to_string(),
                            message: "Request failed: no database connections available".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("cache.hit_rate".to_string(), MetricValue::Percentage(0.0)),
                        ("cache.connected".to_string(), MetricValue::Gauge(0.0)),
                        ("database.connections_used".to_string(), MetricValue::Counter(50)),
                        ("database.connections_max".to_string(), MetricValue::Counter(50)),
                        ("database.connection_wait_ms".to_string(), MetricValue::Gauge(5000.0)),
                        ("api.requests_per_sec".to_string(), MetricValue::Gauge(1000.0)),
                    ]),
                    config: HashMap::from([
                        ("database.pool_size".to_string(), serde_json::json!(50)),
                        ("cache.host".to_string(), serde_json::json!("redis-primary")),
                    ]),
                    processes: vec![
                        ProcessInfo {
                            pid: 7777,
                            name: "api-server".to_string(),
                            state: "running".to_string(),
                            memory_mb: 1024,
                            cpu_percent: 45.0,
                        },
                    ],
                    errors: vec![
                        "Cache unavailable".to_string(),
                        "Database connection timeout".to_string(),
                    ],
                },
                // TRAP: Cache failure is visible and seems like the problem
                // CORRECT: Cache failure caused traffic to hit DB directly, exhausting pool
                // Fix the cache first - that's the root cause; DB pool exhaustion is secondary
                correct_diagnosis: Diagnosis {
                    root_cause: "Cache failure causing all requests to hit database, exhausting connection pool".to_string(),
                    affected_component: "cache".to_string(),
                    severity: Severity::Critical,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::RestartService,
                    target: "cache".to_string(),
                    parameters: HashMap::new(),
                },
            },

            // === EXPERT: Recent change is a red herring ===
            // NOTE: SIMULATED scenario - timing correlation is misleading
            FaultScenario {
                id: "expert-002".to_string(),
                name: "Coincidental timing - deploy is not the cause".to_string(),
                category: FaultCategory::Network,
                difficulty: Difficulty::Expert,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T15:00:00Z".to_string(),
                            level: "INFO".to_string(),
                            source: "deploy".to_string(),
                            message: "Deployed version 2.3.4 of payment-service".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T15:00:30Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "payment-service".to_string(),
                            message: "Failed to connect to payment-gateway.external.com".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T15:00:31Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "payment-service".to_string(),
                            message: "DNS resolution failed for payment-gateway.external.com".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T15:00:35Z".to_string(),
                            level: "INFO".to_string(),
                            source: "dns-monitor".to_string(),
                            message: "External DNS server 8.8.8.8 unreachable".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("payment_service.success_rate".to_string(), MetricValue::Percentage(0.0)),
                        ("payment_service.version".to_string(), MetricValue::Gauge(234.0)),
                        ("dns.external_queries_failed".to_string(), MetricValue::Counter(150)),
                        ("dns.internal_queries_ok".to_string(), MetricValue::Counter(500)),
                        ("network.external_connectivity".to_string(), MetricValue::Gauge(0.0)),
                    ]),
                    config: HashMap::from([
                        ("payment.gateway_url".to_string(), serde_json::json!("https://payment-gateway.external.com")),
                        ("dns.servers".to_string(), serde_json::json!(["8.8.8.8", "8.8.4.4"])),
                    ]),
                    processes: vec![
                        ProcessInfo {
                            pid: 8888,
                            name: "payment-service".to_string(),
                            state: "running".to_string(),
                            memory_mb: 256,
                            cpu_percent: 5.0,
                        },
                    ],
                    errors: vec![
                        "Payment processing failed".to_string(),
                        "External service unreachable".to_string(),
                    ],
                },
                // TRAP: Deploy just happened - obvious to blame the deploy and rollback
                // CORRECT: DNS/network issue is the real cause; internal DNS works, external doesn't
                // The deploy is coincidental timing
                correct_diagnosis: Diagnosis {
                    root_cause: "External network/DNS connectivity failure - not related to recent deploy".to_string(),
                    affected_component: "network".to_string(),
                    severity: Severity::Critical,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::Escalate,
                    target: "network-team".to_string(),
                    parameters: HashMap::from([
                        ("reason".to_string(), "External network connectivity failure requires infrastructure investigation".to_string()),
                    ]),
                },
            },

            // === EXPERT: Cascading failure - must fix in correct order ===
            // NOTE: SIMULATED scenario - multiple components failing
            FaultScenario {
                id: "expert-003".to_string(),
                name: "Cascading failure from certificate expiry".to_string(),
                category: FaultCategory::Security,
                difficulty: Difficulty::Expert,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T00:00:01Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "load-balancer".to_string(),
                            message: "TLS handshake failed: certificate expired".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T00:00:02Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "service-mesh".to_string(),
                            message: "mTLS authentication failed to backend services".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T00:00:03Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "api-gateway".to_string(),
                            message: "All backend health checks failing".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T00:00:05Z".to_string(),
                            level: "WARN".to_string(),
                            source: "autoscaler".to_string(),
                            message: "Scaling up due to unhealthy instances".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("loadbalancer.healthy_backends".to_string(), MetricValue::Counter(0)),
                        ("loadbalancer.tls_errors".to_string(), MetricValue::Counter(5000)),
                        ("certificate.days_until_expiry".to_string(), MetricValue::Gauge(-1.0)),
                        ("api.requests_failed".to_string(), MetricValue::Counter(10000)),
                        ("autoscaler.instances".to_string(), MetricValue::Counter(20)),
                    ]),
                    config: HashMap::from([
                        ("tls.cert_path".to_string(), serde_json::json!("/etc/ssl/server.crt")),
                        ("tls.auto_renew".to_string(), serde_json::json!(false)),
                    ]),
                    processes: vec![
                        ProcessInfo {
                            pid: 1001,
                            name: "load-balancer".to_string(),
                            state: "running".to_string(),
                            memory_mb: 128,
                            cpu_percent: 90.0,
                        },
                        ProcessInfo {
                            pid: 1002,
                            name: "api-gateway".to_string(),
                            state: "running".to_string(),
                            memory_mb: 256,
                            cpu_percent: 5.0,
                        },
                    ],
                    errors: vec![
                        "Certificate expired".to_string(),
                        "All backends unhealthy".to_string(),
                        "Autoscaler spinning up instances".to_string(),
                    ],
                },
                // TRAP: Many things look broken (backends, autoscaler, high CPU on LB)
                // CORRECT: Certificate expiry is root cause; cannot be auto-fixed, must escalate
                // Restarting services won't help - need cert renewal
                correct_diagnosis: Diagnosis {
                    root_cause: "TLS certificate expired causing cascading mTLS and health check failures".to_string(),
                    affected_component: "load-balancer".to_string(),
                    severity: Severity::Critical,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::Escalate,
                    target: "security-team".to_string(),
                    parameters: HashMap::from([
                        ("reason".to_string(), "TLS certificate expired - requires manual renewal".to_string()),
                    ]),
                },
            },

            // === EXPERT: Subtle resource leak with misleading symptoms ===
            // NOTE: SIMULATED scenario - file descriptor leak
            FaultScenario {
                id: "expert-004".to_string(),
                name: "File descriptor leak causing random failures".to_string(),
                category: FaultCategory::Memory,
                difficulty: Difficulty::Expert,
                system_state: SystemState {
                    logs: vec![
                        LogEntry {
                            timestamp: "2026-01-24T12:00:00Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "file-processor".to_string(),
                            message: "Failed to open file: Too many open files".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T12:00:01Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "http-client".to_string(),
                            message: "Socket creation failed: Too many open files".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T12:00:02Z".to_string(),
                            level: "ERROR".to_string(),
                            source: "database-client".to_string(),
                            message: "Connection failed: Too many open files".to_string(),
                        },
                        LogEntry {
                            timestamp: "2026-01-24T11:00:00Z".to_string(),
                            level: "INFO".to_string(),
                            source: "file-processor".to_string(),
                            message: "Processed batch of 1000 files".to_string(),
                        },
                    ],
                    metrics: HashMap::from([
                        ("process.open_fds".to_string(), MetricValue::Counter(65535)),
                        ("process.fd_limit".to_string(), MetricValue::Counter(65536)),
                        ("process.memory_mb".to_string(), MetricValue::Gauge(512.0)),
                        ("process.uptime_hours".to_string(), MetricValue::Counter(72)),
                        ("file_processor.files_processed".to_string(), MetricValue::Counter(50000)),
                    ]),
                    config: HashMap::from([
                        ("process.fd_limit".to_string(), serde_json::json!(65536)),
                        ("batch.size".to_string(), serde_json::json!(1000)),
                    ]),
                    processes: vec![
                        ProcessInfo {
                            pid: 9999,
                            name: "file-processor".to_string(),
                            state: "running".to_string(),
                            memory_mb: 512,
                            cpu_percent: 25.0,
                        },
                    ],
                    errors: vec![
                        "Too many open files".to_string(),
                        "Socket creation failed".to_string(),
                    ],
                },
                // TRAP: Could increase fd_limit, but that just delays the problem
                // CORRECT: File descriptor leak - needs restart to clear, then fix the bug
                correct_diagnosis: Diagnosis {
                    root_cause: "File descriptor leak exhausting system limit after processing many files".to_string(),
                    affected_component: "file-processor".to_string(),
                    severity: Severity::High,
                    confidence: 1.0,
                },
                correct_repair: RepairAction {
                    action_type: ActionType::RestartService,
                    target: "file-processor".to_string(),
                    parameters: HashMap::new(),
                },
            },
        ]
    }

    /// Get all scenarios
    pub fn scenarios(&self) -> &[FaultScenario] {
        &self.scenarios
    }

    /// Get scenarios by difficulty
    pub fn scenarios_by_difficulty(&self, difficulty: Difficulty) -> Vec<&FaultScenario> {
        self.scenarios.iter().filter(|s| s.difficulty == difficulty).collect()
    }

    /// Format system state as context for LLM
    /// This is what gets sent to the LLM for diagnosis
    pub fn format_context_for_llm(state: &SystemState, max_tier: SecurityTier) -> String {
        let mut context = String::new();

        // Logs are tier 0 (public) - always visible
        context.push_str("=== SYSTEM LOGS ===\n");
        for log in &state.logs {
            context.push_str(&format!(
                "[{}] {} [{}]: {}\n",
                log.timestamp, log.level, log.source, log.message
            ));
        }

        // Metrics are tier 0 (public) - always visible
        context.push_str("\n=== METRICS ===\n");
        for (name, value) in &state.metrics {
            let v = match value {
                MetricValue::Gauge(g) => format!("{:.2}", g),
                MetricValue::Counter(c) => c.to_string(),
                MetricValue::Percentage(p) => format!("{:.1}%", p),
            };
            context.push_str(&format!("{}: {}\n", name, v));
        }

        // Config is tier 1 (internal) - only if agent has tier >= 1
        if max_tier >= SecurityTier::Internal {
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

        // Errors summary
        if !state.errors.is_empty() {
            context.push_str("\n=== ERRORS ===\n");
            for err in &state.errors {
                context.push_str(&format!("- {}\n", err));
            }
        }

        context
    }

    /// Create the prompt for the LLM
    pub fn create_diagnosis_prompt(scenario: &FaultScenario, max_tier: SecurityTier) -> String {
        let context = Self::format_context_for_llm(&scenario.system_state, max_tier);

        format!(
            r#"You are a self-healing OS agent. Analyze the following system state and provide:
1. A diagnosis (root cause, affected component, severity)
2. A repair action from the allowed playbook

ALLOWED REPAIR ACTIONS:
- RestartService(target): Restart a service
- ReloadConfig(target, rollback=true/false): Reload configuration
- ClearCache(target): Clear a cache
- IncreaseLimit(target, resource, value): Increase a resource limit
- RotateLogs(path, delete_older_than_days): Rotate/clean logs
- KillProcess(pid): Kill a process
- UpdateConfig(key, value): Update a config value
- NoAction: No action needed
- Escalate(reason): Escalate to human operator

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
    "parameters": {{ "key": "value" }}
  }}
}}

Analyze and respond:"#,
            context = context
        )
    }

    /// Evaluate an LLM's response against ground truth
    pub fn evaluate_response(
        &self,
        scenario: &FaultScenario,
        llm_diagnosis: &Diagnosis,
        llm_repair: &RepairAction,
    ) -> TestResult {
        // Exact match for diagnosis
        let diagnosis_exact = llm_diagnosis.root_cause.to_lowercase().contains(
            &scenario.correct_diagnosis.root_cause.to_lowercase().split_whitespace().next().unwrap_or("")
        ) && llm_diagnosis.affected_component == scenario.correct_diagnosis.affected_component;

        // Partial credit for diagnosis
        let diagnosis_partial = if diagnosis_exact {
            1.0
        } else if llm_diagnosis.affected_component == scenario.correct_diagnosis.affected_component {
            0.5
        } else {
            0.0
        };

        // Exact match for repair
        let repair_exact = llm_repair.action_type == scenario.correct_repair.action_type
            && llm_repair.target == scenario.correct_repair.target;

        // Partial credit for repair
        let repair_partial = if repair_exact {
            1.0
        } else if llm_repair.action_type == scenario.correct_repair.action_type {
            0.5
        } else {
            0.0
        };

        TestResult {
            scenario_id: scenario.id.clone(),
            diagnosis_correct: diagnosis_exact,
            repair_correct: repair_exact,
            diagnosis_partial_credit: diagnosis_partial,
            repair_partial_credit: repair_partial,
            llm_diagnosis: Some(llm_diagnosis.clone()),
            llm_repair: Some(llm_repair.clone()),
            response_time_ms: 0,
        }
    }

    /// Record a test result
    pub fn record_result(&mut self, result: TestResult) {
        self.results.push(result);
    }

    /// Get summary statistics
    pub fn summary(&self) -> TestSummary {
        let total = self.results.len();
        if total == 0 {
            return TestSummary::default();
        }

        let diagnosis_correct = self.results.iter().filter(|r| r.diagnosis_correct).count();
        let repair_correct = self.results.iter().filter(|r| r.repair_correct).count();
        let both_correct = self.results.iter().filter(|r| r.diagnosis_correct && r.repair_correct).count();

        let avg_diagnosis_score: f64 = self.results.iter().map(|r| r.diagnosis_partial_credit).sum::<f64>() / total as f64;
        let avg_repair_score: f64 = self.results.iter().map(|r| r.repair_partial_credit).sum::<f64>() / total as f64;

        TestSummary {
            total_scenarios: total,
            diagnosis_accuracy: diagnosis_correct as f64 / total as f64,
            repair_accuracy: repair_correct as f64 / total as f64,
            first_try_success_rate: both_correct as f64 / total as f64,
            avg_diagnosis_score,
            avg_repair_score,
        }
    }
}

impl Default for HealingTestHarness {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for test results
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestSummary {
    pub total_scenarios: usize,
    pub diagnosis_accuracy: f64,
    pub repair_accuracy: f64,
    /// The key metric: did the LLM get diagnosis AND repair correct on first try?
    pub first_try_success_rate: f64,
    pub avg_diagnosis_score: f64,
    pub avg_repair_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_creation() {
        let harness = HealingTestHarness::new();
        assert!(!harness.scenarios().is_empty(), "Should have built-in scenarios");
    }

    #[test]
    fn test_context_formatting() {
        let harness = HealingTestHarness::new();
        let scenario = &harness.scenarios()[0];

        // With tier 0, should see logs and metrics but not config
        let context_tier0 = HealingTestHarness::format_context_for_llm(&scenario.system_state, SecurityTier::Public);
        assert!(context_tier0.contains("LOGS"), "Should contain logs");
        assert!(context_tier0.contains("METRICS"), "Should contain metrics");
        assert!(!context_tier0.contains("CONFIGURATION"), "Should NOT contain config at tier 0");

        // With tier 1, should see everything
        let context_tier1 = HealingTestHarness::format_context_for_llm(&scenario.system_state, SecurityTier::Internal);
        assert!(context_tier1.contains("CONFIGURATION"), "Should contain config at tier 1");
    }

    #[test]
    fn test_prompt_generation() {
        let harness = HealingTestHarness::new();
        let scenario = &harness.scenarios()[0];

        let prompt = HealingTestHarness::create_diagnosis_prompt(scenario, SecurityTier::Internal);
        assert!(prompt.contains("self-healing OS agent"), "Prompt should set context");
        assert!(prompt.contains("RestartService"), "Prompt should list allowed actions");
    }
}
