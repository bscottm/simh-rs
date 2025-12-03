// SPDX-License-Identifier: MIT

#[cfg(test)]
mod nullenv;

#[cfg(test)]
mod test {
    use sim_core::env::{SimRequest, SimResponse};
    use sim_core::{
        cli::{
            cli_error::CLIErrorKind,
            cmd_repl::{CmdREPL, CmdToken},
        },
        env::{ExamineResult, ResourceCLIMetadata},
    };
    use std::cell::RefCell;
    use std::io::Cursor;
    use std::rc::Rc;
    use std::time::Duration;

    use crate::nullenv::{cmd_output_to_string, NullTestingEnvironment};

    #[test]
    pub fn examine_command_verification() -> Result<(), Box<dyn std::error::Error>> {
        let testenv = NullTestingEnvironment::new().unwrap();
        // Use the existing, holy function:
        let manifest = testenv.env.resource_manifest();

        // FIXME: Add backs: "e state", "e pc,vec00" and "e devzero dup2"
        // FIXME: Add back: "e devzero pC" (unknown resource)

        // Verify that 'e pc' resolves correctly and communicates with the simulator
        executed_command_with_verification(
            &manifest,
            vec!["examine pc"],
            "PC", // The canonical name the Simulator expects
            0x1234,
        )?;

        Ok(())
    }

    fn executed_command_with_verification(
        manifest: &Vec<ResourceCLIMetadata>,
        cmd: Vec<&str>,
        expected_resource_name: &str,
        mock_val: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let output_sink = Rc::new(RefCell::new(Cursor::new(Vec::new())));

        let mut cli = CmdREPL::new_with_sink("TestSystem", manifest.clone(), Some(output_sink.clone()));

        cli.stringvec_reader(cmd.iter().map(|s| s.to_string()).collect())?;

        // MOCK SIMULATOR
        let (sim_tx, sim_rx) = cli.mock_connect();
        let expected_name = expected_resource_name.to_uppercase();
        let handle = std::thread::spawn(move || {
            // Wait for the CLI message with a timeout to avoid hanging the test runner
            match sim_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(SimRequest::Examine(reqs)) => {
                    // VERIFY: The parser correctly mapped the name and created the request
                    assert_eq!(reqs[0].resource_name, expected_name);

                    // RESPOND: Simulate the device returning data
                    let response_data = vec![Ok(ExamineResult::Values(vec![mock_val]))];
                    sim_tx.send(SimResponse::ExamineData(response_data)).unwrap();
                }
                Ok(other) => panic!("Expected Examine request, got {:?}", other),
                Err(e) => panic!("Mock simulator timed out: {}", e),
            }
        });

        match cli.next_command() {
            Ok(CmdToken::ExecutedCommand) => {
                handle.join().map_err(|_| "Mock thread panicked")?;
                Ok(())
            }
            other => panic!("Expected ExecutedCommand, got {:?}", other),
        }
    }

    #[test]
    fn test_ambiguous_resource() -> Result<(), Box<dyn std::error::Error>> {
        let testenv = NullTestingEnvironment::new().unwrap();
        let manifest = testenv.env.resource_manifest();
        let output_sink = Rc::new(RefCell::new(Cursor::new(Vec::new())));

        let mut cli = CmdREPL::new_with_sink("Test", manifest, Some(output_sink.clone()));
        cli.stringvec_reader(vec!["e dup2".to_string()])?;

        match cli.next_command() {
            Err(e) => {
                if let CLIErrorKind::AmbiguousResource { res, devices } = e.kind {
                    assert_eq!(res, "DUP2");
                    // Verify the manifest correctly identified the conflict
                    assert!(devices.contains("DEVZERO"));
                    assert!(devices.contains("DEVONE"));

                    // Show the output
                    println!("{}", cmd_output_to_string(output_sink));
                } else {
                    // println!("Unexpected result, output sink is '{}'", cmd_output_to_string(*output_sink));
                    panic!("Expected AmbiguousResource, got {:?}", e.kind);
                }
            }
            Ok(_) => panic!("Expected error for ambiguous resource"),
        }
        Ok(())
    }
}
