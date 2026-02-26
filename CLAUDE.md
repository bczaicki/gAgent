# CLAUDE.md — gAgent Project Guide

This file is the authoritative reference for any AI agent (including Claude Code) working on the gAgent codebase. Read it in full before making changes.

---

## What is gAgent?

gAgent is a **local-first, privacy-respecting AI agent** that replicates [OpenClaw](https://openclaw.ai/) capabilities using local LLMs via Ollama and OpenAI-compatible servers. All data stays on disk as human-readable files. No cloud dependencies for core functionality.

**Core philosophy:** Security, privacy, autonomy. The user's data never leaves their machine.

---

## Project Structure

```
gAgent/
  Cargo.toml                     # Workspace root — all deps defined here
  gagent-core/                   # Types, config, errors, bootstrap, memory, agent identity
  gagent-llm/                    # LlmProvider trait + Ollama + OpenAI-compatible implementations
  gagent-tools/                  # Tool trait, ToolRegistry, built-in tools (file, shell, git)
  gagent-harness/                # Core agent loop, session persistence, context compaction
  gagent-ralph/                  # Two-phase RALPH loop (PLANNING/BUILDING), notifications
  gagent-mcp/                    # MCP tool bridge — connect to external MCP servers
  gagent-sandbox/                # Path containment, Docker sandboxing, execution policies
  gagent-cli/                    # Binary entry point (`gagent`), clap CLI
  .gagent/                       # Workspace directory (created by `gagent init`)
```

### Crate Dependency Graph

```
gagent-cli (binary: `gagent`)
├── gagent-core
├── gagent-llm → gagent-core
├── gagent-harness → gagent-core, gagent-llm, gagent-tools
└── (future: gagent-ralph, gagent-mcp, gagent-sandbox)

gagent-ralph → gagent-core, gagent-llm, gagent-tools, gagent-harness
gagent-mcp → gagent-core, gagent-tools
gagent-sandbox → gagent-core
```

**Rule:** `gagent-core` has ZERO internal dependencies. All other crates depend on it. Never create circular dependencies.

---

## Build & Test

```bash
# Build everything
cargo build --workspace

# Run all tests
cargo test --workspace

# Run the CLI
cargo run -p gagent-cli -- --help
cargo run -p gagent-cli -- run                   # interactive chat (needs Ollama running)
cargo run -p gagent-cli -- init                   # scaffold .gagent/ workspace
cargo run -p gagent-cli -- config show            # print current config
# Inside the interactive session, use slash commands:
#   /plan spec.md       — generate IMPLEMENTATION_PLAN.md
#   /build              — run building phase
#   /run spec.md        — full plan + build cycle
```

**Always run `cargo test --workspace` after making changes.** Tests must pass before any commit.

Current test count: 7 tests across gagent-core, gagent-llm, gagent-tools.

---

## Coding Conventions

### Rust Style
- **Edition 2021**, resolver 2
- All workspace deps go in root `Cargo.toml` `[workspace.dependencies]` — crates reference them with `{ workspace = true }`
- Error handling: `thiserror` for error types in `gagent-core::error`, `anyhow` only in `gagent-cli` (binary boundary)
- Async everywhere: `tokio` runtime, `async-trait` for trait objects, `futures::stream::BoxStream` for streaming
- Use `tracing` (not `log` / `println!`) for debug output. `tracing::debug!`, `tracing::warn!`, etc.
- Use `f64` for temperature — f32 causes ugly TOML serialization (0.699999988...)
- Prefer `impl Into<String>` for constructor params that take owned strings

### File Organization
- One module per concept: `config.rs`, `error.rs`, `agent.rs`, `provider.rs`, `ollama.rs`, etc.
- `lib.rs` only contains `pub mod` declarations and `pub use` re-exports
- Tests go in `#[cfg(test)] mod tests` at the bottom of each file
- No `tests/` integration test directories unless specifically needed

### Naming
- Crate names: `gagent-{name}` (kebab-case)
- Module names: `snake_case`
- Types/traits: `PascalCase`
- The binary is named `gagent` (defined in `gagent-cli/Cargo.toml`)

---

## Architecture Patterns

### LLM Provider
The `LlmProvider` trait (`gagent-llm/src/provider.rs`) is the abstraction over LLM backends:
- `chat()` — non-streaming, returns complete `ChatResponse`
- `chat_stream()` — streaming, returns `BoxStream<StreamChunk>`
- Ollama implementation uses `/api/chat` endpoint (NOT `/api/generate`)
- Message format: `ChatMessage` with `Role` enum (System/User/Assistant/Tool)

### Tool System
The `Tool` trait (`gagent-tools/src/definition.rs`):
- `definition()` → `ToolDefinition` (name, description, parameters as JSON schema)
- `execute(params, context)` → `ToolResult`
- Tools are registered in a `ToolRegistry` which can be queried by the LLM and harness

### Agent Loop (Phase 2 — gagent-harness)
The core loop follows [OpenClaw's agent loop](https://docs.openclaw.ai/concepts/agent-loop):
1. Assemble context (system prompt + message history)
2. Call LLM with available tools
3. If response contains tool_calls → execute tools → append results → loop to step 2
4. If text-only response → return to user
5. Stream assistant tokens as they arrive

### RALPH Loop (Phase 3 — gagent-ralph)
Two-phase state machine from [Ralph Loop](https://github.com/Endogen/ralph-loop):
- **PLANNING:** accept spec → generate `IMPLEMENTATION_PLAN.md` → emit `PLANNING_COMPLETE`
- **BUILDING:** iterative — fresh session each iteration → pick next task → implement → test → commit → emit notifications

### Bootstrap Files (Phase 1 — gagent-core)
Following [OpenClaw's bootstrap convention](https://docs.openclaw.ai/concepts/system-prompt):
- `SOUL.md` — personality/tone
- `IDENTITY.md` — agent name + emoji
- `USER.md` — user profile/preferences
- `AGENTS.md` — multi-agent context
- `TOOLS.md` — tool usage guidance
- `MEMORY.md` — long-term memory
- `memory/*.md` — daily memory entries
- Constraint: 20,000 chars/file, 150,000 chars total

---

## Configuration

Config lives at `.gagent/config.toml` (project-level). Loaded by `Config::load()` with fallback to defaults.

```toml
[llm]
provider = "ollama"
base_url = "http://localhost:11434"
model = "llama3.2"
context_length = 8192
temperature = 0.7

[agent]
name = "gAgent"
emoji = "🌱"
workspace_dir = ".gagent"
timeout_secs = 600

[session]
sessions_dir = ".gagent/sessions"
max_messages = 100
max_context_chars = 150000

[sandbox]
mode = "off"                    # "off" | "non-main" | "all"
allowed_paths = []
confirm_commands = []
denied_commands = []
```

---

## Implementation Phases

### Phase 0: Foundation ✅ COMPLETE
Workspace structure, gagent-core (types/config/errors), gagent-llm (LlmProvider + Ollama), gagent-tools (Tool trait + registry), gagent-cli (run/init/ralph/config subcommands), stub crates for future phases.

### Phase 1: Bootstrap Files & Prompt Assembly — NOT STARTED
- `gagent-core/src/bootstrap.rs` — load `.gagent/` files, enforce char limits
- `gagent-core/src/prompt.rs` — assemble system prompt in correct order
- `gagent init` scaffolds workspace with templates
- Prompt order: identity → personality → tooling → safety → workspace → bootstrap → runtime metadata

### Phase 2: Tool System & Agent Harness — NOT STARTED
- `gagent-tools/src/builtin/` — FileRead, FileWrite, FileSearch (glob/grep), Shell, Git
- `gagent-harness/src/harness.rs` — core agent loop (LLM ↔ tool execution cycle)
- `gagent-harness/src/session.rs` — JSONL session persistence, load/resume
- `gagent-harness/src/context.rs` — approximate token counting, auto-compaction
- 600s default timeout, configurable

### Phase 3: RALPH Loop ✅ COMPLETE
- `gagent-ralph/src/ralph_loop.rs` — two-phase state machine
- `gagent-ralph/src/plan.rs` — parse/update IMPLEMENTATION_PLAN.md
- `gagent-ralph/src/notification.rs` — JSON notifications to `.ralph/pending-notification.txt`
- CLI: slash commands `/plan`, `/build`, `/run` inside `gagent run` interactive session; supports `--max-iter` flag
- MockProvider for testing, SystemPrompt derives Clone

### Phase 4: Memory System — NOT STARTED
- `gagent-core/src/memory.rs` — read/write/search memory files
- Memory tools: MemorySearch, MemoryWrite, MemoryRead
- Distill learnings at session end, consolidate during heartbeats

### Phase 5: MCP Gateway — NOT STARTED
- `gagent-mcp/src/mcp_client.rs` — connect to MCP servers via stdio (using `rmcp` crate)
- `gagent-mcp/src/mcp_bridge.rs` — discover tools via `tools/list`, register in ToolRegistry
- Config in `.gagent/config.json` `mcpServers` block
- Hot-reload with `notify` crate

### Phase 6: Security & Sandboxing — NOT STARTED
- `gagent-sandbox/src/path_guard.rs` — `PathGuard` with `canonicalize()`, symlink blocking
- `gagent-sandbox/src/docker.rs` — Docker sandbox via `bollard` crate
- `gagent-sandbox/src/policy.rs` — execution policies (allow/deny/confirm)

### Phase 7: Polish — NOT STARTED
- Heartbeat system, OpenAI-compatible provider, `ratatui` TUI, global config, retry logic, crash recovery

---

## Key Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| `tokio` | Async runtime | `features = ["full"]` |
| `reqwest` | HTTP client | `features = ["json", "stream"]` |
| `serde` / `serde_json` | Serialization | `features = ["derive"]` |
| `clap` | CLI parsing | `features = ["derive"]` |
| `thiserror` | Error derive macros | Used in gagent-core |
| `anyhow` | Error handling | Only in gagent-cli binary |
| `tracing` | Structured logging | With `tracing-subscriber` |
| `async-trait` | Async trait methods | Throughout |
| `futures` | Stream utilities | `BoxStream`, `StreamExt` |
| `toml` | Config serialization | In gagent-core, gagent-cli |

Future deps (not yet added): `bollard` (Docker), `rmcp` (MCP), `notify` (file watching), `ratatui` (TUI), `glob`/`regex` (file search)

---

## Common Mistakes to Avoid

1. **Don't use `/api/generate` for Ollama** — always use `/api/chat` (supports message history + tool calling)
2. **Don't use `f32` for temperature** — use `f64` to avoid ugly TOML/JSON serialization
3. **Don't add deps directly to crate Cargo.toml** — add to `[workspace.dependencies]` in root first, then reference with `{ workspace = true }`
4. **Don't put business logic in gagent-cli** — it's just the CLI layer. Logic goes in the appropriate library crate.
5. **Don't create circular crate dependencies** — gagent-core is the leaf, everything else builds on it
6. **Don't use `println!` for debug output** — use `tracing::debug!` / `tracing::info!`
7. **Don't skip tests** — every new module should have `#[cfg(test)] mod tests` with at least basic coverage
8. **Don't break the existing CLI interface** — `run`, `init`, `config` are the stable subcommands; RALPH is accessed via `/plan`, `/build`, `/run` slash commands inside the interactive session

---

## Security Considerations

gAgent executes code on the user's machine. When implementing tool execution:
- **Always validate paths** against allowed workspace boundaries (Phase 6 PathGuard)
- **Never execute shell commands without timeout** — default 600s, always use `tokio::time::timeout`
- **Sanitize tool parameters** — don't pass raw LLM output to shell commands
- **Log all tool executions** via tracing at `info` level minimum
- **When sandbox mode is active**, route shell commands through Docker
- **Never send data to external services** unless explicitly configured by the user

---

## Working on This Project

When making changes:
1. Read the relevant source files first — understand existing patterns
2. Follow existing code style (check neighboring files)
3. Add tests for new functionality
4. Run `cargo test --workspace` before considering the work done
5. Keep crate boundaries clean — don't leak implementation details across crate boundaries
6. Update this CLAUDE.md if you change architecture or add new patterns
