# gAgent Implementation Phases

Detailed phase-by-phase implementation plan for gAgent. Each phase builds on the previous and has clear deliverables and verification criteria.

---

## Phase 0: Foundation ✅ COMPLETE

**Goal:** Cargo workspace skeleton, core types, LLM provider, working CLI.

### What was built
- **Root `Cargo.toml`** — workspace with 8 members, all deps centralized in `[workspace.dependencies]`
- **`gagent-core`** — `Config` (TOML serialization), `GagentError` (thiserror), `AgentIdentity`
- **`gagent-llm`** — `LlmProvider` trait, `OllamaProvider` (streaming + non-streaming via `/api/chat`), `ChatMessage`/`Role`/`ToolCall`/`StreamChunk` types
- **`gagent-tools`** — `Tool` trait, `ToolDefinition`, `ToolRegistry`
- **`gagent-cli`** — `gagent` binary with `run`, `init`, `ralph`, `config` subcommands
- **Stub crates** — `gagent-harness`, `gagent-ralph`, `gagent-mcp`, `gagent-sandbox`

### Files
```
Cargo.toml
gagent-core/src/{lib,agent,config,error}.rs
gagent-llm/src/{lib,provider,ollama,message}.rs
gagent-tools/src/{lib,definition,registry}.rs
gagent-cli/src/main.rs
gagent-{harness,ralph,mcp,sandbox}/src/lib.rs   (stubs)
```

### Verification
```bash
cargo build --workspace    # clean build
cargo test --workspace     # 7 tests pass
gagent --help              # shows all subcommands
gagent config show         # prints default TOML config
gagent init                # scaffolds .gagent/ workspace
gagent run                 # interactive chat (needs Ollama)
```

---

## Phase 1: Bootstrap Files & Prompt Assembly

**Goal:** Load `.gagent/` bootstrap files into structured system prompts. `gagent init` scaffolds a complete workspace.

### Files to create/modify
```
gagent-core/src/bootstrap.rs    # Load and validate .gagent/ files
gagent-core/src/prompt.rs       # Assemble system prompt from bootstrap context
gagent-core/src/lib.rs          # Add new module exports
gagent-cli/src/main.rs          # Wire bootstrap into run_interactive()
```

### Bootstrap file loading
- Read each `.gagent/*.md` file if it exists
- Enforce 20,000 char/file limit (truncate with warning)
- Enforce 150,000 char total limit across all files
- Parse `IDENTITY.md` for agent name + emoji
- Track which files were loaded for debug output

### Prompt assembly order
1. **Identity block** — from IDENTITY.md: "You are {name} {emoji}"
2. **Personality block** — from SOUL.md: behavioral instructions
3. **Tool guidance block** — from TOOLS.md: how to use available tools
4. **Safety block** — hardcoded: never expose secrets, respect workspace boundaries
5. **Workspace block** — current directory, git branch, project detection
6. **User context block** — from USER.md: user preferences
7. **Agent context block** — from AGENTS.md: multi-agent awareness
8. **Memory block** — from MEMORY.md: long-term learnings
9. **Runtime metadata** — timestamp, session ID, model name, OS info

### Sub-agent context
Sub-agents only receive:
- AGENTS.md
- TOOLS.md
- The specific task prompt

They do NOT get SOUL.md, USER.md, or MEMORY.md.

### `gagent init` enhancements
- Generate richer template files with examples and documentation
- Detect if workspace already exists, offer to merge/skip
- Optional `--force` flag to overwrite

### Verification
- `gagent init` creates complete workspace with all template files
- `gagent run --verbose` shows assembled system prompt in tracing output
- Bootstrap context is properly ordered and within char limits
- Missing files are silently skipped (not errors)
- Files exceeding char limits are truncated with a tracing warning

---

## Phase 2: Tool System & Agent Harness

**Goal:** Built-in tools + the core agent loop that cycles between LLM inference and tool execution.

