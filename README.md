# gAgent

A basic Rust project that demonstrates a **Ralph loop** for local agents.

## What is included

- A small command-line app with in-memory local agents.
- A Ralph loop cycle with three phases:
  1. Plan
  2. Execute
  3. Reflect
- Interactive commands to list agents and run a loop iteration.

## Run

```bash
cargo run
```

Example usage inside the prompt:

```text
> list
> run scout summarize local docs
> run builder create rust stub
> exit
```

## Test

```bash
cargo test
```
