# gAgent

A basic Rust project that demonstrates a **Ralph loop** for local agents and executes tasks through **Ollama**.

## What is included

- A small command-line app with in-memory local agents.
- A Ralph loop cycle with three phases:
  1. Plan
  2. Execute (streams output from Ollama)
  3. Reflect
- Interactive commands to list agents and run a loop iteration.

## Requirements

- [Ollama](https://ollama.com/) running locally (`ollama serve`)
- A pulled model (default is `llama3.2`):

```bash
ollama pull llama3.2
```

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

## Configuration

Set environment variables if you want to customize connection details:

```bash
export OLLAMA_URL=http://127.0.0.1:11434
export OLLAMA_MODEL=llama3.2
```

## Test

```bash
cargo test
```