### Files to create/modify
```
gagent-tools/src/builtin/mod.rs         # Built-in tool module
gagent-tools/src/builtin/file_ops.rs    # FileRead, FileWrite, FileSearch tools
gagent-tools/src/builtin/shell.rs       # Shell command execution tool
gagent-tools/src/builtin/git.rs         # Git operations tool
gagent-harness/src/harness.rs           # Core agent loop
gagent-harness/src/session.rs           # Session persistence (JSONL)
gagent-harness/src/context.rs           # Context budgeting and compaction
gagent-harness/src/timeout.rs           # Timeout wrapper
gagent-harness/src/lib.rs               # Module exports
gagent-cli/src/main.rs                  # Wire harness into run command
```

### Built-in tools

**FileRead** — Read file contents
- Params: `path` (string, required), `offset` (int, optional), `limit` (int, optional)
- Returns file contents as string
- Respects PathGuard boundaries (when Phase 6 is active)

**FileWrite** — Write content to a file
- Params: `path` (string, required), `content` (string, required)
- Creates parent directories if needed
- Returns confirmation with byte count

**FileSearch** — Search files by name pattern or content
- Params: `pattern` (string, required), `path` (string, optional), `content_search` (bool, optional)
- Glob matching for file names, substring/regex for content
- Returns list of matching files with context

**Shell** — Execute a shell command
- Params: `command` (string, required), `timeout_secs` (int, optional)
- Captures stdout + stderr
- Enforced timeout (default: 30s per command, max: configurable)
- Returns exit code + output

**Git** — Git operations
- Params: `operation` (string, required), `args` (string, optional)
- Supported: status, log, diff, add, commit, branch
- Returns operation output

### Agent harness (core loop)

```
┌─────────────────────────────────────────────┐
│                 Agent Loop                    │
│                                               │
│  1. Assemble context (system prompt + history)│
│  2. Call LLM with tool definitions            │
│  3. Parse response                            │
│     ├─ tool_calls? → execute tools            │
│     │               append results            │
│     │               goto step 2               │
│     └─ text only? → return to user            │
│  4. Stream tokens as they arrive              │
│                                               │
│  Timeout: 600s default (configurable)         │
│  Max tool rounds: 20 per turn                 │
└─────────────────────────────────────────────┘
```

### Session persistence
- Session ID: UUID v4, generated at session start
- Session file: `.gagent/sessions/{session_id}.jsonl`
- Format: one JSON object per line (append-only)
- Each line: `{"role":"user","content":"...","timestamp":"ISO-8601"}`
- Resume: `gagent run --session <id>` loads history from file

### Context management
- Approximate token count: `chars / 4` (rough heuristic)
- When context exceeds `max_context_chars`:
  1. Keep system prompt (always)
  2. Keep last N messages (configurable, default: 10)
  3. Summarize dropped messages into a compact "conversation so far" block
  4. Insert summary as a system message before the retained messages
- Compaction is transparent to the user

### Verification
```bash
gagent run
# > "Read the contents of Cargo.toml"
# Agent calls FileRead tool, returns file contents
# > "List files in the current directory"
# Agent calls Shell tool with `ls`, returns listing
# > "What git branch am I on?"
# Agent calls Git tool with status, returns branch info
```

---

## Phase 3: RALPH Loop

**Goal:** Two-phase PLANNING/BUILDING state machine. `gagent ralph run spec.md` goes from spec to working code.

### Files to create/modify
```
gagent-ralph/src/ralph_loop.rs      # Main state machine
gagent-ralph/src/plan.rs            # Parse/update IMPLEMENTATION_PLAN.md
gagent-ralph/src/notification.rs    # Notification emission and formatting
gagent-ralph/src/iteration.rs       # Single building iteration logic
gagent-ralph/src/lib.rs             # Module exports
gagent-cli/src/main.rs              # Wire ralph subcommands
```

### Planning phase
1. Create `.ralph/` directory
2. Start a fresh agent session with planning-specific prompt
3. Load the spec file as user input
4. Agent reads the codebase (using tools) to understand structure
5. Agent generates `IMPLEMENTATION_PLAN.md`
6. Emit `PLANNING_COMPLETE` notification
7. Return the plan to the user

