use anyhow::Result;
use clap::{Parser, Subcommand};
use gagent_core::{BootstrapFiles, Config, PromptAssembler, MAX_TOTAL_CHARS};
use gagent_harness::{AgentHarness, Session};
use gagent_llm::OllamaProvider;
use gagent_ralph::{RalphConfig, RalphLoop};
use gagent_tools::{ToolRegistry, builtin::*};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gagent", about = "🌱 gAgent — your local, private AI agent")]
#[command(version)]
struct Cli {
    /// Verbose output (enables tracing)
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive chat session
    Run {
        /// Model to use (overrides config)
        #[arg(short, long)]
        model: Option<String>,

        /// Ollama base URL (overrides config)
        #[arg(long)]
        url: Option<String>,

        /// Disable streaming (get complete responses)
        #[arg(long)]
        no_stream: bool,
    },

    /// Initialize a .gagent workspace in the current directory
    Init,

    /// Show or modify configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show the current configuration
    Show,
    /// Initialize a default config file
    Init,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up tracing
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("gagent=debug,info")
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter("gagent=info,warn")
            .with_target(false)
            .without_time()
            .init();
    }

    match cli.command {
        Commands::Run {
            model,
            url,
            no_stream,
        } => {
            run_interactive(model, url, no_stream).await?;
        }
        Commands::Init => {
            init_workspace()?;
        }
        Commands::Config { action } => match action {
            ConfigAction::Show => {
                let config = Config::default();
                println!("{}", toml::to_string_pretty(&config)?);
            }
            ConfigAction::Init => {
                let config_path = std::path::Path::new(".gagent/config.toml");
                if config_path.exists() {
                    eprintln!("Config already exists at {}", config_path.display());
                } else {
                    let config = Config::default();
                    config.save(config_path)?;
                    eprintln!("🌱 Config written to {}", config_path.display());
                }
            }
        },
    }

    Ok(())
}

fn init_workspace() -> Result<()> {
    let base = std::path::Path::new(".gagent");

    if base.exists() {
        eprintln!("🌱 Workspace already exists at {}", base.display());
        return Ok(());
    }

    std::fs::create_dir_all(base.join("memory"))?;
    std::fs::create_dir_all(base.join("sessions"))?;

    // Create template bootstrap files with better guidance
    let templates = [
        (
            "IDENTITY.md",
            "# Identity\n\nname: gAgent\nemoji: 🌱\n\n<!-- Format: \"name: YourName\" and \"emoji: 🚀\" on separate lines.\n     Character limit: 20,000 per file -->\n",
        ),
        (
            "SOUL.md",
            "# Soul\n\nYou are helpful, thoughtful, and precise. You communicate clearly and concisely.\n\n\
            When working on code:\n\
            - Prefer idiomatic patterns for the language\n\
            - Write tests for new functionality\n\
            - Follow existing conventions in the codebase\n\n\
            When interacting with users:\n\
            - Ask clarifying questions when requirements are unclear\n\
            - Provide context for your decisions\n\
            - Focus on practical solutions\n\n\
            <!-- Character limit: 20,000 per file -->\n",
        ),
        (
            "USER.md",
            "# User Profile\n\n<!-- Tell the agent about yourself:\n\
            - Your role and expertise\n\
            - Preferred languages/frameworks\n\
            - Coding style preferences\n\
            - Project context\n\n\
            Character limit: 20,000 per file -->\n",
        ),
        (
            "AGENTS.md",
            "# Agents\n\n<!-- Multi-agent context goes here.\n     Character limit: 20,000 per file -->\n",
        ),
        (
            "TOOLS.md",
            "# Tools\n\n<!-- Tool usage guidance goes here.\n     Character limit: 20,000 per file -->\n",
        ),
        (
            "MEMORY.md",
            "# Memory\n\n<!-- Long-term agent memory. Auto-maintained.\n     Character limit: 20,000 per file -->\n",
        ),
    ];

    for (filename, content) in templates {
        let path = base.join(filename);
        std::fs::write(&path, content)?;
        eprintln!("  Created {}", path.display());
    }

    // Write default config
    let config = Config::default();
    let config_path = base.join("config.toml");
    config.save(&config_path)?;
    eprintln!("  Created {}", config_path.display());

    eprintln!("\n🌱 Workspace initialized at {}/", base.display());
    eprintln!("   Edit the files above to customize your agent.");

    // Validate bootstrap files
    match BootstrapFiles::load(base) {
        Ok(bootstrap) => {
            eprintln!("\n✓ Bootstrap files validated");
            eprintln!("  Identity: {}", bootstrap.identity.display_prefix());
            eprintln!("  Total: {} / {} chars", bootstrap.total_chars, MAX_TOTAL_CHARS);
        }
        Err(e) => {
            eprintln!("\n⚠ Warning: {}", e);
        }
    }

    Ok(())
}

