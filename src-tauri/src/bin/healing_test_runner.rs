//! Self-Healing OS Test Runner
//!
//! This binary runs the healing test harness scenarios against local LLMs
//! and measures first-try accuracy for system healing tasks.
//!
//! NOTE: All scenarios are SIMULATED - they do not represent real system state.

use marlos_lib::healing_test::{HealingTestHarness, Difficulty, Diagnosis, RepairAction, ActionType, Severity};
use marlos_lib::llm_client::{LlmClient, LlmProvider};
use marlos_lib::memory::SecurityTier;
use std::env;
use std::collections::HashMap;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse command line args
    let mut provider = "lmstudio".to_string();
    let mut model = "".to_string();
    let mut show_prompts = false;
    let mut difficulty_filter: Option<Difficulty> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--provider" | "-p" => {
                if i + 1 < args.len() {
                    provider = args[i + 1].clone();
                    i += 1;
                }
            }
            "--model" | "-m" => {
                if i + 1 < args.len() {
                    model = args[i + 1].clone();
                    i += 1;
                }
            }
            "--show-prompts" => show_prompts = true,
            "--debug" => show_prompts = true, // Also shows raw responses
            "--easy" => difficulty_filter = Some(Difficulty::Easy),
            "--medium" => difficulty_filter = Some(Difficulty::Medium),
            "--hard" => difficulty_filter = Some(Difficulty::Hard),
            "--expert" => difficulty_filter = Some(Difficulty::Expert),
            "--help" | "-h" => {
                print_help();
                return;
            }
            _ => {}
        }
        i += 1;
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     Self-Healing OS Test Harness - LLM Accuracy Evaluation   ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ NOTE: All scenarios are SIMULATED for testing purposes.      ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Create LLM client
    let client = match provider.as_str() {
        "lmstudio" => {
            println!("Using LM Studio (localhost:4321)");
            LlmClient::lm_studio()
        }
        "ollama" => {
            let model_name = if model.is_empty() { "llama3.2" } else { &model };
            println!("Using Ollama with model: {}", model_name);
            LlmClient::ollama(model_name)
        }
        "claude" | "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| {
                eprintln!("Error: ANTHROPIC_API_KEY environment variable not set");
                eprintln!("Set it with: set ANTHROPIC_API_KEY=your-key-here");
                std::process::exit(1);
            });
            let model_name = if model.is_empty() { "claude-sonnet-4-20250514" } else { &model };
            println!("Using Claude API with model: {}", model_name);
            LlmClient::anthropic(&api_key, model_name)
        }
        "haiku" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| {
                eprintln!("Error: ANTHROPIC_API_KEY environment variable not set");
                std::process::exit(1);
            });
            println!("Using Claude Haiku (fast/cheap)");
            LlmClient::claude_haiku(&api_key)
        }
        "sonnet" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| {
                eprintln!("Error: ANTHROPIC_API_KEY environment variable not set");
                std::process::exit(1);
            });
            println!("Using Claude Sonnet");
            LlmClient::claude_sonnet(&api_key)
        }
        _ => {
            eprintln!("Unknown provider: {}. Use 'lmstudio', 'ollama', 'claude', 'haiku', or 'sonnet'", provider);
            return;
        }
    };

    // Check connection
    println!("Checking LLM connection...");
    match client.complete("Say 'OK' if you can hear me.") {
        Ok(response) => {
            println!("✓ Connected! Model: {}", response.model);
            println!();
        }
        Err(e) => {
            eprintln!("✗ Failed to connect to LLM: {}", e);
            eprintln!();
            eprintln!("Make sure LM Studio is running with a model loaded.");
            eprintln!("The server should be on http://localhost:4321");
            return;
        }
    }

    let harness = HealingTestHarness::new();
    let scenarios: Vec<_> = match difficulty_filter {
        Some(d) => harness.scenarios_by_difficulty(d).into_iter().cloned().collect(),
        None => harness.scenarios().to_vec(),
    };

    println!("Running {} test scenarios...\n", scenarios.len());

    let mut results = Vec::new();

    for (i, scenario) in scenarios.iter().enumerate() {
        println!("─────────────────────────────────────────────────────────────────");
        println!("SCENARIO {}/{}: {} [{}]", i + 1, scenarios.len(), scenario.name, scenario.id);
        println!("Difficulty: {:?}", scenario.difficulty);
        println!("─────────────────────────────────────────────────────────────────");

        let prompt = HealingTestHarness::create_diagnosis_prompt(scenario, SecurityTier::Guarded);

        if show_prompts {
            println!("\n=== PROMPT ===\n{}\n", prompt);
        }

        println!("Sending to LLM...");

        match client.complete(&prompt) {
            Ok(response) => {
                println!("Response received in {}ms", response.response_time_ms);

                if show_prompts {
                    println!("\n=== RAW RESPONSE ===\n{}\n", response.content);
                }

                // Parse the LLM's response
                let (diagnosis, repair) = parse_llm_response(&response.content);

                // Evaluate
                let diagnosis_correct = evaluate_diagnosis(&diagnosis, &scenario.correct_diagnosis);
                let repair_correct = evaluate_repair(&repair, &scenario.correct_repair);

                println!();
                println!("LLM Response:");
                if let Some(ref d) = diagnosis {
                    println!("  Diagnosis: {} (component: {})", d.root_cause, d.affected_component);
                } else {
                    println!("  Diagnosis: FAILED TO PARSE");
                    // Show first 200 chars of response to debug
                    let preview: String = response.content.chars().take(200).collect();
                    println!("  [Raw preview: {}...]", preview.replace('\n', " "));
                }
                if let Some(ref r) = repair {
                    println!("  Repair: {:?} -> {}", r.action_type, r.target);
                } else {
                    println!("  Repair: FAILED TO PARSE");
                }

                println!();
                println!("Expected:");
                println!("  Diagnosis: {} (component: {})",
                    scenario.correct_diagnosis.root_cause,
                    scenario.correct_diagnosis.affected_component);
                println!("  Repair: {:?} -> {}",
                    scenario.correct_repair.action_type,
                    scenario.correct_repair.target);

                println!();

                // Show mismatch details when failing
                if !diagnosis_correct || !repair_correct {
                    println!("Mismatch Details:");
                    if let Some(ref d) = diagnosis {
                        if d.affected_component.to_lowercase() != scenario.correct_diagnosis.affected_component.to_lowercase() {
                            println!("  Component: '{}' != expected '{}'",
                                d.affected_component, scenario.correct_diagnosis.affected_component);
                        }
                    }
                    if let Some(ref r) = repair {
                        if r.action_type != scenario.correct_repair.action_type {
                            println!("  Action: {:?} != expected {:?}",
                                r.action_type, scenario.correct_repair.action_type);
                        } else if r.target.to_lowercase() != scenario.correct_repair.target.to_lowercase() {
                            println!("  Target: '{}' != expected '{}'",
                                r.target, scenario.correct_repair.target);
                        }
                    }
                }

                println!("Result: Diagnosis {} | Repair {} | First-try {}",
                    if diagnosis_correct { "✓" } else { "✗" },
                    if repair_correct { "✓" } else { "✗" },
                    if diagnosis_correct && repair_correct { "✓ SUCCESS" } else { "✗ FAIL" });

                results.push((scenario.id.clone(), scenario.difficulty, diagnosis_correct, repair_correct));
            }
            Err(e) => {
                println!("✗ LLM Error: {}", e);
                results.push((scenario.id.clone(), scenario.difficulty, false, false));
            }
        }

        println!();
    }

    // Summary
    println!("═══════════════════════════════════════════════════════════════");
    println!("                         SUMMARY");
    println!("═══════════════════════════════════════════════════════════════");

    let total = results.len();
    let diagnosis_correct = results.iter().filter(|(_, _, d, _)| *d).count();
    let repair_correct = results.iter().filter(|(_, _, _, r)| *r).count();
    let first_try_success = results.iter().filter(|(_, _, d, r)| *d && *r).count();

    println!("Total scenarios: {}", total);
    println!("Diagnosis accuracy: {}/{} ({:.1}%)", diagnosis_correct, total, 100.0 * diagnosis_correct as f64 / total as f64);
    println!("Repair accuracy: {}/{} ({:.1}%)", repair_correct, total, 100.0 * repair_correct as f64 / total as f64);
    println!();
    println!(">>> FIRST-TRY SUCCESS RATE: {}/{} ({:.1}%) <<<",
        first_try_success, total, 100.0 * first_try_success as f64 / total as f64);

    // By difficulty
    println!();
    println!("By Difficulty:");
    for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard, Difficulty::Expert] {
        let diff_results: Vec<_> = results.iter().filter(|(_, d, _, _)| *d == difficulty).collect();
        if !diff_results.is_empty() {
            let success = diff_results.iter().filter(|(_, _, d, r)| *d && *r).count();
            println!("  {:?}: {}/{} ({:.1}%)", difficulty, success, diff_results.len(),
                100.0 * success as f64 / diff_results.len() as f64);
        }
    }

    println!();
    println!("Individual Results:");
    for (id, difficulty, diag, repair) in &results {
        let status = if *diag && *repair { "✓" } else { "✗" };
        println!("  {} {} ({:?})", status, id, difficulty);
    }
}

