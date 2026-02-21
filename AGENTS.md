# AGENTS.md — gAgent Multi-Agent Architecture

This document describes how gAgent's multi-agent system works, the roles of each agent type, and the protocols for agent coordination.

---

## Overview

gAgent supports multiple agent personas operating within a single workspace. Each agent session gets a tailored system prompt assembled from the bootstrap files. The primary mechanism for multi-agent coordination is the **RALPH loop** — a two-phase (PLANNING/BUILDING) pattern where a planning agent generates work, and building agents execute it iteratively.

Agents share state through the filesystem:
- `.gagent/MEMORY.md` — shared long-term memory
- `.gagent/memory/*.md` — daily memory entries
- `.ralph/IMPLEMENTATION_PLAN.md` — shared task list
- `.ralph/pending-notification.txt` — inter-agent notifications
- Git history — implicit shared context

---

## Agent Types

### 1. Interactive Agent (default)

**Activated by:** `gagent run`

The primary conversational agent. Runs an interactive chat loop with the user, has access to all registered tools, and maintains session history.

**Bootstrap context loaded:**
- `SOUL.md` — personality and tone
- `IDENTITY.md` — name and emoji
- `USER.md` — user preferences
- `AGENTS.md` — this file (multi-agent awareness)
- `TOOLS.md` — tool usage guidance
- `MEMORY.md` — long-term memory

**Capabilities:**
- Conversational interaction with the user
- Tool execution (file operations, shell, git, MCP tools)
- Memory reading and writing
- Session persistence and resumption

**System prompt assembly order:**
1. Identity (from IDENTITY.md)
2. Personality (from SOUL.md)
3. Tool definitions and guidance (from TOOLS.md + ToolRegistry)
4. Safety constraints
5. Workspace context (current directory, git status)
6. Bootstrap context (from remaining .gagent/ files)
7. Runtime metadata (timestamp, session ID, model info)

---

### 2. Planning Agent

**Activated by:** `gagent ralph plan <spec>` or the planning phase of `gagent ralph run`

A specialized agent that reads a specification/PRD and produces a structured implementation plan. Runs as a single session — no iteration.

**Bootstrap context loaded:**
- `AGENTS.md` — awareness of its role in the pipeline
- `TOOLS.md` — available tool guidance
- Planning-specific system prompt

**Input:** A spec/PRD file (markdown)

**Output:** `IMPLEMENTATION_PLAN.md` in the `.ralph/` directory with this format:

```markdown
# Implementation Plan

## Overview
Brief description of what will be built.

## Tasks

- [ ] Task 1: Description of first task
  - Details, acceptance criteria, files to modify
- [ ] Task 2: Description of second task
  - Details, acceptance criteria, files to modify
- [ ] Task 3: ...

## Notes
Any architectural decisions, risks, or dependencies.
```

**Constraints:**
- Read-only access to the codebase (may read files to understand structure)
- Does NOT write code or execute shell commands
- Must produce a plan with checkbox-style `[ ]` task markers
- Emits `PLANNING_COMPLETE` notification when done

---

### 3. Building Agent

**Activated by:** `gagent ralph build` or the building phase of `gagent ralph run`

An iterative agent that picks up tasks from the implementation plan and executes them. Each iteration gets a **fresh session** (clean context window) to avoid context degradation.

**Bootstrap context loaded:**
- `AGENTS.md` — awareness of its role
- `TOOLS.md` — tool usage guidance
- `IMPLEMENTATION_PLAN.md` — current task list
- Recent git log — what's been done in previous iterations

**Per-iteration workflow:**
1. Read `IMPLEMENTATION_PLAN.md`
2. Identify the next incomplete task (first unchecked `[ ]`)
3. Implement the task (write code, run commands)
4. Run validation (tests, backpressure command if configured)
5. Mark the task as complete (`[x]`) in the plan
6. Git commit with a descriptive message
7. Emit a notification (PROGRESS/ERROR/BLOCKED/DONE)
8. Terminate the iteration

**Constraints:**
- Full tool access (file read/write, shell, git)
- One task per iteration (focused scope)
- Must commit after completing a task
- Must update the implementation plan
- Fresh context each iteration — does NOT carry over conversation history
- Max iterations configurable (default: 10)

**Termination conditions:**
- All tasks marked `[x]` → emit `DONE`
- Max iterations reached → emit `MAX_ITERATIONS_REACHED`
- Agent signals `STOP` → emit `STOPPED`
- Unrecoverable error → emit `ERROR`

---

### 4. Sub-Agent (future)

**Activated by:** the harness when the primary agent delegates a subtask.

