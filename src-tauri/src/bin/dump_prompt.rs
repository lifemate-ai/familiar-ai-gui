/// dump_prompt — print the current agent system prompt to stdout.
///
/// Usage:
///   cargo run --bin dump_prompt
///
/// The output is the exact system prompt sent to the LLM at the start of
/// each turn. Pipe it into the eval harness or claude -p for testing.
fn main() {
    let config = familiar_gui_lib::config::Config::load()
        .unwrap_or_default();
    let mut agent = familiar_gui_lib::agent::Agent::new(config);
    print!("{}", agent.eval_system_prompt());
}