fn print_help() {
    println!("Self-Healing OS Test Runner");
    println!();
    println!("USAGE:");
    println!("  healing_test_runner [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  -p, --provider <NAME>   LLM provider:");
    println!("                          'lmstudio' (default) - Local LM Studio");
    println!("                          'ollama' - Local Ollama");
    println!("                          'claude' - Claude API (requires ANTHROPIC_API_KEY)");
    println!("                          'sonnet' - Claude Sonnet");
    println!("                          'haiku' - Claude Haiku (fast/cheap)");
    println!("  -m, --model <NAME>      Model name override");
    println!("  --debug                 Show prompts and raw responses");
    println!("  --easy                  Only run Easy scenarios");
    println!("  --medium                Only run Medium scenarios");
    println!("  --hard                  Only run Hard scenarios");
    println!("  --expert                Only run Expert scenarios");
    println!("  -h, --help              Show this help");
    println!();
    println!("ENVIRONMENT:");
    println!("  ANTHROPIC_API_KEY       API key for Claude (required for claude/sonnet/haiku)");
    println!();
    println!("EXAMPLES:");
    println!("  healing_test_runner                        # Use LM Studio");
    println!("  healing_test_runner -p ollama -m mistral   # Use Ollama");
    println!("  healing_test_runner -p sonnet              # Use Claude Sonnet");
    println!("  healing_test_runner -p haiku --easy        # Quick test with Haiku");
}

