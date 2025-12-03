# SIMH-RS: System Archaeology Implemented in Rust

_SIMH-RS_ is a rewrite of the venerable computer system archaeology collections, [SIMHv3], [open-simh] and
[SIMH]. _SIMH-RS_'s code is adapted from the [open-simh] and [SIMH] codebases. It continues to be licensed
under the MIT License without restrictions and maintains the original [SIMH] disclaimers.

## SIMH-RS Documentation

- [`SimEnvironment`: The Simulation Environment](docs/sim_environment.md)
- [Device I/O Dispatch](docs/dev_wiring.md)

## Differences from C SIMH

There are a few differences between SIMH-RC and SIMH/open-simh:

- Parsing the command line is more structured. For example, the EXAMINE command's syntax is:

    `EXAMINE [@output_file] [mask_op value] [cmp_op value}] resource[,resource]`

  SIMH-RS parses the arguments in the above order, whereas SIMH/open-simh accepts the arguments in any order
  and the output redirection, mask and comparison expressions could occur multiple times and anywhere in the
  command line.
  
- Debugging: Debugging has been completely reworked and is no longer closely tied to bit flags in
  `DEVICE`-s.
  
  "Categories" have replaced SIMH `DEVICE` bit flags. A category is a string registered and hashed in SIMH-RS'
  global debugging category registry. The category's hash value is used to determine whether it's enabled in
  the `sim_debug!` and `cli_debug!` macros, which is fast but obviously not as fast as a NOT/AND masking
  combination. However, capabilities enable extensibility without weird assumptions about bitmasks and
  debugging-related functionality across mulitple devices or units.
  
  The implication is that the `device` in `SET device DEBUG=arg` is ignored, since debugging is no longer tied
  to the device, but to the debugging capabilty's name.
  
  For more, see the [debugging system](docs/Debugging.md)

## Motivation (Why? Isn't SIMH good enough?)

The code for all three "original" emulation collections is written for an older C dialect, with one author
claiming pride in the fact that "SIMH can still compile using Microsoft Visual C 2008!" Moreover, the code is
messy -- try reading the `_eth_reader` function in `sim_ether.c` -- and has more than a few issues identified
by static and runtime analyzers (inconsistent mutex locking around `pthread_cond_signal`, as one example.)

Not to mention the Imperious Troll Overhead every time one contributes code to Open-SIMH...

[SIMHv3]: https://simh.trailing-edge.com/
[open-simh]: https://opensimh.org/
[SIMH]: https://github.com/simh/simh

