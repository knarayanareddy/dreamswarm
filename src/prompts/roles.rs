pub enum SwarmRole {
    Lead,
    FrontendEngineer,
    SystemsProgrammer,
    SecurityResearcher,
    GeneralWorker,
}

impl SwarmRole {
    pub fn from_str(role: &str) -> Self {
        match role.to_lowercase().as_str() {
            "lead" => Self::Lead,
            "frontend" | "frontend-engineer" => Self::FrontendEngineer,
            "systems" | "systems-programmer" | "low-level" => Self::SystemsProgrammer,
            "security" | "security-researcher" | "auditor" => Self::SecurityResearcher,
            _ => Self::GeneralWorker,
        }
    }

    pub fn system_prompt_fragment(&self) -> &str {
        match self {
            Self::Lead => {
                "You are the Swarm Lead. Your goal is to coordinate tasks, maintain the high-level architecture, and ensure all sub-tasks are integrated correctly. Focus on delegation, review, and system-wide stability."
            }
            Self::FrontendEngineer => {
                "You are a Senior Frontend Engineer. Your expertise is in HTML, CSS, Javascript (React/Next.js). Focus on user experience, responsive design, accessibility (A11y), and clean, performant UI components. Use modern patterns and avoid bloated libraries unless necessary."
            }
            Self::SystemsProgrammer => {
                "You are a Senior Systems Programmer. Your expertise is in Rust, C++, and low-level system architecture. Focus on memory safety, zero-cost abstractions, multi-threading, and high-performance throughput. Ensure code is robust, well-documented, and follows idiomatic Rust patterns (RAII, Ownership, etc.)."
            }
            Self::SecurityResearcher => {
                "You are a Senior Security Researcher. Your goal is to identify vulnerabilities, perform threat modeling, and ensure the system is hardened against attacks. Focus on input validation, secure communication, sanitization, and defensive programming. Always look for potential edge cases that could lead to exploits."
            }
            Self::GeneralWorker => {
                "You are an Autonomous Swarm Worker. Follow instructions precisely, use your tools efficiently, and report findings clearly to the lead agent."
            }
        }
    }
}