fn parse_llm_response(content: &str) -> (Option<Diagnosis>, Option<RepairAction>) {
    // Try to extract JSON from the response
    let json_str = extract_json(content);

    let diagnosis = parse_diagnosis(&json_str);
    let repair = parse_repair(&json_str);

    (diagnosis, repair)
}

fn extract_json(content: &str) -> String {
    // Find JSON block in response (may have markdown code fences)
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

fn parse_diagnosis(json_str: &str) -> Option<Diagnosis> {
    let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;

    // Try "diagnosis" or root-level keys
    let diag = parsed.get("diagnosis").or_else(|| Some(&parsed))?;

    // Try various field name alternatives
    let root_cause = diag.get("root_cause")
        .or_else(|| diag.get("rootCause"))
        .or_else(|| diag.get("cause"))
        .or_else(|| diag.get("issue"))
        .or_else(|| diag.get("problem"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;

    let affected_component = diag.get("affected_component")
        .or_else(|| diag.get("affectedComponent"))
        .or_else(|| diag.get("component"))
        .or_else(|| diag.get("service"))
        .or_else(|| diag.get("target"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;

    let severity = diag.get("severity")
        .and_then(|v| v.as_str())
        .map(|s| match s.to_lowercase().as_str() {
            "low" => Severity::Low,
            "medium" | "moderate" => Severity::Medium,
            "high" => Severity::High,
            "critical" | "severe" => Severity::Critical,
            _ => Severity::Medium,
        })
        .unwrap_or(Severity::Medium);

    Some(Diagnosis {
        root_cause,
        affected_component,
        severity,
        confidence: 1.0,
    })
}

fn parse_repair(json_str: &str) -> Option<RepairAction> {
    let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;

    // Try "repair" or "action" or root level
    let repair = parsed.get("repair")
        .or_else(|| parsed.get("action"))
        .or_else(|| parsed.get("fix"))
        .or_else(|| Some(&parsed))?;

    // Get action type - try various field names
    let action_str = repair.get("action")
        .or_else(|| repair.get("action_type"))
        .or_else(|| repair.get("actionType"))
        .or_else(|| repair.get("type"))
        .and_then(|v| v.as_str())?;

    let action_lower = action_str.to_lowercase().replace(" ", "").replace("_", "");
    let action_type = match action_lower.as_str() {
        "restartservice" | "restart" | "servicerestart" => ActionType::RestartService,
        "reloadconfig" | "reload" | "configreload" => ActionType::ReloadConfig,
        "clearcache" | "cacheclear" | "flushcache" => ActionType::ClearCache,
        "increaselimit" | "limitincrease" | "raiselimit" => ActionType::IncreaseLimit,
        "rotatelogs" | "logrotate" | "rotatelog" | "logrotation" => ActionType::RotateLogs,
        "killprocess" | "processkill" | "kill" | "terminate" => ActionType::KillProcess,
        "updateconfig" | "configupdate" | "modifyconfig" => ActionType::UpdateConfig,
        "noaction" | "none" | "donothing" | "noop" | "noactionneeded" => ActionType::NoAction,
        "escalate" | "escalation" | "notifyoperator" | "alertoperator" => ActionType::Escalate,
        _ => return None,
    };

    // Get target - try various field names
    let target = repair.get("target")
        .or_else(|| repair.get("service"))
        .or_else(|| repair.get("component"))
        .or_else(|| repair.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let parameters = repair.get("parameters")
        .or_else(|| repair.get("params"))
        .or_else(|| repair.get("options"))
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| {
                    let val = v.as_str()
                        .map(|s| s.to_string())
                        .or_else(|| Some(v.to_string()));
                    val.map(|s| (k.clone(), s))
                })
                .collect::<HashMap<String, String>>()
        })
        .unwrap_or_default();

    Some(RepairAction {
        action_type,
        target,
        parameters,
    })
}

fn evaluate_diagnosis(llm: &Option<Diagnosis>, expected: &Diagnosis) -> bool {
    let Some(llm) = llm else { return false };

    let llm_component = llm.affected_component.to_lowercase();
    let expected_component = expected.affected_component.to_lowercase();

    // Component matching - more flexible:
    // 1. Exact match
    // 2. Expected is contained in LLM response
    // 3. LLM response is contained in expected
    // 4. Key word from expected is in LLM response
    let component_matches = llm_component == expected_component
        || llm_component.contains(&expected_component)
        || expected_component.contains(&llm_component)
        || expected_component.split('-').any(|part| part.len() > 3 && llm_component.contains(part));

    if !component_matches {
        return false;
    }

    // Root cause should capture the right concept (semantic matching)
    let llm_lower = llm.root_cause.to_lowercase();
    let expected_lower = expected.root_cause.to_lowercase();

    // Define semantic concept groups - if LLM mentions any term in the same group as expected, it's a match
    let concept_groups: &[&[&str]] = &[
        &["leak", "leaking", "exhausted", "exhausting", "exhaustion", "limit", "oom", "out of memory"],
        &["deadlock", "lock", "blocked", "contention", "spin", "waiting"],
        &["config", "configuration", "setting", "misconfigur"],
        &["disk", "space", "storage", "full", "capacity"],
        &["cache", "redis", "caching", "hit rate"],
        &["certificate", "cert", "tls", "ssl", "expir", "handshake"],
        &["dns", "network", "connectivity", "unreachable", "resolution"],
        &["batch", "scheduled", "cron", "normal", "within limits"],
        &["descriptor", "fd", "file handle", "too many open", "socket"],
        &["connection", "pool", "database", "db"],
    ];

    // Check if both expected and LLM response share concepts from the same group
    for group in concept_groups {
        let expected_has = group.iter().any(|term| expected_lower.contains(term));
        let llm_has = group.iter().any(|term| llm_lower.contains(term));
        if expected_has && llm_has {
            return true;
        }
    }

    // Fallback: check for direct word overlap (at least 2 significant words)
    let expected_words: std::collections::HashSet<&str> = expected_lower
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .collect();

    let llm_words: std::collections::HashSet<&str> = llm_lower
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .collect();

    let overlap = expected_words.intersection(&llm_words).count();
    overlap >= 2
}

fn evaluate_repair(llm: &Option<RepairAction>, expected: &RepairAction) -> bool {
    let Some(llm) = llm else { return false };

    // Action type must match
    if llm.action_type != expected.action_type {
        return false;
    }

    // For NoAction or Escalate, target matching is flexible
    if expected.action_type == ActionType::NoAction {
        return true;
    }

    if expected.action_type == ActionType::Escalate {
        // For escalate, any reasonable escalation target is acceptable
        // The key is that the LLM knew to escalate, not the exact team name
        return true;
    }

    // Target matching - flexible:
    let llm_target = llm.target.to_lowercase();
    let expected_target = expected.target.to_lowercase();

    // Direct matches
    if llm_target == expected_target
        || llm_target.contains(&expected_target)
        || expected_target.contains(&llm_target)
        || expected_target.split('-').any(|part| part.len() > 3 && llm_target.contains(part))
    {
        return true;
    }

    // Common service aliases (LLM might use actual hostname from config)
    let aliases: &[(&str, &[&str])] = &[
        ("cache", &["redis", "redis-primary", "redis-cluster", "memcached"]),
        ("database", &["postgres", "mysql", "db", "rds"]),
        ("storage", &["disk", "volume", "nfs", "s3"]),
    ];

    for (canonical, alt_names) in aliases {
        if expected_target == *canonical {
            if alt_names.iter().any(|alt| llm_target.contains(alt)) {
                return true;
            }
        }
    }

    false
}
