# gAgent

Your green, local, personal AI agent.

gAgent is a **local-first, privacy-respecting AI agent** that brings autonomous coding capabilities to your machine using local LLMs. Inspired by [OpenClaw](https://openclaw.ai/) and the [Ralph Loop](https://github.com/Endogen/ralph-loop) pattern, gAgent can plan, build, and iterate on software projects — all without sending your data to the cloud.

---

## Why gAgent?

- **Local-first** — runs entirely on your machine via [Ollama](https://ollama.ai/) or any OpenAI-compatible server
- **Privacy by default** — your code, conversations, and data never leave your disk
- **Autonomous building** — the RALPH loop can take a spec and iteratively implement it with git commits
- **Memory across sessions** — agents remember learnings and context between conversations
- **Extensible** — MCP tool bridge connects to any Model Context Protocol server
- **Secure** — path containment, Docker sandboxing, and execution policies

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2021)
- [Ollama](https://ollama.ai/) running locally with a model pulled (e.g., `ollama pull llama3.2`)

### Install

```bash
git clone https://github.com/your-username/gAgent.git
cd gAgent
cargo build --workspace
```

### Initialize a workspace

```bash
cargo run -p gagent-cli -- init
```

This creates a `.gagent/` directory with configuration and bootstrap files:

```
.gagent/
  config.toml     # LLM provider, model, and agent settings
  SOUL.md         # Agent personality and tone
  IDENTITY.md     # Agent name and emoji
  USER.md         # Your preferences
  AGENTS.md       # Multi-agent context
  TOOLS.md        # Tool usage guidance
  MEMORY.md       # Long-term agent memory
  memory/         # Daily memory entries
  sessions/       # Conversation history
```

### Start chatting

```bash
cargo run -p gagent-cli -- run
```

Override the model or Ollama URL:

```bash
cargo run -p gagent-cli -- run --model mistral --url http://localhost:11434
```

---

## Usage

```
gagent [OPTIONS] <COMMAND>

Commands:
  run     Start an interactive chat session
  init    Initialize a .gagent workspace in the current directory
  ralph   RALPH loop commands (plan + build)
  config  Show or modify configuration

Options:
  -v, --verbose  Enable verbose output (debug tracing)
  -h, --help     Print help
  -V, --version  Print version
```

### Interactive Chat

```bash
gagent run                          # default model from config
gagent run --model llama3.2         # override model
gagent run --no-stream              # disable streaming (wait for complete response)
```

The interactive agent can read files, execute commands, search your codebase, and manage git — all through natural language.

### RALPH Loop (Autonomous Building)

The RALPH loop takes a specification and autonomously implements it in two phases:

1. **Planning** — reads your spec, analyzes the codebase, generates an `IMPLEMENTATION_PLAN.md`
2. **Building** — iteratively picks tasks from the plan, implements them, runs tests, and commits

```bash
# Full pipeline: plan + build
gagent ralph run spec.md --max-iterations 10

# With test validation after each iteration
gagent ralph run spec.md --backpressure "cargo test"

# Phases separately
gagent ralph plan spec.md           # generate plan only
gagent ralph build                  # execute existing plan
```

### Configuration

```bash
gagent config show                  # print current config
gagent config init                  # create default config file
```

Edit `.gagent/config.toml` to customize:

```toml
[llm]
provider = "ollama"
base_url = "http://localhost:11434"
model = "llama3.2"
temperature = 0.7

[agent]
name = "gAgent"
emoji = "🌱"
timeout_secs = 600

[sandbox]
mode = "off"                        # "off" | "non-main" | "all"
```

---

## Architecture

gAgent is a Cargo workspace with 8 crates:

```
gagent-core       Core types, config, errors, bootstrap, memory
gagent-llm        LLM provider abstraction + Ollama implementation
gagent-tools      Tool trait, registry, built-in tools (file, shell, git)
gagent-harness    Agent loop, session persistence, context compaction
gagent-ralph      RALPH two-phase loop (planning + building)
gagent-mcp        MCP gateway — bridge to external tool servers
gagent-sandbox    Path containment, Docker sandboxing, execution policies
gagent-cli        Binary entry point (the `gagent` command)
```

### Agent Loop

The core agent loop follows the [OpenClaw agent loop](https://docs.openclaw.ai/concepts/agent-loop) pattern:

```
User input → Assemble context → LLM inference → Tool execution → Repeat → Reply
```

1. Assemble system prompt from bootstrap files + conversation history
2. Send to LLM with available tool definitions
3. If response contains tool calls: execute tools, feed results back, repeat
4. If text response: stream to user
5. Persist session history

### Bootstrap Files

Following the [OpenClaw system prompt convention](https://docs.openclaw.ai/concepts/system-prompt), agent behavior is configured through markdown files in `.gagent/`:

| File | Purpose |
|------|---------|
| `SOUL.md` | Personality and tone |
| `IDENTITY.md` | Agent name and emoji |
| `USER.md` | User profile and preferences |
| `AGENTS.md` | Multi-agent coordination context |
| `TOOLS.md` | Tool usage guidance |
| `MEMORY.md` | Long-term memory (auto-maintained) |

### RALPH Loop

The [Ralph Loop](https://github.com/Endogen/ralph-loop) pattern provides process-level control over autonomous building:

- **Planning phase:** single session, reads spec, outputs a structured task list
- **Building phase:** iterative sessions, each picks one task, implements it, commits
- **Notifications:** JSON events emitted to `.ralph/pending-notification.txt`
- **Backpressure:** optional command (e.g., `cargo test`) run after each iteration

See [RALPH vs OpenClaw](https://kenhuangus.substack.com/p/ralph-vs-openclaw-understanding-process) for a comparison of process-level vs. session-level control.

---

## Project Status

| Phase | Status | Description |
|-------|--------|-------------|
| 0 | **Complete** | Workspace structure, core types, Ollama provider, CLI |
| 1 | Not started | Bootstrap file loading and prompt assembly |
| 2 | Not started | Built-in tools and agent harness (core loop) |
| 3 | Not started | RALPH loop (autonomous planning + building) |
| 4 | Not started | Memory system (cross-session persistence) |
| 5 | Not started | MCP gateway (external tool servers) |
| 6 | Not started | Security and Docker sandboxing |
| 7 | Not started | Polish (TUI, heartbeats, crash recovery) |

See [PHASES.md](PHASES.md) for detailed implementation plans per phase.

---

## Development

```bash
# Build
cargo build --workspace

# Test
cargo test --workspace

# Run with verbose logging
cargo run -p gagent-cli -- -v run

# Check formatting
cargo fmt --check

# Lint
cargo clippy --workspace
```

### Project Documentation

| File | Purpose |
|------|---------|
| [PHASES.md](PHASES.md) | Detailed implementation roadmap |
| [AGENTS.md](AGENTS.md) | Multi-agent architecture and protocols |
| [CLAUDE.md](CLAUDE.md) | AI agent guide for working on this codebase |
| [LICENSE](LICENSE) | MIT License |

---

## Contributing

gAgent is in early development. The architecture is designed for extensibility:

- **New LLM providers:** implement the `LlmProvider` trait in `gagent-llm`
- **New tools:** implement the `Tool` trait in `gagent-tools`
- **New agent types:** define bootstrap context + lifecycle in `gagent-ralph` or a new crate

---

## License

MIT
