// SPDX-License-Identifier: MIT
/*~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
 * sim-core/src/sim_cmds/cmd_table.rs: The SIMH simulator command table
 * module.
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

use std::fs::File;

/**
 * Command line context state.
 */

pub struct CmdContext {
    // Input source (underlying file)
    infile: File,
    // Input source (pretty name, e.g., "<stdin>" for stdin)
    infile_name: String,
    // Input line number.
    lineno: u32,
}

impl CmdContext {
    /// Constructor for CmdContext
    pub fn new(
        infile: File,
        infile_name: String
    ) -> CmdContext {
        CmdContext {
            infile,
            infile_name,
            lineno: 0
        }
    }
}