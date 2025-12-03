# Debugging

## Categories

SIMH's debugging is tightly coupled to bit flags embedded in the `DEVICE` structure. This is a reasonable
approach that assumes that simulator and device debugging are static and never needs to be extended. The bit
flag design breaks when applied to Ethernet and the underlying Ethernet emulation.

Consequently, the approach taken in SIMH-RS is more generalizable.
