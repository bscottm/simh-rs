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

/// Command action type alias
type CmdAction = fn(&str) -> Result<(), String>;

/// Command table structure
pub struct CmdTable {
    /// Command name
    name: &'static str,
    /// Command action function
    action: CmdAction,
    /// Help index text (locator)
    help_locator: &'static str
}

impl CmdTable {
    /// Create a new command table entry
    pub const fn new(
        name: &'static str,
        action: CmdAction,
        help_locator: &'static str
    ) -> Self {
        CmdTable { name, action, help_locator }
    }
}

// The SIMH command table:
fn cmd_placeholder(_args: &str) -> Result<(), String> {
    Err("Command not implemented".to_string())
}

/// The default global command table
const SIM_COMMANDS: &[CmdTable] = &[
    CmdTable::new("RESET", cmd_placeholder, HLP_RESET),
    CmdTable::new("EXAMINE", cmd_placeholder, HLP_EXAMINE),
    CmdTable::new("IEXAMINE", cmd_placeholder, HLP_IEXAMINE),
    CmdTable::new("DEPOSIT", cmd_placeholder, HLP_DEPOSIT),
    CmdTable::new("IDEPOSIT", cmd_placeholder, HLP_IDEPOSIT),
    CmdTable::new("EVALUATE", cmd_placeholder, HLP_EVALUATE),
    CmdTable::new("RUN", cmd_placeholder, HLP_RUN),
    CmdTable::new("GO", cmd_placeholder, HLP_GO),
    CmdTable::new("STEP", cmd_placeholder, HLP_STEP),
    CmdTable::new("NEXT", cmd_placeholder, HLP_NEXT),
    CmdTable::new("CONTINUE", cmd_placeholder, HLP_CONTINUE),
    CmdTable::new("BOOT", cmd_placeholder, HLP_BOOT),
    CmdTable::new("BREAK", cmd_placeholder, HLP_BREAK),
    CmdTable::new("NOBREAK", cmd_placeholder, HLP_NOBREAK),
    CmdTable::new("DEBUG", cmd_placeholder, HLP_DEBUG),
    CmdTable::new("NODEBUG", cmd_placeholder, HLP_NODEBUG),
    CmdTable::new("ATTACH", cmd_placeholder, HLP_ATTACH),
    CmdTable::new("DETACH", cmd_placeholder, HLP_DETACH),
    CmdTable::new("ASSIGN", cmd_placeholder, HLP_ASSIGN),
    CmdTable::new("DEASSIGN", cmd_placeholder, HLP_DEASSIGN),
    CmdTable::new("SAVE", cmd_placeholder, HLP_SAVE),
    CmdTable::new("RESTORE", cmd_placeholder, HLP_RESTORE),
    CmdTable::new("GET", cmd_placeholder, PLACEHOLDER_HELP),
    CmdTable::new("LOAD", cmd_placeholder, HLP_LOAD),
    CmdTable::new("DUMP", cmd_placeholder, HLP_DUMP),
    CmdTable::new("EXIT", cmd_placeholder, HLP_EXIT),
    CmdTable::new("QUIT", cmd_placeholder, PLACEHOLDER_HELP),
    CmdTable::new("BYE", cmd_placeholder, PLACEHOLDER_HELP),
    CmdTable::new("CD", cmd_placeholder, HLP_CD),
    CmdTable::new("PWD", cmd_placeholder, HLP_PWD),
    CmdTable::new("DIR", cmd_placeholder, HLP_DIR),
    CmdTable::new("LS", cmd_placeholder, HLP_LS),
    CmdTable::new("TYPE", cmd_placeholder, HLP_TYPE),
    CmdTable::new("CAT", cmd_placeholder, HLP_CAT),
    CmdTable::new("DELETE", cmd_placeholder, HLP_DELETE),
    CmdTable::new("RM", cmd_placeholder, HLP_RM),
    CmdTable::new("COPY", cmd_placeholder, HLP_COPY),
    CmdTable::new("CP", cmd_placeholder, HLP_CP),
    CmdTable::new("RENAME", cmd_placeholder, HLP_RENAME),
    CmdTable::new("MOVE", cmd_placeholder, HLP_MOVE),
    CmdTable::new("MV", cmd_placeholder, HLP_MOVE),
    CmdTable::new("MKDIR", cmd_placeholder, HLP_MKDIR),
    CmdTable::new("RMDIR", cmd_placeholder, HLP_RMDIR),
    CmdTable::new("SET", cmd_placeholder, HLP_SET),
    CmdTable::new("SHOW", cmd_placeholder, HLP_SHOW),
    CmdTable::new("DO", cmd_placeholder, HLP_DO),
    CmdTable::new("GOTO", cmd_placeholder, HLP_GOTO),
    CmdTable::new("RETURN", cmd_placeholder, HLP_RETURN),
    CmdTable::new("SHIFT", cmd_placeholder, HLP_SHIFT),
    CmdTable::new("CALL", cmd_placeholder, HLP_CALL),
    CmdTable::new("ON", cmd_placeholder, HLP_ON),
    CmdTable::new("IF", cmd_placeholder, HLP_IF),
    CmdTable::new("ELSE", cmd_placeholder, HLP_IF),
    CmdTable::new("PROCEED", cmd_placeholder, HLP_PROCEED),
    CmdTable::new("IGNORE", cmd_placeholder, HLP_IGNORE),
    CmdTable::new("ECHO", cmd_placeholder, HLP_ECHO),
    CmdTable::new("ECHOF", cmd_placeholder, HLP_ECHOF),
    CmdTable::new("ASSERT", cmd_placeholder, HLP_ASSERT),
    CmdTable::new("SEND", cmd_placeholder, HLP_SEND),
    CmdTable::new("NOSEND", cmd_placeholder, HLP_SEND),
    CmdTable::new("EXPECT", cmd_placeholder, HLP_EXPECT),
    CmdTable::new("NOEXPECT", cmd_placeholder, HLP_EXPECT),
    CmdTable::new("SLEEP", cmd_placeholder, HLP_SLEEP),
    CmdTable::new("!", cmd_placeholder, HLP_SPAWN),
    CmdTable::new("HELP", cmd_placeholder, HLP_HELP),
    CmdTable::new("SCREENSHOT", cmd_placeholder, HLP_SCREENSHOT),
    CmdTable::new("TAR", cmd_placeholder, HLP_TAR),
    CmdTable::new("CURL", cmd_placeholder, HLP_CURL),
    CmdTable::new("RUNLIMIT", cmd_placeholder, HLP_RUNLIMIT),
    CmdTable::new("NORUNLIMIT", cmd_placeholder, HLP_RUNLIMIT),
    CmdTable::new("TESTLIB", cmd_placeholder, HLP_TESTLIB),
    CmdTable::new("DISKINFO", cmd_placeholder, HLP_DISKINFO),
    CmdTable::new("ZAPTYPE", cmd_placeholder, PLACEHOLDER_HELP)
];

