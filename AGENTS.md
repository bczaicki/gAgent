# gAgent: Multi-Agent Coordination

## Overview

This project implements a **local-first, privacy-preserving AI agent system** called `gAgent` (Green Agent). The architecture supports both single-agent workflows and multi-agent coordination through the [RALPH Loop](https://github.com/Endogen/ralph-loop) pattern.

## Core Agent

The primary agent, `gAgent`, operates using the [OpenClaw agent loop](https://docs.openclaw.ai/concepts/agent-loop) pattern:

```
User input → Assemble context → LLM inference → Tool execution → Repeat → Reply
```

Key capabilities:
- Local-first execution with Ollama/LLM providers
- Privacy by default (no cloud data transfer)
- Autonomous building via RALPH loop
- Multi-agent coordination through shared context

## Multi-Agent Coordination

The `AGENTS.md` file defines context for coordinating multiple agents. For single-agent use, this file can describe:
- Agent roles and responsibilities
- Communication protocols
- Task delegation patterns

Example multi-agent setup:

```markdown
## Agent Roles

- `gAgent`: Primary reasoning and task planning
- `gBuilder`: Code generation and execution
- `gTester`: Test automation and validation

## Communication

Agents communicate via shared memory and message passing through the `.gagent/memory/` directory.
```

## RALPH Loop Integration

For autonomous workflows, agents can participate in the RALPH loop:

1. **Planning phase**: Single session to generate an `IMPLEMENTATION_PLAN.md`
2. **Building phase**: Iterative execution with task selection and git commits

## Configuration

Agents are configured through:
- `.gagent/IDENTITY.md` (agent name/emoji)
- `.gagent/SOUL.md` (personality/tone)
- `.gagent/TOOLS.md` (tool usage guidelines)

## Project Status

| Phase | Status | Description |
|-------|--------|-------------|
| 0 | Complete | Core agent loop, Ollama integration, CLI |
| 3 | In Progress | RALPH loop implementation |
| 4 | Not started | Memory system |

For detailed implementation plans, see [PHASES.md](PHASES.md).
