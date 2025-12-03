// SPDX-License-Identifier: MIT

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// Input radix options
///
/// This enum determines how scalars are parsed. A simulator sets its default input radix, e.g., PDP-11
/// expects octal, wheras VAXen expect hexadecimal. The EXAMINE and DEPOSIT commands can override the input
/// radix, i.e., change to decimal from octal or even binary. Even with a default radix, though, the user can
/// specify the number's radix by a "0b" (binary), "0d" (decimal), "0o" (octal) or "0x" (hexadecimal) numeric
/// prefix.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum InputRadix {
    Bin,
    Dec,
    Oct,
    Hex,
}
