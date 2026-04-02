use std::collections::HashSet;

pub const REQUIRED_EXECUTION_COMMANDS: [&str; 3] = ["build", "up", "exec"];
pub const REQUIRED_COLLECTION_COMMANDS: [&str; 2] = ["features", "templates"];

pub struct IntrospectionPortingInput {
    pub ok: bool,
    pub read_configuration_ported: bool,
    pub metadata_resolve_ported: bool,
}

pub struct CommandPortingInput {
    pub ok: bool,
    pub ported_commands: Vec<String>,
}

pub struct OutputCompatibilityInput {
    pub ok: bool,
    pub json_schema_parity: bool,
    pub text_output_parity: bool,
}

pub struct CommandPortingInputSet {
    pub introspection_porting: IntrospectionPortingInput,
    pub execution_porting: CommandPortingInput,
    pub collection_porting: CommandPortingInput,
    pub output_compatibility: OutputCompatibilityInput,
}

#[derive(Debug, PartialEq)]
pub enum CommandPortingMissingCheck {
    IntrospectionPorting,
    ExecutionPorting,
    CollectionPorting,
    OutputCompatibility,
}

#[derive(Debug, PartialEq)]
pub struct CommandPortingEvaluation {
    pub complete: bool,
    pub summary: String,
    pub missing_checks: Vec<CommandPortingMissingCheck>,
}

fn has_introspection_porting(input: &IntrospectionPortingInput) -> bool {
    input.ok && input.read_configuration_ported && input.metadata_resolve_ported
}

fn has_command_porting(input: &CommandPortingInput, required_commands: &[&str]) -> bool {
    let ported_commands: HashSet<&str> = input
        .ported_commands
        .iter()
        .map(|command| command.trim())
        .filter(|command| !command.is_empty())
        .collect();

    input.ok
        && required_commands
            .iter()
            .all(|required_command| ported_commands.contains(required_command))
}

fn has_output_compatibility(input: &OutputCompatibilityInput) -> bool {
    input.ok && input.json_schema_parity && input.text_output_parity
}

pub fn evaluate_command_porting(input: &CommandPortingInputSet) -> CommandPortingEvaluation {
    let mut missing_checks = Vec::new();

    if !has_introspection_porting(&input.introspection_porting) {
        missing_checks.push(CommandPortingMissingCheck::IntrospectionPorting);
    }

    if !has_command_porting(
        &input.execution_porting,
        &REQUIRED_EXECUTION_COMMANDS,
    ) {
        missing_checks.push(CommandPortingMissingCheck::ExecutionPorting);
    }

    if !has_command_porting(
        &input.collection_porting,
        &REQUIRED_COLLECTION_COMMANDS,
    ) {
        missing_checks.push(CommandPortingMissingCheck::CollectionPorting);
    }

    if !has_output_compatibility(&input.output_compatibility) {
        missing_checks.push(CommandPortingMissingCheck::OutputCompatibility);
    }

    if missing_checks.is_empty() {
        return CommandPortingEvaluation {
            complete: true,
            summary: "Command porting complete with output compatibility checks satisfied.".to_string(),
            missing_checks,
        };
    }

    let missing_labels = missing_checks
        .iter()
        .map(|missing_check| match missing_check {
            CommandPortingMissingCheck::IntrospectionPorting => "introspection-porting",
            CommandPortingMissingCheck::ExecutionPorting => "execution-porting",
            CommandPortingMissingCheck::CollectionPorting => "collection-porting",
            CommandPortingMissingCheck::OutputCompatibility => "output-compatibility",
        })
        .collect::<Vec<_>>()
        .join(", ");

    CommandPortingEvaluation {
        complete: false,
        summary: format!("Command porting incomplete. Missing: {missing_labels}."),
        missing_checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_input() -> CommandPortingInputSet {
        CommandPortingInputSet {
            introspection_porting: IntrospectionPortingInput {
                ok: true,
                read_configuration_ported: true,
                metadata_resolve_ported: true,
            },
            execution_porting: CommandPortingInput {
                ok: true,
                ported_commands: REQUIRED_EXECUTION_COMMANDS
                    .iter()
                    .map(|command| (*command).to_string())
                    .collect(),
            },
            collection_porting: CommandPortingInput {
                ok: true,
                ported_commands: REQUIRED_COLLECTION_COMMANDS
                    .iter()
                    .map(|command| (*command).to_string())
                    .collect(),
            },
            output_compatibility: OutputCompatibilityInput {
                ok: true,
                json_schema_parity: true,
                text_output_parity: true,
            },
        }
    }

    #[test]
    fn marks_command_porting_complete_when_all_porting_checks_pass() {
        let input = complete_input();
        let result = evaluate_command_porting(&input);

        assert!(result.complete);
        assert!(result.summary.contains("Command porting complete"));
        assert!(result.missing_checks.is_empty());
    }

    #[test]
    fn fails_command_porting_when_execution_commands_are_partially_ported() {
        let mut input = complete_input();
        input.execution_porting.ported_commands = vec!["build".to_string(), "up".to_string()];

        let result = evaluate_command_porting(&input);

        assert!(!result.complete);
        assert_eq!(
            result.missing_checks,
            vec![CommandPortingMissingCheck::ExecutionPorting]
        );
    }

    #[test]
    fn fails_command_porting_when_output_compatibility_is_not_preserved() {
        let mut input = complete_input();
        input.output_compatibility = OutputCompatibilityInput {
            ok: false,
            json_schema_parity: false,
            text_output_parity: true,
        };

        let result = evaluate_command_porting(&input);

        assert!(!result.complete);
        assert_eq!(
            result.missing_checks,
            vec![CommandPortingMissingCheck::OutputCompatibility]
        );
    }
}
