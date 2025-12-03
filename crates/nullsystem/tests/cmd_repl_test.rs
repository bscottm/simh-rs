// SPDX-License-Identifier: MIT

mod nullenv;

#[cfg(test)]
mod test {
    use crate::nullenv::NullTestingEnvironment;
    use sim_core::cli::{cli_error::CLIErrorKind, cmd_repl::CmdREPL, parsers::command_line, Span};

    const TESTING_PROMPT: &str = "sim-rs/testing> ";

    fn expect_command<'a>(input: Vec<String>, expected_cmd: &'a str, expected_remainder: Option<&'a str>) {
        let test_env = NullTestingEnvironment::new().expect("Failed to create test env");

        // Generate manifest from our owned environment
        let manifest = test_env.env.resource_manifest();
        let mut cli = CmdREPL::new("TestSystem", manifest);

        cli.stringvec_reader(input).expect("Failed to load input");

        loop {
            match cli.inputs_mut().read_logical_line(TESTING_PROMPT, None) {
                Ok(None) => panic!("Got unexpected EOF"),
                Ok(Some(line)) => {
                    let span = Span::new_with_line(line.as_str(), cli.inputs().get_lineno());

                    match command_line(span) {
                        Ok((_, Some((cmd, rest)))) => {
                            assert_eq!(cmd.as_str(), expected_cmd);
                            if let Some(remainder) = expected_remainder {
                                assert_eq!(rest.input, remainder);
                            }
                            break; // Success
                        }
                        Ok((_, None)) => continue, // Comment line
                        Err(err) => panic!("Parse error: {:?}", err),
                    }
                }
                Err(err) => panic!("Reader error: {:?}", err),
            }
        }
    }

    #[test]
    fn command_reader() {
        let input = vec![r#"command "arg with # hash" other"#.to_string()];
        expect_command(input, "command", Some(" \"arg with # hash\" other"));

        let shell_variants = [
            vec!["!ls".to_string()],
            vec!["!  ls".to_string()],
            vec!["!  ls  ".to_string()],
        ];

        for shell_cmd in shell_variants {
            expect_command(shell_cmd, "!ls", None);
        }
    }

    #[test]
    fn multiline_comment() -> Result<(), Box<dyn std::error::Error>> {
        let inputs = vec![
            vec![
                "## Extended comment block",
                "##",
                "## Last comment line...",
                "   this is a command line",
            ],
            vec![
                "## Extended comment block",
                "        ",
                "   this is a command line",
            ],
            vec!["", "## Extended comment block", "   this is a command line"],
        ];

        for input in inputs {
            next_command_harness(input.iter().map(|s| s.to_string()).collect())?;
        }

        Ok(())
    }

    fn next_command_harness(input: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
        let test_env = NullTestingEnvironment::new()?;
        let mut cli = CmdREPL::new("TestSystem", test_env.env.resource_manifest());

        cli.stringvec_reader(input)?;

        // next_command now takes REPLState internally, so we pass the env ref
        match cli.next_command() {
            Err(cli_err) => {
                // Assert it's an unknown command because "THIS" isn't a valid verb
                if let CLIErrorKind::UnknownCommand(cmd) = cli_err.kind {
                    assert_eq!(cmd, "THIS");
                } else {
                    panic!("Expected UnknownCommand(THIS), got {:?}", cli_err);
                }
            }
            Ok(_) => panic!("Expected error for 'this is a command line', got Ok"),
        }

        Ok(())
    }
}
