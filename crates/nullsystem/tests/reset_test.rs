// SPDX-License-Identifier: MIT

mod nullenv;

#[cfg(test)]
mod test {
    use sim_core::cli::{
        cli_error::CLIErrorKind,
        cmd_repl::{CmdREPL, CmdToken},
    };
    use sim_core::env::SimResponse;
    use std::cell::RefCell;
    use std::io::Cursor;
    use std::rc::Rc;

    use crate::nullenv::NullTestingEnvironment;

    #[test]
    pub fn reset_cmd() -> Result<(), Box<dyn std::error::Error>> {
        let testenv = NullTestingEnvironment::new().unwrap();

        // Initialize CLI with the manifest from our environment
        let output_sink = Rc::new(RefCell::new(Cursor::new(Vec::new())));
        let mut cli =
            CmdREPL::new_with_sink("TestSystem", testenv.env.resource_manifest(), Some(output_sink));

        let (sim_tx, _sim_rx) = cli.mock_connect();

        // --- PART 1: Happy Path ---
        let happy_commands = vec![
            vec!["reset"],
            vec![" RESET "],
            vec!["reset all"],
            vec!["reset null_input"],
            vec!["reset all  ## comment test"],
        ];

        for cmd in happy_commands {
            let cmd_strings = cmd.iter().map(|s| s.to_string()).collect();
            cli.stringvec_reader(cmd_strings)?;

            // MOCK: The simulator thread would normally receive a message here.
            // We simulate the simulator's "Ok" response immediately.
            sim_tx.send(SimResponse::Ok).unwrap();

            match cli.next_command() {
                Ok(CmdToken::ExecutedCommand) => (),
                other => panic!("Expected ExecutedCommand, got {:?}", other),
            }

            // Ensure we hit EOF after the command
            match cli.next_command() {
                Ok(CmdToken::Eof) => cli.pop_input(),
                other => panic!("Expected EOF after command, got {:?}", other),
            }
        }

        // --- PART 2: Failure Path (Invalid Syntax/Devices) ---
        let failure_cases = vec![
            (vec!["reset bad_device"], "Invalid device"),
            (vec!["reset null_input extra"], "Extraneous input"),
        ];

        for (cmd, msg) in failure_cases {
            let cmd_strings = cmd.iter().map(|s| s.to_string()).collect();
            cli.stringvec_reader(cmd_strings)?;

            match cli.next_command() {
                Err(err) => match err.kind {
                    CLIErrorKind::InvalidDevice(_) => (),
                    CLIErrorKind::ExtraneousInput(_) => (),
                    _ => panic!("Command '{}' failed with wrong error: {:?}", msg, err),
                },
                Ok(token) => panic!("Command '{}' should have failed, got {:?}", msg, token),
            }
            cli.pop_input();
        }

        Ok(())
    }
}
