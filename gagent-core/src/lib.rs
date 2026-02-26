pub mod agent;
pub mod bootstrap;
pub mod config;
pub mod error;
pub mod memory;
pub mod prompt;

pub use agent::AgentIdentity;
pub use bootstrap::{BootstrapFiles, MAX_FILE_CHARS, MAX_TOTAL_CHARS};
pub use config::Config;
pub use error::{GagentError, Result};
pub use memory::{MemoryStore, SearchResult, MAX_MEMORY_FILE_CHARS};
pub use prompt::{PromptAssembler, SystemPrompt};
