use std::collections::HashMap;
use std::io::{self, Write};

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

    fn execute(&self, task: &str) -> String {
        format!("{} executes task: {}", self.name, task)
    }

    fn reflect(&self, outcome: &str) -> String {
        format!("{} reflects on outcome: {}", self.name, outcome)
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

    fn run_once(&self, agent_name: &str, goal: &str) -> Result<Vec<String>, String> {
        let agent = self
            .agents
            .get(agent_name)
            .ok_or_else(|| format!("No agent named '{agent_name}'"))?;

        let plan = agent.plan(goal);
        let execution = agent.execute(goal);
        let reflection = agent.reflect(&execution);

        Ok(vec![plan, execution, reflection])
    }
}

fn print_help() {
    println!("Commands:");
    println!("  help                        Show this help menu");
    println!("  list                        List registered local agents");
    println!("  run <agent> <goal>          Run one Ralph loop iteration");
    println!("  exit                        Quit");
}

fn main() {
    let mut loop_engine = RalphLoop::new();
    loop_engine.register_agent(LocalAgent::new("scout", "information-gatherer"));
    loop_engine.register_agent(LocalAgent::new("builder", "task-executor"));

    println!("gAgent Ralph Loop (local agents)");
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

        match loop_engine.run_once(agent_name, goal) {
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
    use super::{LocalAgent, RalphLoop};

    #[test]
    fn ralph_loop_runs_for_registered_agent() {
        let mut loop_engine = RalphLoop::new();
        loop_engine.register_agent(LocalAgent::new("tester", "qa"));

        let output = loop_engine
            .run_once("tester", "validate command parsing")
            .expect("run should succeed");

        assert_eq!(output.len(), 3);
        assert!(output[0].contains("plans"));
        assert!(output[1].contains("executes"));
        assert!(output[2].contains("reflects"));
    }

    #[test]
    fn ralph_loop_errors_for_unknown_agent() {
        let loop_engine = RalphLoop::new();
        let error = loop_engine
            .run_once("ghost", "do work")
            .expect_err("unknown agent should fail");

        assert!(error.contains("No agent named"));
    }
}
