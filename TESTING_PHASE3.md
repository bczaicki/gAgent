# Testing Instructions for Phase 3 (RALPH Loop) PR

## Prerequisites

- Rust toolchain (1.70+)
- Ollama installed and running with a model (e.g., `llama3.2`) OR OpenAI-compatible server
- Git installed

## Quick Validation

```bash
# 1. Build the workspace
cargo build --workspace

# 2. Run all tests (should see 71 tests pass)
cargo test --workspace

# 3. Verify the binary builds
cargo run -p gagent-cli -- --help
```

**Expected:** All tests pass, CLI shows help with `ralph` subcommand options.

---

## Unit Test Coverage

Phase 3 adds comprehensive tests for:

- **notification.rs** — JSON notification writing, event types
- **plan.rs** — IMPLEMENTATION_PLAN.md parsing, task status tracking, checkbox format
- **ralph_loop.rs** — two-phase state machine (PLANNING → BUILDING)
- **mock.rs** — MockProvider for testing without real LLM

```bash
# Run tests with output to see details
cargo test --workspace -- --nocapture

# Run only RALPH tests
cargo test -p gagent-ralph -- --nocapture
```

---

## Manual Testing

### 1. Initialize a Test Workspace

```bash
cd /tmp
mkdir ralph-test && cd ralph-test
cargo run -p gagent-cli -- init
```

**Verify:** `.gagent/` directory created with bootstrap files (SOUL.md, IDENTITY.md, etc.)

### 2. Test RALPH Plan Phase

Create a spec file:

```bash
cat > spec.md << 'EOF'
# Test Feature

Create a simple calculator module with:
- Add function
- Subtract function
- Tests for both
EOF
```

Run planning:

```bash
cargo run -p gagent-cli -- ralph plan spec.md
```

**Expected:**
- `.ralph/IMPLEMENTATION_PLAN.md` created with checkbox tasks
- `.ralph/SPEC.md` copied from input
- `.ralph/pending-notification.txt` contains `PLANNING_COMPLETE` event
- Exit with instructions to run `ralph build`

**Verify:**
- Open `.ralph/IMPLEMENTATION_PLAN.md` — should have `- [ ]` checkbox format tasks
- Check notification format is valid JSON with `event`, `timestamp`, `data` fields

### 3. Test RALPH Build Phase

```bash
cargo run -p gagent-cli -- ralph build --max-iterations 3
```

**Expected:**
- Iterative task execution (one task per iteration)
- Each iteration creates new session in `.gagent/sessions/`
- Tasks marked `[x]` as completed in IMPLEMENTATION_PLAN.md
- Notifications emitted: `ITERATION_STARTED`, `TASK_STARTED`, `TASK_COMPLETED`, `ITERATION_COMPLETE`
- Stops after 3 iterations or when all tasks done

**Verify:**
- Check `.ralph/IMPLEMENTATION_PLAN.md` — tasks should progress from `[ ]` → `[x]`
- Check `.ralph/pending-notification.txt` — should have iteration events
- Check `.gagent/sessions/` — should have multiple `ralph-build-*.jsonl` files

### 4. Test Full RALPH Run

```bash
cargo run -p gagent-cli -- ralph run spec.md --max-iterations 5
```

**Expected:**
- Combines plan + build phases
- Runs planning, then automatically starts building
- Stops after 5 iterations or completion

### 5. Test Backpressure Flag

```bash
cargo run -p gagent-cli -- ralph build --backpressure
```

**Expected:**
- After each iteration, waits for `.ralph/pending-notification.txt` to be deleted
- Prints message: "Waiting for notification acknowledgment..."
- You can manually `rm .ralph/pending-notification.txt` to proceed

---

## Edge Cases to Test

### Empty Plan

```bash
echo "# Empty Spec" > empty.md
cargo run -p gagent-cli -- ralph plan empty.md
```

**Expected:** Creates plan even if LLM returns minimal tasks, no crash.

### Invalid Plan Format

Manually edit `.ralph/IMPLEMENTATION_PLAN.md` to break checkbox format:

```markdown
## Tasks
This is not a checkbox
```

```bash
cargo run -p gagent-cli -- ralph build
```

**Expected:** Graceful handling — either skips malformed tasks or errors clearly.

### Interrupted Build

```bash
# Start build
cargo run -p gagent-cli -- ralph build --max-iterations 10

# Press Ctrl+C after first iteration
^C

# Check state
cat .ralph/IMPLEMENTATION_PLAN.md
```

**Expected:** Partially completed tasks marked `[x]`, rest still `[ ]`. Can resume.

### No Ollama Running

```bash
# Stop Ollama or use wrong URL in config
cargo run -p gagent-cli -- ralph run spec.md
```

**Expected:** Clear error about connection failure, not panic.

---

## Code Review Checklist

- [ ] All 71 tests pass
- [ ] No unwrap() calls that could panic (check new files)
- [ ] Proper error handling with `thiserror` in library crates
- [ ] Tracing logs at appropriate levels (debug/info/warn)
- [ ] MockProvider used in tests (no real LLM calls)
- [ ] Config changes reflected in `.gagent/config.toml` template
- [ ] CLAUDE.md updated with Phase 3 completion status
- [ ] SystemPrompt derives Clone (needed for RALPH loop)
- [ ] Notification JSON format matches spec (event, timestamp, data)
- [ ] Plan parsing handles both `[ ]` and `[x]` checkboxes
- [ ] Session files use JSONL format correctly
- [ ] CLI help text is clear for new ralph subcommands

---

## Performance Testing

```bash
# Test with many tasks (create a complex spec)
cat > complex-spec.md << 'EOF'
# Complex Project
Build a web server with 20+ features...
EOF

cargo run -p gagent-cli -- ralph run complex-spec.md --max-iterations 20
```

**Watch for:**
- Memory usage stays reasonable
- Session files don't grow unbounded (compaction should work)
- Notification writes don't fail under iteration load

---

## Regression Testing

```bash
# Ensure Phase 1 & 2 still work

# Test bootstrap loading
cargo run -p gagent-cli -- run
# Type: "What files are in your bootstrap?"
# Should mention SOUL.md, IDENTITY.md, etc.

# Test tools still work
cargo run -p gagent-cli -- run
# Type: "Read the contents of .gagent/IDENTITY.md"
# Should use FileRead tool successfully
```

---

## Final Checklist

- [ ] `cargo build --workspace` succeeds
- [ ] `cargo test --workspace` shows 71 passing tests
- [ ] `cargo clippy --workspace` shows no warnings
- [ ] `cargo fmt --workspace --check` passes
- [ ] Manual RALPH plan/build/run flows work
- [ ] Notification files written correctly
- [ ] Plan parsing handles checkboxes
- [ ] MockProvider works in tests (no real LLM needed for tests)
- [ ] Existing `run`, `init`, `config` subcommands still work
- [ ] No new dependencies added outside workspace pattern

---

## Questions for PR Author

1. What happens if the LLM returns invalid tool calls during RALPH build?
2. How does the system handle if IMPLEMENTATION_PLAN.md is deleted mid-build?
3. Are there limits on spec file size or plan complexity?
4. What's the expected behavior if a task fails repeatedly?

---

**If all checks pass, the Phase 3 implementation is ready to merge! 🌱**
