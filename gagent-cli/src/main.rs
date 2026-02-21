use anyhow::Result;
use clap::{Parser, Subcommand};
use gagent_core::Config;
use gagent_llm::{ChatMessage, ChatRequest, LlmProvider, OllamaProvider, StreamChunk};
use futures::StreamExt;
use std::io::{self, Write};
use tracing::info;

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

    // Create template bootstrap files
    let templates = [
        ("SOUL.md", "# Soul\n\nDescribe the agent's personality and tone here.\n"),
        ("IDENTITY.md", "# Identity\n\nname: gAgent\nemoji: 🌱\n"),
        ("USER.md", "# User Profile\n\nDescribe yourself and your preferences here.\n"),
        ("AGENTS.md", "# Agents\n\nMulti-agent context goes here.\n"),
        ("TOOLS.md", "# Tools\n\nTool usage guidance goes here.\n"),
        ("MEMORY.md", "# Memory\n\nLong-term agent memory. Auto-maintained.\n"),
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

    Ok(())
}

async fn run_interactive(
    model_override: Option<String>,
    url_override: Option<String>,
    no_stream: bool,
) -> Result<()> {
    // Load config
    let config_path = std::path::Path::new(".gagent/config.toml");
    let config = Config::load(config_path).unwrap_or_default();

    let base_url = url_override
        .unwrap_or_else(|| config.llm.base_url.clone());
    let model = model_override
        .unwrap_or_else(|| config.llm.model.clone());

    let provider = OllamaProvider::new(&base_url, &model);

    eprintln!("🌱 gAgent — connected to {} (model: {})", base_url, model);
    eprintln!("   Type your message and press Enter. Type 'quit' or 'exit' to stop.\n");

    let mut history: Vec<ChatMessage> = vec![ChatMessage::system(
        "You are gAgent, a helpful local AI assistant. You are running locally on the user's machine via Ollama. Be concise and helpful.",
    )];

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
            eprintln!("🌱 Goodbye!");
            break;
        }

        history.push(ChatMessage::user(input));

        let request = ChatRequest {
            messages: history.clone(),
            tools: vec![],
            temperature: Some(config.llm.temperature),
            stream: !no_stream,
        };

        if no_stream {
            // Non-streaming mode
            match provider.chat(request).await {
                Ok(response) => {
                    println!("\n🌱 {}\n", response.message.content);
                    history.push(response.message);
                }
                Err(e) => {
                    eprintln!("\n❌ Error: {e}\n");
                }
            }
        } else {
            // Streaming mode
            match provider.chat_stream(request).await {
                Ok(mut stream) => {
                    eprint!("\n🌱 ");
                    let mut full_response = String::new();

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(StreamChunk::Text(text)) => {
                                eprint!("{text}");
                                io::stderr().flush()?;
                                full_response.push_str(&text);
                            }
                            Ok(StreamChunk::Done { total_tokens }) => {
                                if let Some(tokens) = total_tokens {
                                    info!("Total tokens: {tokens}");
                                }
                                break;
                            }
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("\n❌ Stream error: {e}");
                                break;
                            }
                        }
                    }

                    eprintln!("\n");
                    if !full_response.is_empty() {
                        history.push(ChatMessage::assistant(full_response));
                    }
                }
                Err(e) => {
                    eprintln!("\n❌ Error: {e}\n");
                }
            }
        }
    }

    Ok(())
}
