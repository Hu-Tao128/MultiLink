use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Action {
    RetrieveContext,
    ExecuteSkill,
    GenerateResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub action: Action,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
    pub goal: String,
}

pub struct MinimalPlanner;

impl MinimalPlanner {
    pub fn plan(input: &str) -> Plan {
        let mut steps = Vec::new();
        let lower = input.to_lowercase();

        // 1. All plans need context first
        steps.push(PlanStep {
            action: Action::RetrieveContext,
            description: "Gathering relevant project context and files.".to_string(),
        });

        // 2. Skill check for complex analysis
        if lower.contains("analyze") || lower.contains("review") || lower.contains("optimize") {
            steps.push(PlanStep {
                action: Action::ExecuteSkill,
                description: "Running specialized analysis skill.".to_string(),
            });
        }

        // 3. Final step: generate the response
        steps.push(PlanStep {
            action: Action::GenerateResponse,
            description: "Synthesizing final response based on gathered info.".to_string(),
        });

        Plan {
            steps,
            goal: input.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_plan() {
        let plan = MinimalPlanner::plan("hello there");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].action, Action::RetrieveContext);
        assert_eq!(plan.steps[1].action, Action::GenerateResponse);
    }

    #[test]
    fn test_analysis_plan() {
        let plan = MinimalPlanner::plan("analyze my project");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].action, Action::RetrieveContext);
        assert_eq!(plan.steps[1].action, Action::ExecuteSkill);
        assert_eq!(plan.steps[2].action, Action::GenerateResponse);
    }
}