### Building phase
1. Read `IMPLEMENTATION_PLAN.md`
2. For each iteration (up to `--max-iterations`):
   a. Start a fresh agent session (clean context)
   b. Inject plan + recent git log as context
   c. Agent picks next unchecked task
   d. Agent implements the task (code + tests)
   e. If `--backpressure` is set, run the command and feed result back
   f. Agent marks task `[x]` in plan
   g. Agent commits to git
   h. Emit notification
   i. If all tasks done, emit `DONE` and stop
3. If max iterations reached, emit `MAX_ITERATIONS_REACHED`

### Plan parsing
- Parse markdown task list: `- [ ] Task description` / `- [x] Task description`
- Support nested sub-tasks
- Track task indices for updates
- Rewrite file when marking tasks complete

### Backpressure
- Optional shell command run after each iteration
- Example: `--backpressure "cargo test"`
- If command fails (non-zero exit), feed error output to next iteration
- Agent should fix the issue before moving to next task

### CLI
```bash
gagent ralph plan spec.md                    # planning only
gagent ralph build --max-iterations 10       # building only (plan must exist)
gagent ralph run spec.md --max-iterations 10 # plan + build
gagent ralph run spec.md --backpressure "cargo test"
```

### Verification
1. Create `spec.md`: "Build a hello world CLI in Python with argparse"
2. `gagent ralph run spec.md --max-iterations 5`
3. Verify: `IMPLEMENTATION_PLAN.md` created with tasks
4. Verify: Python files written, git commits made
5. Verify: `.ralph/pending-notification.txt` contains JSON notifications

---

## Phase 4: Memory System

**Goal:** Persistent memory across sessions. Agent remembers learnings and user context.

### Files to create/modify
```
gagent-core/src/memory.rs              # Memory read/write/search operations
gagent-core/src/lib.rs                 # Export memory module
gagent-tools/src/builtin/memory.rs     # MemorySearch, MemoryWrite, MemoryRead tools
gagent-tools/src/builtin/mod.rs        # Register memory tools
gagent-harness/src/harness.rs          # End-of-session memory distillation
```

### Memory storage
- `MEMORY.md` — curated long-term memory (max 20,000 chars)
- `memory/YYYY-MM-DD.md` — daily raw entries

### Memory tools (for the agent)

**MemoryRead** — Read memory files
- Params: `file` (string, optional — defaults to MEMORY.md)
- Returns file contents

**MemoryWrite** — Write a memory entry
- Params: `content` (string, required), `file` (string, optional)
- Appends timestamped entry to today's daily file (default)
- Or appends to MEMORY.md if specified

**MemorySearch** — Search across all memory files
- Params: `query` (string, required), `regex` (bool, optional)
- Searches MEMORY.md + all `memory/*.md` files
- Returns matching lines with file + line number context

### Memory lifecycle
1. **During session:** agent uses MemoryWrite to note important learnings
2. **At session end:** harness prompts agent to distill session into key learnings
3. **Periodic consolidation:** during heartbeats, daily files are summarized into MEMORY.md
4. **MEMORY.md maintenance:** when approaching 20k char limit, oldest/least relevant entries are pruned

### Verification
1. Session 1: tell agent "Remember that the database password is stored in vault"
2. Agent writes to `memory/2025-01-15.md`
3. Session 2: ask "Where is the database password stored?"
4. Agent uses MemorySearch, finds the entry, answers correctly

---

## Phase 5: MCP Gateway

**Goal:** Connect to external MCP servers, discover their tools, and make them available to the agent.

### Files to create/modify
```
gagent-mcp/src/mcp_client.rs    # Connect to MCP servers via stdio transport
gagent-mcp/src/mcp_bridge.rs    # Discover tools, register in ToolRegistry, proxy calls
gagent-mcp/src/config.rs        # Parse mcpServers config block
gagent-mcp/src/lib.rs           # Module exports
```

### Dependencies to add
- `rmcp` — Rust MCP SDK (protocol types + stdio transport)
- `notify` — file system watcher for hot-reload