const PLACEHOLDER_HELP : &'static str = "*Placeholder";
const HLP_RESET : &'static str = "*Commands Resetting Devices";
const HLP_EXAMINE : &'static str = "*Commands Examining_and_Changing_State";
const HLP_IEXAMINE : &'static str = "*Commands Examining_and_Changing_State";
const HLP_DEPOSIT : &'static str = "*Commands Examining_and_Changing_State";
const HLP_IDEPOSIT : &'static str = "*Commands Examining_and_Changing_State";
const HLP_EVALUATE : &'static str = "*Commands Evaluating_Instructions";
const HLP_LOAD : &'static str = "*Commands Loading_and_Saving_Programs LOAD";
const HLP_DUMP : &'static str = "*Commands Loading_and_Saving_Programs DUMP";
const HLP_SAVE : &'static str = "*Commands Saving_and_Restoring_State SAVE";
const HLP_RESTORE : &'static str = "*Commands Saving_and_Restoring_State RESTORE";
const HLP_RUN : &'static str = "*Commands Running_A_Simulated_Program RUN";
const HLP_GO : &'static str = "*Commands Running_A_Simulated_Program GO";
const HLP_CONTINUE : &'static str = "*Commands Running_A_Simulated_Program CONTINUE";
const HLP_STEP : &'static str = "*Commands Running_A_Simulated_Program STEP";
const HLP_NEXT : &'static str = "*Commands Running_A_Simulated_Program NEXT";
const HLP_BOOT : &'static str = "*Commands Running_A_Simulated_Program BOOT";
const HLP_BREAK : &'static str = "*Commands Stopping_The_Simulator User_Specified_Stop_Conditions BREAK";
const HLP_NOBREAK : &'static str = "*Commands Stopping_The_Simulator User_Specified_Stop_Conditions BREAK";
const HLP_DEBUG : &'static str = "*Commands Stopping_The_Simulator User_Specified_Stop_Conditions DEBUG";
const HLP_NODEBUG : &'static str = "*Commands Stopping_The_Simulator User_Specified_Stop_Conditions DEBUG";
const HLP_RUNLIMIT : &'static str = "*Commands Stopping_The_Simulator User_Specified_Stop_Conditions RUNLIMIT";
const HLP_ATTACH : &'static str = "*Commands Connecting_and_Disconnecting_Devices ATTACH";
const HLP_DETACH : &'static str = "*Commands Connecting_and_Disconnecting_Devices DETACH";
const HLP_CD : &'static str = "*Commands Controlling_Simulator_Operating_Environment Working_Directory CD";
const HLP_PWD : &'static str = "*Commands Controlling_Simulator_Operating_Environment Working_Directory PWD";
const HLP_DIR : &'static str = "*Commands Listing_Files DIR";
const HLP_LS : &'static str = "*Commands Listing_Files LS";
const HLP_TYPE : &'static str = "*Commands Displaying_Files TYPE";
const HLP_CAT : &'static str = "*Commands Displaying_Files CAT";
const HLP_DELETE : &'static str = "*Commands Removing_Files DEL";
const HLP_RM : &'static str = "*Commands Removing_Files RM";
const HLP_COPY : &'static str = "*Commands Copying_Files COPY";
const HLP_CP : &'static str = "*Commands Copying_Files CP";
const HLP_RENAME : &'static str = "*Commands Renaming_Files RENAME";
const HLP_MOVE : &'static str = "*Commands Renaming_Files MOVE";
const HLP_MKDIR : &'static str = "*Commands Creating_Directories MKDIR";
const HLP_RMDIR : &'static str = "*Commands Deleting_Directories RMDIR";
const HLP_SET : &'static str = "*Commands SET";
const HLP_SET_CONSOLE : &'static str = "*Commands SET CONSOLE";
const HLP_SET_REMOTE : &'static str = "*Commands SET REMOTE";
const HLP_SET_DEFAULT : &'static str = "*Commands SET Working_Directory";
const HLP_SET_LOG : &'static str = "*Commands SET Log";
const HLP_SET_DEBUG : &'static str = "*Commands SET Debug";
const HLP_SET_BREAK : &'static str = "*Commands SET Breakpoints";
const HLP_SET_THROTTLE : &'static str = "*Commands SET Throttle";
const HLP_SET_CLOCK : &'static str = "*Commands SET Clock";
const HLP_SET_ASYNCH : &'static str = "*Commands SET Asynch";
const HLP_SET_ENVIRON : &'static str = "*Commands SET Environment";
const HLP_SET_ON : &'static str = "*Commands SET Command_Status_Trap_Dispatching";
const HLP_SET_VERIFY : &'static str = "*Commands SET Command_Execution_Display";
const HLP_SET_MESSAGE : &'static str = "*Commands SET Command_Error_Status_Display";
const HLP_SET_QUIET : &'static str = "*Commands SET Command_Output_Display";
const HLP_SET_PROMPT : &'static str = "*Commands SET Command_Prompt";
const HLP_NOAUTOSIZE : &'static str = "*Commands SET NoAutosize";
const HLP_SHOW : &'static str = "*Commands SHOW";
const HLP_SHOW_CONFIG : &'static str = "*Commands SHOW";
const HLP_SHOW_DEVICES : &'static str = "*Commands SHOW";
const HLP_SHOW_FEATURES : &'static str = "*Commands SHOW";
const HLP_SHOW_QUEUE : &'static str = "*Commands SHOW";
const HLP_SHOW_TIME : &'static str = "*Commands SHOW";
const HLP_SHOW_MODIFIERS : &'static str = "*Commands SHOW";
const HLP_SHOW_NAMES : &'static str = "*Commands SHOW";
const HLP_SHOW_SHOW : &'static str = "*Commands SHOW";
const HLP_SHOW_VERSION : &'static str = "*Commands SHOW";
const HLP_SHOW_DEFAULT : &'static str = "*Commands SHOW";
const HLP_SHOW_CONSOLE : &'static str = "*Commands SHOW";
const HLP_SHOW_REMOTE : &'static str = "*Commands SHOW";
const HLP_SHOW_BREAK : &'static str = "*Commands SHOW";
const HLP_SHOW_LOG : &'static str = "*Commands SHOW";
const HLP_SHOW_DEBUG : &'static str = "*Commands SHOW";
const HLP_SHOW_THROTTLE : &'static str = "*Commands SHOW";
const HLP_SHOW_ASYNCH : &'static str = "*Commands SHOW";
const HLP_SHOW_ETHERNET : &'static str = "*Commands SHOW";
const HLP_SHOW_SERIAL : &'static str = "*Commands SHOW";
const HLP_SHOW_SYNC : &'static str = "*Commands SHOW";
const HLP_SHOW_MULTIPLEXER : &'static str = "*Commands SHOW";
const HLP_SHOW_VIDEO : &'static str = "*Commands SHOW";
const HLP_SHOW_CLOCKS : &'static str = "*Commands SHOW";
const HLP_SHOW_ON : &'static str = "*Commands SHOW";
const HLP_SHOW_DO : &'static str = "*Commands SHOW";
const HLP_SHOW_RUNLIMIT : &'static str = "*Commands SHOW";
const HLP_SHOW_SEND : &'static str = "*Commands SHOW";
const HLP_SHOW_EXPECT : &'static str = "*Commands SHOW";
const HLP_HELP : &'static str = "*Commands HELP";
const HLP_ASSIGN : &'static str = "*Commands Logical_Names";
const HLP_DEASSIGN : &'static str = "*Commands Logical_Names";
const HLP_DO : &'static str = "*Commands Executing_Command_Files";
const HLP_GOTO : &'static str = "*Commands Executing_Command_Files GOTO";
const HLP_RETURN : &'static str = "*Commands Executing_Command_Files RETURN";
const HLP_SHIFT : &'static str = "*Commands Executing_Command_Files SHIFT";
const HLP_CALL : &'static str = "*Commands Executing_Command_Files CALL";
const HLP_ON : &'static str = "*Commands Executing_Command_Files Error_Trapping";
const HLP_PROCEED : &'static str = "*Commands Executing_Command_Files PROCEED";
const HLP_IGNORE : &'static str = "*Commands Executing_Command_Files PROCEED";
const HLP_ECHO : &'static str = "*Commands Executing_Command_Files Displaying_Arbitrary_Text ECHO_Command";
const HLP_ECHOF : &'static str = "*Commands Executing_Command_Files Displaying_Arbitrary_Text ECHOF_Command";
const HLP_SEND : &'static str = "*Commands Executing_Command_Files Injecting_Console_Input";
const HLP_EXPECT : &'static str = "*Commands Executing_Command_Files Reacting_To_Console_Output";
const HLP_SLEEP : &'static str = "*Commands Executing_Command_Files Pausing_Command_Execution";
const HLP_ASSERT : &'static str = "*Commands Executing_Command_Files Testing_Simulator_State";
const HLP_IF : &'static str = "*Commands Executing_Command_Files Testing_Simulator_State";
const HLP_EXIT : &'static str = "*Commands Exiting_The_Simulator";
const HLP_SCREENSHOT : &'static str = "*Commands Screenshot_Video_Window";
const HLP_SPAWN : &'static str = "*Commands Executing_System_Commands";
const HLP_TESTLIB : &'static str = "*Commands Testing_Device_Libraries";
const HLP_TAR : &'static str = "*Commands File_Tools Tar_Tool";
const HLP_CURL : &'static str = "*Commands File_Tools Curl_Tool";
const HLP_DISKINFO : &'static str = "*Commands Disk_Container_Information";