Sub-agents receive a restricted context:
- `AGENTS.md` — role awareness only
- `TOOLS.md` — tool guidance only
- No `SOUL.md`, `USER.md`, or `MEMORY.md` — sub-agents don't have personality or memory

This keeps sub-agents focused and prevents context bloat.

---

## Notification Protocol

Agents communicate asynchronously through notification files. The RALPH loop uses `.ralph/pending-notification.txt`.

**Notification JSON format:**

```json
{
  "timestamp": "2025-01-15T10:30:00Z",
  "project_path": "/path/to/project",
  "agent": "building",
  "message": "PROGRESS: Completed task 3 — added user authentication endpoint",
  "iteration": 3,
  "tasks_total": 8,
  "tasks_complete": 3,
  "status": "pending"
}
```

**Status values:**
- `PLANNING_COMPLETE` — planning phase finished, plan ready
- `PROGRESS` — a task was completed successfully
- `ERROR` — a task failed, includes error details
- `BLOCKED` — agent cannot proceed, needs human intervention
- `DONE` — all tasks complete
- `MAX_ITERATIONS_REACHED` — hit the iteration limit
- `STOPPED` — agent chose to stop (e.g., unclear requirements)

---

## Memory Protocol

All agents share a memory system rooted in `.gagent/`:

### MEMORY.md (Long-Term)
- Distilled learnings, preferences, and facts
- Updated at session end (interactive agent) or after significant events
- Hard limit: 20,000 characters
- When approaching the limit, older/less relevant entries are consolidated

### memory/YYYY-MM-DD.md (Daily)
- Timestamped entries from each session
- Raw observations and context
- Used for memory search across time
- Consolidated into MEMORY.md periodically

### Memory Operations
- **Write:** append a timestamped entry to today's daily file
- **Search:** substring/regex search across all memory files
- **Read:** read MEMORY.md or a specific daily file
- **Consolidate:** summarize daily files into MEMORY.md (during heartbeats)

---

## Context Budgets

Each agent type operates within character budgets to stay within LLM context windows:

| Source | Max Chars | Notes |
|--------|-----------|-------|
| Single bootstrap file | 20,000 | Any .gagent/*.md file |
| Total bootstrap context | 150,000 | All files combined |
| Session history | configurable | Default: 150,000 chars, then auto-compaction |
| Tool definitions | ~5,000 | Grows with number of registered tools |

**Auto-compaction:** when the total context approaches the limit, older messages are summarized into a compact form and the full messages are dropped. The system prompt and most recent messages are always preserved.

---

## Workspace Layout

```
project/
├── .gagent/                    # Agent workspace (created by `gagent init`)
│   ├── config.toml             # Project-level configuration
│   ├── SOUL.md                 # Personality and tone
│   ├── IDENTITY.md             # Agent name and emoji
│   ├── USER.md                 # User profile and preferences
│   ├── AGENTS.md               # This file (multi-agent context)
│   ├── TOOLS.md                # Tool usage guidance
│   ├── MEMORY.md               # Long-term memory
│   ├── HEARTBEAT.md            # Periodic check-in config (Phase 7)
│   ├── BOOTSTRAP.md            # First-run onboarding (deleted after setup)
│   ├── memory/                 # Daily memory entries
│   │   ├── 2025-01-14.md
│   │   └── 2025-01-15.md
│   └── sessions/               # Session history files (JSONL)
│       ├── abc123.jsonl
│       └── def456.jsonl
├── .ralph/                     # RALPH loop state (created by `gagent ralph`)
│   ├── IMPLEMENTATION_PLAN.md  # Task list with checkboxes
│   └── pending-notification.txt # Notification queue
└── ... (project files)
```

---

## Adding a New Agent Type

To add a new agent type:

1. Define its **bootstrap context** — which `.gagent/` files it loads
2. Define its **tool access** — which tools from the registry it can use
3. Define its **system prompt template** — assembled from bootstrap + role-specific instructions
4. Define its **lifecycle** — single session, iterative, or event-driven
5. Define its **notification emissions** — what events it reports
6. Add it to the CLI as a subcommand or invocation mode
7. Document it in this file

---

## Agent Coordination Patterns

### Sequential Pipeline (RALPH)
```
User → Planning Agent → IMPLEMENTATION_PLAN.md → Building Agent (iteration 1)
                                                → Building Agent (iteration 2)
                                                → ...
                                                → DONE notification → User
```

### Interactive with Memory Sharing
```
Session 1 (Interactive Agent) → writes MEMORY.md
Session 2 (Interactive Agent) → reads MEMORY.md → has prior context
```

### Delegated Subtask (future)
```
Interactive Agent → spawns Sub-Agent for focused task
                  → Sub-Agent returns result
                  → Interactive Agent incorporates result
```
