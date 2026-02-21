use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
struct LocalAgent {
    name: String,
    role: String,
}

impl LocalAgent {
    fn new(name: &str, role: &str) -> Self {
        Self {
            name: name.to_string(),
            role: role.to_string(),
        }
    }

    fn plan(&self, goal: &str) -> String {
        format!("{} ({}) plans to handle: {}", self.name, self.role, goal)
    }

    fn reflect(&self, outcome: &str) -> String {
        format!("{} reflects on outcome: {}", self.name, outcome)
    }

    fn build_prompt(&self, goal: &str) -> String {
        format!(
            "You are a local agent named '{name}' with role '{role}'. Complete this task: {goal}. Keep output concise and actionable.",
            name = self.name,
            role = self.role,
            goal = goal
        )
    }
}

#[derive(Debug, Clone)]
struct OllamaConfig {
    base_url: String,
    model: String,
}

impl OllamaConfig {
    fn from_env() -> Self {
        Self {
            base_url: env::var("OLLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string()),
            model: env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string()),
        }
    }
}

#[derive(Debug, Default)]
struct RalphLoop {
    agents: HashMap<String, LocalAgent>,
}

impl RalphLoop {
    fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    fn register_agent(&mut self, agent: LocalAgent) {
        self.agents.insert(agent.name.clone(), agent);
    }

    fn run_once(
        &self,
        agent_name: &str,
        goal: &str,
        ollama: &OllamaConfig,
    ) -> Result<Vec<String>, String> {
        let agent = self
            .agents
            .get(agent_name)
            .ok_or_else(|| format!("No agent named '{agent_name}'"))?;

        let plan = agent.plan(goal);
        let execution = self.execute_with_ollama_stream(agent, goal, ollama)?;
        let reflection = agent.reflect(&execution);

        Ok(vec![plan, execution, reflection])
    }

    fn execute_with_ollama_stream(
        &self,
        agent: &LocalAgent,
        goal: &str,
        ollama: &OllamaConfig,
    ) -> Result<String, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|error| format!("Failed to build HTTP client: {error}"))?;

        let payload = json!({
            "model": ollama.model,
            "prompt": agent.build_prompt(goal),
            "stream": true
        });

        let endpoint = format!("{}/api/generate", ollama.base_url.trim_end_matches('/'));
        let response = client
            .post(endpoint)
            .json(&payload)
            .send()
            .map_err(|error| format!("Failed to call Ollama: {error}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .unwrap_or_else(|_| "(unable to read response body)".to_string());
            return Err(format!(
                "Ollama request failed with status {status}: {body}"
            ));
        }

        println!(
            "Streaming execution from Ollama model '{}'...",
            ollama.model
        );
        print!("  ");
        let _ = io::stdout().flush();

        let mut reader = BufReader::new(response);
        let mut output = String::new();

        loop {
            let mut line = String::new();
            let bytes_read = reader
                .read_line(&mut line)
                .map_err(|error| format!("Failed to read Ollama stream: {error}"))?;

            if bytes_read == 0 {
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let (chunk, done) = parse_ollama_chunk(trimmed)?;
            if !chunk.is_empty() {
                output.push_str(&chunk);
                print!("{chunk}");
                let _ = io::stdout().flush();
            }

            if done {
                break;
            }
        }

        println!();

        if output.trim().is_empty() {
            return Err("Ollama returned an empty response".to_string());
        }

        Ok(output)
    }
}

fn parse_ollama_chunk(line: &str) -> Result<(String, bool), String> {
    let parsed: Value = serde_json::from_str(line)
        .map_err(|error| format!("Invalid streaming JSON from Ollama: {error}; line: {line}"))?;

    let chunk = parsed
        .get("response")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let done = parsed.get("done").and_then(Value::as_bool).unwrap_or(false);

    Ok((chunk, done))
}

fn print_help() {
    println!("Commands:");
    println!("  help                        Show this help menu");
    println!("  list                        List registered local agents");
    println!("  run <agent> <goal>          Run one Ralph loop iteration via Ollama");
    println!("  exit                        Quit");
    println!();
    println!("Environment variables:");
    println!("  OLLAMA_URL                  Default: http://127.0.0.1:11434");
    println!("  OLLAMA_MODEL                Default: llama3.2");
}

fn main() {
    let mut loop_engine = RalphLoop::new();
    loop_engine.register_agent(LocalAgent::new("scout", "information-gatherer"));
    loop_engine.register_agent(LocalAgent::new("builder", "task-executor"));
    let ollama = OllamaConfig::from_env();

    println!("gAgent Ralph Loop (local agents + Ollama)");
    print_help();

    let stdin = io::stdin();

    loop {
        print!("> ");
        if let Err(error) = io::stdout().flush() {
            eprintln!("Failed to flush stdout: {error}");
            continue;
        }

        let mut input = String::new();
        match stdin.read_line(&mut input) {
            Ok(0) => {
                println!("Goodbye.");
                break;
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("Failed to read input: {error}");
                continue;
            }
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input == "help" {
            print_help();
            continue;
        }

        if input == "list" {
            if loop_engine.agents.is_empty() {
                println!("No local agents registered.");
            } else {
                println!("Registered agents:");
                for agent in loop_engine.agents.values() {
                    println!("  - {} ({})", agent.name, agent.role);
                }
            }
            continue;
        }

        if input == "exit" {
            println!("Goodbye.");
            break;
        }

        let mut parts = input.splitn(3, ' ');
        let command = parts.next().unwrap_or_default();

        if command != "run" {
            eprintln!("Unknown command. Type 'help' for usage.");
            continue;
        }

        let Some(agent_name) = parts.next() else {
            eprintln!("Usage: run <agent> <goal>");
            continue;
        };

        let Some(goal) = parts.next() else {
            eprintln!("Usage: run <agent> <goal>");
            continue;
        };

        match loop_engine.run_once(agent_name, goal, &ollama) {
            Ok(steps) => {
                println!("Ralph loop output:");
                for (index, step) in steps.iter().enumerate() {
                    println!("  {}. {}", index + 1, step);
                }
            }
            Err(error) => eprintln!("Error: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_ollama_chunk, LocalAgent, OllamaConfig, RalphLoop};

    #[test]
    fn parses_ollama_stream_chunk() {
        let line = r#"{"model":"llama3.2","response":"hello","done":false}"#;
        let (chunk, done) = parse_ollama_chunk(line).expect("chunk should parse");

        assert_eq!(chunk, "hello");
        assert!(!done);
    }

    #[test]
    fn parses_ollama_stream_done_chunk() {
        let line = r#"{"model":"llama3.2","response":"","done":true}"#;
        let (chunk, done) = parse_ollama_chunk(line).expect("chunk should parse");

        assert_eq!(chunk, "");
        assert!(done);
    }

    #[test]
    fn ralph_loop_errors_for_unknown_agent() {
        let loop_engine = RalphLoop::new();
        let ollama = OllamaConfig::from_env();
        let error = loop_engine
            .run_once("ghost", "do work", &ollama)
            .expect_err("unknown agent should fail");

        assert!(error.contains("No agent named"));
    }

    #[test]
    fn build_prompt_contains_agent_context() {
        let agent = LocalAgent::new("tester", "qa");
        let prompt = agent.build_prompt("validate command parsing");

        assert!(prompt.contains("tester"));
        assert!(prompt.contains("qa"));
        assert!(prompt.contains("validate command parsing"));
    }
}