async fn run_interactive(
    model_override: Option<String>,
    url_override: Option<String>,
    _no_stream: bool, // TODO: implement streaming with harness.run_stream()
) -> Result<()> {
    // Load config
    let config_path = std::path::Path::new(".gagent/config.toml");
    let config = Config::load(config_path).unwrap_or_default();

    let base_url = url_override.unwrap_or_else(|| config.llm.base_url.clone());
    let model = model_override.unwrap_or_else(|| config.llm.model.clone());

    let provider = OllamaProvider::new(&base_url, &model);

    // Load bootstrap files
    let workspace_dir = std::path::Path::new(".gagent");
    let bootstrap = BootstrapFiles::load(workspace_dir).unwrap_or_default();

    // Assemble system prompt
    let assembler = PromptAssembler::new(config.clone(), bootstrap.clone());
    let system_prompt = assembler.assemble();

    tracing::info!(
        "System prompt assembled: {} chars",
        system_prompt.char_count
    );

    // Create agent harness
    let harness = AgentHarness::new(config.clone(), system_prompt);

    // Register built-in tools
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FileReadTool::new()));
    registry.register(Box::new(FileWriteTool::new()));
    registry.register(Box::new(FileSearchTool::new()));
    registry.register(Box::new(ShellTool::new()));
    registry.register(Box::new(GitTool::new()));

    tracing::info!("Registered {} tools", registry.len());

    // Create or load session
    let mut session = Session::new();

    eprintln!("🌱 gAgent — connected to {} (model: {})", base_url, model);
    eprintln!("   {} tools available: file_read, file_write, file_search, shell, git", registry.len());
    eprintln!("   Type your message and press Enter. Type 'quit' or 'exit' to stop.");
    eprintln!();
    eprintln!("   Slash commands:");
    eprintln!("     /plan <spec>              — generate an implementation plan");
    eprintln!("     /build [--max-iter N]     — execute the building phase");
    eprintln!("     /run <spec> [--max-iter N] — run full plan + build cycle");
    eprintln!("     /help                     — show this help");
    eprintln!();

    loop {
        // Print prompt
        eprint!("You: ");
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input == "quit" || input == "exit" {
            // Save session
            let session_path = Session::default_path(&config.session.sessions_dir, &session.id);
            if let Some(parent) = session_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Err(e) = session.save(&session_path).await {
                eprintln!("⚠ Failed to save session: {}", e);
            } else {
                eprintln!("💾 Session saved: {}", session.id);
            }
            eprintln!("🌱 Goodbye!");
            break;
        }

        // Dispatch slash commands
        if input.starts_with('/') {
            handle_slash_command(input, &config).await?;
            continue;
        }

        // Run agent loop (this handles tool calls internally)
        match harness.run(input, &mut session, &provider, &registry).await {
            Ok(response) => {
                println!("\n🌱 {}\n", response);
            }
            Err(e) => {
                eprintln!("\n❌ Error: {}\n", e);
            }
        }
    }

    Ok(())
}

async fn handle_slash_command(input: &str, config: &Config) -> Result<()> {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0];
    let args = parts.get(1).copied().unwrap_or("").trim();

    match cmd {
        "/help" => {
            eprintln!();
            eprintln!("  ╔══════════════════════════════════════════════════╗");
            eprintln!("  ║              Slash Commands                      ║");
            eprintln!("  ╠══════════════════════════════════════════════════╣");
            eprintln!("  ║  /plan <spec>               Generate a plan      ║");
            eprintln!("  ║  /build [--max-iter N]      Execute build phase  ║");
            eprintln!("  ║  /run <spec> [--max-iter N] Plan + build cycle   ║");
            eprintln!("  ║  /help                      Show this help       ║");
            eprintln!("  ╚══════════════════════════════════════════════════╝");
            eprintln!();
        }

        "/plan" => {
            if args.is_empty() {
                eprintln!("  ✗ /plan requires a spec file path");
                eprintln!("    Usage: /plan <spec>");
                return Ok(());
            }
            eprintln!();
            eprintln!("  ┌─ /plan ─────────────────────────────────────────");
            eprintln!("  │  Spec: {}", args);
            eprintln!("  └─────────────────────────────────────────────────");
            eprintln!();
            run_ralph_plan(args, None, config).await?;
        }

        "/build" => {
            let max_iterations = parse_max_iter(args).unwrap_or(10);
            eprintln!();
            eprintln!("  ┌─ /build ────────────────────────────────────────");
            eprintln!("  │  Max iterations: {}", max_iterations);
            eprintln!("  └─────────────────────────────────────────────────");
            eprintln!();
            run_ralph_build(max_iterations, config).await?;
        }

        "/run" => {
            let (spec, max_iterations) = parse_run_args(args);
            if spec.is_empty() {
                eprintln!("  ✗ /run requires a spec file path");
                eprintln!("    Usage: /run <spec> [--max-iter N]");
                return Ok(());
            }
            eprintln!();
            eprintln!("  ┌─ /run ──────────────────────────────────────────");
            eprintln!("  │  Spec: {}", spec);
            eprintln!("  │  Max iterations: {}", max_iterations);
            eprintln!("  └─────────────────────────────────────────────────");
            eprintln!();
            run_ralph_full(spec, max_iterations, false, config).await?;
        }

        _ => {
            eprintln!("  ✗ Unknown command: {}", cmd);
            eprintln!("    Type /help to see available commands.");
        }
    }

    Ok(())
}

