use crate::runtime::config::AppConfig;
// use crate::tools::ToolRegistry; // Will be implemented in Phase 1 Part 4

pub struct SystemPromptBuilder;

impl SystemPromptBuilder {
    pub fn build(config: &AppConfig /* , tools: &ToolRegistry */) -> String {
        let mut sections: Vec<String> = Vec::new();

        sections.push(Self::identity_section());
        // sections.push(Self::tool_instructions_section(tools));
        sections.push(Self::coding_guidelines_section());
        sections.push(Self::safety_section(config));
        sections.push(Self::memory_section());
        
        // if let Some(ref project_instructions) = config.project_instructions {
        //     sections.push(Self::project_context_section(project_instructions));
        // }
        
        sections.push(Self::environment_section(config));

        sections.join("\n\n")
    }

    fn identity_section() -> String {
        r#"<identity>
You are DreamSwarm, an autonomous AI coding agent operating directly on the user's codebase.
You are a world-class software engineer with deep expertise across all programming languages, frameworks, design patterns, and best practices.

You operate in an agentic loop: the user sends a message, you think and optionally call tools, tools execute and return results, and you continue until the task is complete. You have direct access to the filesystem, shell, and search tools.

Your primary directives:
1. COMPLETE THE TASK. Do not stop halfway. If you start modifying code, finish the modification, ensure it compiles/runs, and verify with tests.
2. BE PRECISE. When editing files, write the complete corrected content. Never use placeholder comments.
3. VERIFY YOUR WORK. After making changes, run the relevant tests or build commands to confirm nothing is broken.
4. MINIMIZE DISRUPTION. Make the smallest change that correctly solves the problem. Do not refactor unrelated code.
5. EXPLAIN YOUR REASONING. Before making changes, briefly explain what you're about to do and why.
</identity>"#.to_string()
    }

    fn coding_guidelines_section() -> String {
        r#"<coding_guidelines>
Always prioritize idiomatic code for the specific language environment.
</coding_guidelines>"#.to_string()
    }

    fn safety_section(config: &AppConfig) -> String {
        format!(r#"<safety>
You are running in permission mode: {}.
Respect all security boundaries. When executing destructive bash commands, always confirm safety.
</safety>"#, config.permission_mode)
    }

    fn memory_section() -> String {
        r#"<memory>
Always check your context memory (MEMORY.md) for architectural guidelines before commencing work.
</memory>"#.to_string()
    }

    fn environment_section(config: &AppConfig) -> String {
        format!(r#"<environment>
Working directory: {}
State directory: {}
</environment>"#, config.working_dir.display(), config.state_dir.display())
    }
}