### MCP server configuration
In `.gagent/config.json`:
```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed"]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_TOKEN": "..."
      }
    }
  }
}
```

### MCP client lifecycle
1. Read `mcpServers` from config
2. For each server:
   a. Spawn process via `tokio::process::Command`
   b. Connect via stdio transport (stdin/stdout)
   c. Initialize MCP session
   d. Call `tools/list` to discover available tools
   e. Convert MCP tool schemas to `ToolDefinition`
   f. Register in `ToolRegistry` with namespaced names (e.g., `mcp.filesystem.read_file`)
3. When agent calls an MCP tool, proxy the call to the appropriate server
4. Hot-reload: watch config file with `notify`, reconnect on changes

### Verification
1. Configure a filesystem MCP server in config
2. `gagent run`
3. Ask agent to use the MCP-provided filesystem tool
4. Verify: tool appears in registry, execution works, results return to agent

---

## Phase 6: Security & Sandboxing

**Goal:** Path containment prevents workspace escape. Docker sandboxing isolates shell commands.

### Files to create/modify
```
gagent-sandbox/src/path_guard.rs    # PathGuard: canonicalize + boundary check
gagent-sandbox/src/docker.rs        # Docker sandbox via bollard crate
gagent-sandbox/src/policy.rs        # Execution policies (allow/deny/confirm)
gagent-sandbox/src/lib.rs           # Module exports
gagent-tools/src/builtin/shell.rs   # Integrate sandbox into shell tool
gagent-tools/src/builtin/file_ops.rs # Integrate path guard into file tools
```

### Dependencies to add
- `bollard` — Docker API client

### PathGuard
```rust
pub struct PathGuard {
    allowed_roots: Vec<PathBuf>,
}

impl PathGuard {
    pub fn check(&self, path: &Path) -> Result<PathBuf, GagentError> {
        let canonical = std::fs::canonicalize(path)?;
        for root in &self.allowed_roots {
            if canonical.starts_with(root) {
                return Ok(canonical);
            }
        }
        Err(GagentError::PathNotAllowed(path.display().to_string()))
    }
}
```
- Resolves symlinks via `canonicalize()`
- Rejects any path outside allowed roots
- Applied to ALL file operations (read, write, search)

### Docker sandbox
- Sandbox modes: `off` (default), `non-main` (sandbox on non-main git branches), `all` (always sandbox)
- Docker container config:
  - Mount workspace directory as RW volume
  - Restrict network access (default: no network)
  - Resource limits: 1GB memory, 2 CPUs
  - Auto-cleanup: containers removed after execution
- Uses `bollard` crate for Docker API (not shelling out to `docker` CLI)

### Execution policies
- `allowed_commands` — whitelist (if non-empty, only these are allowed)
- `denied_commands` — blacklist (always rejected)
- `confirm_commands` — require user confirmation before execution
- Applied at the tool level, before sandbox routing

### Verification
1. Enable sandbox mode in config
2. Run shell commands via agent — verify they execute in Docker
3. Attempt path traversal (`../../etc/passwd`) — verify PathGuard blocks it
4. Attempt denied command — verify it's rejected
5. Run `gagent run` on a non-main branch with `non-main` mode — verify sandbox activates

---

## Phase 7: Polish & Advanced Features

**Goal:** Production hardening, UX improvements, additional LLM providers.

### Features
- **Heartbeat system** — periodic agent check-ins configured in HEARTBEAT.md
- **OpenAI-compatible provider** — support LM Studio, vLLM, llama.cpp server via OpenAI API format
- **`ratatui` TUI** — split-pane terminal UI (chat + status + tool output)
- **Global config** — `~/.gagent/config.toml` merged with project-level `.gagent/config.toml`
- **Retry logic** — exponential backoff for LLM calls and tool execution
- **Crash recovery** — auto-save session state, resume on restart
- **Streaming tool output** — show shell command output in real-time
- **Multiple sessions** — switch between active sessions

### No target date — these are quality-of-life improvements to implement as needed.
