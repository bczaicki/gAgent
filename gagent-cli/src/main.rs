use anyhow::Result;
use clap::{Parser, Subcommand};
use gagent_core::{BootstrapFiles, Config, PromptAssembler, MAX_TOTAL_CHARS};
use gagent_harness::{AgentHarness, Session};
use gagent_harness::CrashRecovery;
use gagent_llm::{OllamaProvider, OpenAiProvider, RetryConfig, RetryProvider};
use gagent_mcp::{McpBridge, parse_mcp_servers};
use gagent_tools::{ToolRegistry, builtin::{FileReadTool, FileWriteTool, FileSearchTool, ShellTool, GitTool, MemoryReadTool, MemoryWriteTool, MemorySearchTool}};
use std::io::{self, Write};

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

    /// RALPH loop commands (plan + build)
    Ralph {
        #[command(subcommand)]
        action: RalphAction,
    },

    /// Show or modify configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum RalphAction {
    /// Generate an implementation plan from a spec
    Plan {
        /// Path to the spec/PRD file
        spec: String,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Execute the building phase from an existing plan
    Build {
        /// Maximum iterations
        #[arg(long, default_value = "10")]
        max_iterations: usize,
    },
    /// Run both plan and build phases
    Run {
        /// Path to the spec/PRD file
        spec: String,

        /// Maximum build iterations
        #[arg(long, default_value = "10")]
        max_iterations: usize,

        /// Backpressure command (run after each iteration)
        #[arg(long)]
        backpressure: Option<String>,
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
        Commands::Ralph { action } => {
            match action {
                RalphAction::Plan { spec, model: _ } => {
                    eprintln!("🌱 RALPH planning not yet implemented. Spec: {spec}");
                }
                RalphAction::Build { max_iterations } => {
                    eprintln!("🌱 RALPH building not yet implemented. Max iterations: {max_iterations}");
                }
                RalphAction::Run {
                    spec,
                    max_iterations,
                    backpressure,
                } => {
                    eprintln!(
                        "🌱 RALPH run not yet implemented. Spec: {spec}, max: {max_iterations}, backpressure: {backpressure:?}"
                    );
                }
            }
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

    // Write MCP config template
    let mcp_config_path = base.join("mcp.json");
    let mcp_template = serde_json::json!({
        "_comment": "Add MCP servers here. Example:",
        "_example": {
            "filesystem": {
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
            }
        }
    });
    std::fs::write(&mcp_config_path, serde_json::to_string_pretty(&mcp_template)?)?;
    eprintln!("  Created {}", mcp_config_path.display());

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

    // Choose provider based on config
    let retry_config = RetryConfig::default();
    let provider: Box<dyn gagent_llm::LlmProvider> = match config.llm.provider.as_str() {
        "openai" | "openai-compatible" => {
            let api_key = std::env::var("OPENAI_API_KEY").ok();
            Box::new(RetryProvider::new(
                OpenAiProvider::new(&base_url, &model, api_key),
                retry_config,
            ))
        }
        _ => Box::new(RetryProvider::new(
            OllamaProvider::new(&base_url, &model),
            retry_config,
        )),
    };

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
    registry.register(Box::new(MemoryReadTool::new()));
    registry.register(Box::new(MemoryWriteTool::new()));
    registry.register(Box::new(MemorySearchTool::new()));

    // Load MCP servers from .gagent/mcp.json if it exists
    let mcp_config_path = workspace_dir.join("mcp.json");
    if mcp_config_path.exists() {
        match std::fs::read_to_string(&mcp_config_path)
            .and_then(|s| Ok(serde_json::from_str::<serde_json::Value>(&s).unwrap_or_default()))
        {
            Ok(mcp_config) => {
                let servers = parse_mcp_servers(&mcp_config);
                if !servers.is_empty() {
                    let mut bridge = McpBridge::new();
                    for server in servers {
                        bridge.add_server(server);
                    }
                    if let Err(e) = bridge.register_all(&mut registry).await {
                        eprintln!("⚠ MCP bridge error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠ Failed to read MCP config: {}", e);
            }
        }
    }

    tracing::info!("Registered {} tools", registry.len());

    // Set up crash recovery
    let crash_recovery = CrashRecovery::new(workspace_dir.join("crash_recovery.json"));

    // Create or load session (resume from crash if checkpoint exists)
    let mut session = if crash_recovery.has_checkpoint() {
        eprintln!("⚠ Previous crash detected. Resuming from checkpoint...");
        match crash_recovery.read_checkpoint() {
            Ok(json) => Session::from_json(&json).unwrap_or_else(|_| Session::new()),
            Err(_) => Session::new(),
        }
    } else {
        Session::new()
    };

    eprintln!("🌱 gAgent — connected to {} (model: {})", base_url, model);
    eprintln!("   {} tools available (built-in + MCP)", registry.len());
    eprintln!("   Type your message and press Enter. Type 'quit' or 'exit' to stop.\n");

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
            // Clear crash recovery checkpoint on clean exit
            let _ = crash_recovery.clear_checkpoint();
            eprintln!("🌱 Goodbye!");
            break;
        }

        // Run agent loop (this handles tool calls internally)
        match harness.run(input, &mut session, provider.as_ref(), &registry).await {
            Ok(response) => {
                println!("\n🌱 {}\n", response);
                // Write crash recovery checkpoint after each successful exchange
                if let Ok(json) = session.to_json() {
                    let _ = crash_recovery.write_checkpoint(&json);
                }
            }
            Err(e) => {
                eprintln!("\n❌ Error: {}\n", e);
            }
        }
    }

    Ok(())
}

