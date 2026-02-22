use gagent_core::{BootstrapFiles, Config, PromptAssembler};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_bootstrap_and_prompt_integration() {
    let workspace = TempDir::new().unwrap();
    let base = workspace.path();

    fs::write(base.join("IDENTITY.md"), "name: TestBot\nemoji: 🤖\n").unwrap();
    fs::write(base.join("SOUL.md"), "Friendly and helpful").unwrap();
    fs::write(base.join("USER.md"), "Power user").unwrap();

    let bootstrap = BootstrapFiles::load(base).unwrap();
    assert_eq!(bootstrap.identity.name, "TestBot");

    let config = Config::default();
    let assembler = PromptAssembler::new(config, bootstrap);
    let prompt = assembler.assemble();

    assert!(prompt.text.contains("You are 🤖 TestBot"));
    assert!(prompt.text.contains("Friendly and helpful"));
    assert!(prompt.text.contains("Safety Guidelines"));
}
