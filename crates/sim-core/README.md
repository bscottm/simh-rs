# sim-core: The Core Simulation Module

`sim-core` is a library that implements the SIMH Control Program (SCP). It is split into
two parts:

- `cli`: The command-line interpreter
- `simenvironment`: The simulator environment that wraps around a simulated system in order
  to control and interact with the simulation.

[Notes to simulated systems][1]

[1]: notes/system_sim.md