/// Parse `--max-iter N` from a string, returning None if not found.
fn parse_max_iter(args: &str) -> Option<usize> {
    let mut parts = args.split_whitespace();
    while let Some(part) = parts.next() {
        if part == "--max-iter" {
            if let Some(n) = parts.next() {
                return n.parse().ok();
            }
        }
    }
    None
}

/// Parse `/run <spec> [--max-iter N]` arguments.
fn parse_run_args(args: &str) -> (&str, usize) {
    let max_iter = parse_max_iter(args).unwrap_or(10);
    // spec is the first token (before any --flags)
    let spec = args
        .split_whitespace()
        .find(|s| !s.starts_with('-'))
        .unwrap_or("");
    (spec, max_iter)
}

async fn run_ralph_plan(spec_path: &str, model_override: Option<String>, config: &Config) -> Result<()> {
    let model = model_override.unwrap_or_else(|| config.llm.model.clone());
    let provider = OllamaProvider::new(&config.llm.base_url, &model);

    let workspace_dir = std::path::Path::new(".gagent");
    let bootstrap = BootstrapFiles::load(workspace_dir).unwrap_or_default();
    let assembler = PromptAssembler::new(config.clone(), bootstrap);
    let system_prompt = assembler.assemble();

    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FileReadTool::new()));
    registry.register(Box::new(FileWriteTool::new()));
    registry.register(Box::new(FileSearchTool::new()));
    registry.register(Box::new(ShellTool::new()));
    registry.register(Box::new(GitTool::new()));

    let mut ralph_config = RalphConfig::default();
    ralph_config.spec_path = Some(PathBuf::from(spec_path));
    std::fs::create_dir_all(&ralph_config.ralph_dir)?;

    let ralph_loop = RalphLoop::new(config.clone(), ralph_config);
    ralph_loop
        .run_planning(&provider, &registry, system_prompt)
        .await?;

    eprintln!("  ✓ Planning complete — IMPLEMENTATION_PLAN.md written");
    eprintln!("    Run /build to start building.");
    eprintln!();

    Ok(())
}

async fn run_ralph_build(max_iterations: usize, config: &Config) -> Result<()> {
    let provider = OllamaProvider::new(&config.llm.base_url, &config.llm.model);

    let workspace_dir = std::path::Path::new(".gagent");
    let bootstrap = BootstrapFiles::load(workspace_dir).unwrap_or_default();
    let assembler = PromptAssembler::new(config.clone(), bootstrap);
    let system_prompt = assembler.assemble();

    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FileReadTool::new()));
    registry.register(Box::new(FileWriteTool::new()));
    registry.register(Box::new(FileSearchTool::new()));
    registry.register(Box::new(ShellTool::new()));
    registry.register(Box::new(GitTool::new()));

    let mut ralph_config = RalphConfig::default();
    ralph_config.max_iterations = max_iterations;
    std::fs::create_dir_all(&ralph_config.ralph_dir)?;

    let ralph_loop = RalphLoop::new(config.clone(), ralph_config);
    ralph_loop
        .run_building(&provider, &registry, system_prompt)
        .await?;

    eprintln!("  ✓ Building phase complete!");
    eprintln!();

    Ok(())
}

async fn run_ralph_full(
    spec_path: &str,
    max_iterations: usize,
    backpressure: bool,
    config: &Config,
) -> Result<()> {
    let provider = OllamaProvider::new(&config.llm.base_url, &config.llm.model);

    let workspace_dir = std::path::Path::new(".gagent");
    let bootstrap = BootstrapFiles::load(workspace_dir).unwrap_or_default();
    let assembler = PromptAssembler::new(config.clone(), bootstrap);
    let system_prompt = assembler.assemble();

    let mut registry = ToolRegistry::new();
    registry.register(Box::new(FileReadTool::new()));
    registry.register(Box::new(FileWriteTool::new()));
    registry.register(Box::new(FileSearchTool::new()));
    registry.register(Box::new(ShellTool::new()));
    registry.register(Box::new(GitTool::new()));

    let mut ralph_config = RalphConfig::default();
    ralph_config.spec_path = Some(PathBuf::from(spec_path));
    ralph_config.max_iterations = max_iterations;
    ralph_config.backpressure = backpressure;
    std::fs::create_dir_all(&ralph_config.ralph_dir)?;

    let ralph_loop = RalphLoop::new(config.clone(), ralph_config);
    ralph_loop
        .run_full_cycle(&provider, &registry, system_prompt)
        .await?;

    eprintln!("  ✓ RALPH cycle complete!");
    eprintln!();

    Ok(())
}
