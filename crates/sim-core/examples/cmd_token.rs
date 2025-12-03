use sim_core::sim_cmds::cmd_dispatch::{
    command_token,
    CommandToken
};

use nom::{
    Err::{Incomplete, Error, Failure }
};

fn main() -> ()
{
    let cmds = &[
        "!dir",
        "! ls",
        "1bad token",
        "load some_file -qual -qual -qual",
        "",
        "# This is a comment.",
        "  # This is another comment.",
        "-- A Haskell comment."];

        for cmd in cmds {
        match command_token(cmd) {
            Ok((remainder, CommandToken::TokBang(_))) => {
                println!("TokBang: {remainder:#?}")
            },
            Ok((remainder, CommandToken::TokCommand(cmd))) => {
                println!("TokCommand: {cmd:#?} {remainder:#?}")
            },
            Ok((_, CommandToken::TokComment(cmnt))) => {
                println!("TokComment: {cmnt:#?}")
            },
            Err(err) => {
                match err {
                    // Will only happen if we use streaming (but since we use complete parser
                    // combinators, it won't happen.)
                    Incomplete(s) => {
                        println!("Incomplete input: {:#?}", s);
                    },
                    Error(err) => {
                        println!("Parsing error on input {:#?}", err.input);
                    },
                    Failure(fail) => {
                        println!("Failure: {fail:#?}")
                    }
                }
            }
        }
    }
}