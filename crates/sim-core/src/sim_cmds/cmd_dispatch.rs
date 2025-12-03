// SPDX-License-Identifier: MIT
/*~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
 * sim-core/src/sim_cmds/cmd_dispatch.rs: Command line parser and
 * 
 * This code is adapted from the original SIMH project, which contains the
 * following copyright notice. The terms of this Rust-based adaptation are
 * unchanged from the original license terms.
 * 
 * Permission is hereby granted, free of charge, to any person obtaining a
 * copy of this software and associated documentation files (the "Software"),
 * to deal in the Software without restriction, including without limitation
 * the rights to use, copy, modify, merge, publish, distribute, sublicense,
 * and/or sell copies of the Software, and to permit persons to whom the
 * Software is furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL
 * ROBERT M SUPNIK BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 *
 * Except as contained in this notice, the name of Robert M Supnik shall not be
 * used in advertising or otherwise to promote the sale, use or other dealings
 * in this Software without prior written authorization from Robert M Supnik.
 *~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~*/

use nom::{
    Err::{ Error, Failure, Incomplete },
    IResult,
    Parser,
    branch::alt,
    bytes::complete::{ tag, take_until },
    character::complete::{ char, alpha1, line_ending, space0 },
    combinator::map,
    multi::many_till
};

use super::cmd_state::CmdContext;

/// Lookup and execute a command
pub fn exec_command(
    cmdline: &str,
    cmdstate: CmdContext
) -> () {
    // Distinguish between bang ("!") command to run a subprocess and a regular
    // SIMH command from SIM_COMMANDS
    match command_token(cmdline) {
        // Execute a shell command
        Ok((remainder, CommandToken::TokBang(_))) => {

        },
        // Lookup and execute.
        Ok((remainder, CommandToken::TokCommand(cmd))) => {

        },
        // Ignore comments.
        Ok((_, CommandToken::TokComment(_))) => (),
        // Error:
        Err(err) => {
            match err {
                Incomplete(_) => (),
                Error(err) => (),
                Failure(_) => ()
            }
        }
    }
}

// Parsers...
#[derive(Debug, PartialEq)]
pub enum CommandToken<'a> {
    // Bang command
    TokBang(&'a str),
    // Ordinary command
    TokCommand(&'a str),
    // Comment
    TokComment(&'a str),
}

/** Parse the initial token from a command line. Public, used in the examples...
 */
pub fn command_token<'a>(cmdline: &'a str) -> IResult<&'a str, CommandToken<'a>> {
    alt((parse_command,
         parse_comment,
         parse_bang)).parse(cmdline)
}

fn parse_bang<'a>(input: &'a str) -> IResult<&'a str, CommandToken<'a>> {
    map(tag("!"), CommandToken::TokBang).parse(input)
}

fn parse_command<'a>(input: &'a str) -> IResult<&'a str, CommandToken<'a>> {
    map(alpha1, CommandToken::TokCommand).parse(input)
}

fn parse_comment<'a>(input: &'a str) -> IResult<&'a str, CommandToken<'a>> {
    let (cmnt, _) = space0(input)?;
    let (_, _) = char('#')(cmnt)?;

    return Ok((cmnt, CommandToken::TokComment(cmnt)));
}

/* Unit tests: */
#[cfg(test)]
mod tests {
    #[test]
    fn trivial_test() {
        use super::{ CommandToken, command_token };

        assert_eq!(command_token("!dir"), Ok(("dir", CommandToken::TokBang("!"))));
        assert_eq!(command_token("! ls"), Ok((" ls", CommandToken::TokBang("!"))));
        assert_eq!(command_token("load some_file"), Ok((" some_file", CommandToken::TokCommand("load"))));
        assert!(command_token("bad! command").is_err());
    }
}