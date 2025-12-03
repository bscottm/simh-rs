// SPDX-License-Identifier: MIT

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// "Legacy" devices: This the list of legacy devices that SIMH has solely for the purpose of
// debug flags -- SIMH uses bitmask debug flags embedded in a `DEVICE`-s `DEBTAB` debug table.
//
// With SIMH-RS' capability name-based debugging, the notion of setting a device debugging flag
// disappears. However, SIMH-RS still has to support the "SET <dev> DEBUG=..." syntax even if the
// device name is effectively ignored.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

pub static LEGACY_DEVICES: &[&str] = &[
    "INT-CLOCK",
    "INT-EXPECT",
    "INT-FLUSH",
    "INT-RUNLIMIT",
    "INT-STEP",
    "INT-STOP",
    "INT-THROTTLE",
    "INT-TIMER",
];
