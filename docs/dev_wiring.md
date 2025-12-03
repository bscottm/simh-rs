# Device I/O Dispatch: IoPayload, execute_io, DeviceHandle, and wire_devices

## Developer & Maintainer Reference Documentation

This document describes the architecture, design, structural topography, and runtime execution paths of the
device dispatch during instruction execution. The subsystem decouples device ownership from execution loops,
providing a compile-time solution to Rust's strict borrowing constraints while preserving low-overhead,
hardware-accurate execution.

The PDP-8 simulator is the primary example, although this documentation applies across all simulators where
write-side I/O maps directly to a device and can be executed while simulating an instruction.

_Future: Add Z-80 `OUT` instruction, PDP-11 I/O as examples._

## The Problem Being Solved

In the C SIMH simulator design, the CPU can directly reach out to peripheral devices during instruction
execution.  A PDP-8 IOT instruction, for example, needs to call the right device handler for device 03 (TTI)
vs. device 04 (TTO) and pass it the accumulator and interrupt-enable bits.  The naive approach — a big `match`
statement in the CPU or a `HashMap<device_number, Box<dyn Handler>>` — has two problems:

1. **Coupling**: the CPU must know the concrete type (or at least the trait) of every device it might talk to.
   Adding a new device means modifying the CPU.

2. **Performance**: a `HashMap` lookup or dynamic dispatch through a fat pointer on every IOT instruction adds
   latency in the hot simulation loop.

SIMH-RS solves both with a four-part mechanism:

```
+-------------------+        +--------------------+
|     CPUTraits     |        |    DeviceTraits     |
|                   |        |                     |
| type IoPayload    | -----> | execute_io(payload) |
| wire_devices()    |        |                     |
| simulate_instr()  |        +--------------------+
+-------------------+
         |
         v
  DeviceHandle (usize)   -- resolved once at startup, used every instruction
```

---

## DeviceHandle

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceHandle(pub usize);
```

A `DeviceHandle` is a newtype wrapper around a `usize` that indexes directly into `SimEnvironment::devices` —
the `Vec<DeviceMetadata<CPU>>` that holds all registered peripherals.

**Why not a string or a HashMap key?**

The simulation loop is performance sensitive and executes hundreds of millions of instructions per second when
not throttled.  Every PDP-8 IOT, Z-80 port instruction dispatchs directly to a device handler.  A `HashMap`
lookup requires hashing a string and a pointer dereference.  An array index is a single multiply-and-add with
no hashing and no pointer chasing.  At simulator speeds, the difference is measurable.

**Sentinel values**

```rust
pub const CPU_DEVICE_ID: usize = usize::MAX;
```

A simulated CPU fulfills both the `CPUTraits` and `DeviceTraits` traits, meaning that the CPU is a device with
extra traits. Consequently, it is treated specially and not stored in the `SimEnvironment::devices` vector.
The CPU's device identifier, `CPU_DEVICE_ID`, is a sentinel value that signals to
`ResourceLocator::device_index` that a resource belongs to the CPU itself rather than to any registered
device.  It is never stored in a `DeviceHandle` — handles only refer to entries in the `devices` Vec.

---

## The IoPayload Associated Type

```rust
pub trait CPUTraits: Debug + SimResourceProvider + Sized {
    type IoPayload;
    // ...
}
```

`IoPayload` is a CPU-specific struct, enum or `()` that carries whatever context a device needs to service an
I/O request.  It is defined by the CPU, not by the device. `()` is useful when additional I/O payload context
is unneeded.

For the PDP-8, a single IOT instruction encodes the device number (6 bits) and the pulse code (3 bits).  That
is *all* the payload needs to carry:

```rust
pub struct PDP8IoPayload {
    pub device: u8,   // 6-bit device number from bits 3-8 of the IOT word
    pub pulse:  u16,  // 3-bit pulse code from bits 0-2
}
```

**The payload does not duplicate CPU registers.** Accumulator, interrupt flags, data field, MQ — all of those
are live on the `&mut CPU` reference that `execute_io` also receives.  A device reads `cpu.acc` directly
rather than a stale copy in the payload.  This keeps the payload minimal and avoids any question of which copy
of AC is authoritative.

The general rule for what belongs in `IoPayload` (from the PDP-8 simulator's perspective):

```
Belongs in IoPayload            Does NOT belong in IoPayload
-----------------------         ----------------------------
Instruction-encoded data        CPU register values
  device number                   accumulator (cpu.acc)
  pulse bits                      link flag
  opcode subfields                data field (cpu.df)
  immediate operands              MQ register
                                  interrupt flags (cpu.int_req)
                                  (all readable via &mut CPU)
```

For a PDP-11-style CPU with memory-mapped I/O, the payload would carry the bus address and data word from the
instruction decode — the things the device cannot obtain from the CPU directly because they are transient
instruction-level values:

```rust
pub struct PDP11IoPayload {
    pub address: u32,    // decoded bus address
    pub data:    u16,    // data word (for writes)
    pub write:   bool,   // true = write cycle, false = read cycle
}
```

For a CPU that does not use `execute_io` at all:

```rust
type IoPayload = ();   // zero-sized, compiled away entirely
```

**Why an associated type instead of a trait method parameter?**

Using an associated type ties the payload to the CPU at the type level.  A device that implements
`DeviceTraits<PDP8Processor>` knows at compile time that its `execute_io` payload is `PDP8IoPayload`.  There
is no dynamic dispatch, no `dyn Any` downcasting, and no risk of passing the wrong payload to the wrong
device.

---

## execute_io

```rust
pub trait DeviceTraits<CPU: CPUTraits>: Debug + SimResourceProvider {
    fn execute_io(
        &mut self,
        payload: &CPU::IoPayload,
        cpu:     &mut CPU,
        sysbus:  &mut SystemBus,
    ) -> Result<(), SimError> {
        Ok(())   // default: ignore the I/O request
    }
}
```

`execute_io` is called by the CPU *on a specific device* during instruction execution.  The CPU has already
resolved which device to call (via a `DeviceHandle`) and passes its `IoPayload` along. The simulation
environment passes its `SystemBus` so that the `execute_io` handler has access to the console and the event
scheduling/timer subsystems.

The device receives:
- `payload` — the instruction-encoded I/O parameters (device number, pulse, etc.)
- `cpu` — a full mutable reference to the CPU struct, giving direct typed access to all registers, flags, and
  memory
- `sysbus` - 

Both parameters together give the device everything it needs.  The payload answers "what operation was
requested"; the CPU reference answers "what is the current machine state" and is the mechanism through which
the device writes results back.

The default implementation does nothing and returns `Ok(())`, so devices that are not I/O-capable (e.g., a
disk controller that only responds to timer events) need not override it.

### Call flow during instruction execution

The following illustrates the control flow that occurs to read a character from the PDP-8's TTI (teletype
input) device. The TTI's `self.flag` is `true` when a character is available to be read:

```
CPU::simulate_instruction()
    |
    | -- decodes IOT instruction
    | -- builds IoPayload { device=03, pulse=6 }
    |
    v
DeviceAccessor::get_device_mut(handle)   -- O(1) Vec index
    |
    v
dyn DeviceTraits::execute_io(&payload, &mut cpu)
    |
    | -- TTI::execute_io():
    |      reads payload.pulse  to know which operation (KRB = pulse 6)
    |      reads cpu.acc        to get current accumulator if needed
    |      writes cpu.acc       to load the keyboard buffer
    |      clears self.flag
    v
  (returns Ok or Err)
```

---

## wire_devices and finalize_hardware

### The problem: name-to-handle resolution

When a PDP-8 CPU decodes an IOT instruction it knows the 6-bit device number.  To call the right `execute_io`
it needs a `DeviceHandle` for that device.  Those handles are stable (the `devices` Vec does not change after
registration), so the CPU can resolve them **once** at startup and cache the results internally.

`wire_devices` is the hook that makes this possible:

```rust
pub trait CPUTraits {
    fn wire_devices(&mut self, devices: &dyn DeviceAccessor<Self>) {
        // default: do nothing
    }
}
```

### DeviceAccessor

```rust
pub trait DeviceAccessor<CPU: CPUTraits> {
    fn resolve_handle(&self, name: &str) -> Option<DeviceHandle>;
    fn get_device_mut(&mut self, handle: DeviceHandle)
        -> Option<&mut (dyn DeviceTraits<CPU> + Send)>;
}
```

`DeviceAccessor` is the minimal interface the CPU sees during wiring and during execution.  It has two
operations:

- `resolve_handle(name)` — slow path, used only once during startup.  Returns the `DeviceHandle` for the named
  device (i.e., the index into the `SimEnvironment::devices` Vec), or `None` if it is not registered.
- `get_device_mut(handle)` — fast path, used on every I/O instruction.  An `#[inline(always)]` array index
  into the `SimEnvironment::devices` Vec.

`ActiveDevices` in `simenv.rs` is the concrete type that implements this trait.  It is a zero-cost newtype
wrapper around `&mut Vec<DeviceMetadata<CPU>>`:

```rust
pub(crate) struct ActiveDevices<'a, CPU: CPUTraits>(
    pub(crate) &'a mut Vec<DeviceMetadata<CPU>>
);
```

### finalize_hardware

`SimEnvironment::finalize_hardware` is the method called by `simloop.rs` after all devices are registered and
the CLI connection is established, but *before* execution begins:

```rust
pub fn finalize_hardware(&mut self) {
    let accessor = ActiveDevices(&mut self.devices);
    self.cpu.wire_devices(&accessor);
}
```

It constructs an `ActiveDevices` view over the device list and hands it to the CPU.  The CPU iterates over the
devices it cares about, calls `resolve_handle` for each by name, and stores the resulting handles.

### PDP-8 example

```rust
impl CPUTraits for PDP8Processor {
    type IoPayload = PDP8IoPayload;

    fn wire_devices(&mut self, devices: &dyn DeviceAccessor<Self>) {
        // Resolve device handles once and cache them.
        // The CPU stores these in its own fields for O(1) access later.
        self.tti_handle = devices.resolve_handle("TTI");
        self.tto_handle = devices.resolve_handle("TTO");
        self.rk_handle  = devices.resolve_handle("RK");
        // Missing devices are silently ignored (None).
    }

    fn simulate_instruction(
        &mut self,
        sysbus:  &mut SystemBus,
        devices: &mut dyn DeviceAccessor<Self>,
    ) -> Result<(), SimError> {
        let instr = self.memory[self.pc as usize];
        self.pc = (self.pc + 1) & VALUE_MASK;

        match (instr >> 9) & 0o7 {
            // ... other opcodes ...

            0o6 => {
                // IOT instruction: bits 3-8 = device, bits 0-2 = pulse
                let device_num = (instr >> 3) & 0o077;
                let pulse      = instr & 0o7;

                let payload = PDP8IoPayload {
                    device: device_num as u8,
                    pulse:  pulse as u8,
                    // No AC copy -- devices read cpu.acc directly.
                };

                // Dispatch to the right device using a pre-resolved handle.
                let handle = match device_num {
                    0o03 => self.tti_handle,
                    0o04 => self.tto_handle,
                    0o74 => self.rk_handle,
                    _    => None,
                };

                if let Some(h) = handle {
                    if let Some(device) = devices.get_device_mut(h) {
                        device.execute_io(&payload, self)?;
                    }
                }
            }

            // ... remaining opcodes ...
        }
        Ok(())
    }
}
```

---

## Full Lifecycle

```
STARTUP PHASE
=============

  main()
    |
    +-- SimEnvironment::new(cpu)
    |     Creates CPU, SystemBus (timer + console), empty devices Vec.
    |
    +-- env.add_standalone(Box::new(TTI::new()))
    +-- env.add_standalone(Box::new(TTO::new()))
    +-- env.add_controller(Box::new(RKController::new()))
    |     Appends to devices Vec, rebuilds resource_index.
    |
    +-- CmdREPL::new("PDP-8", env.resource_manifest())
    |     CLI receives a snapshot of device/resource metadata.
    |
    +-- cli.sim_connect(&mut env)
    |     Wires up mpsc channels between CLI and simulator threads.
    |
    +-- run_simulator(env)            <-- env moves into simulator thread
          |
          +-- env.measure_initial_ips()
          |     Runs CPU in a tight loop to calibrate timer.
          |
          +-- env.finalize_hardware()
          |     Calls cpu.wire_devices(&ActiveDevices(&mut devices))
          |     CPU resolves "TTI" -> DeviceHandle(0)
          |                  "TTO" -> DeviceHandle(1)
          |                  "RK"  -> DeviceHandle(2)
          |     and caches these handles in its own fields.
          |
          +-- env.set_running(false)
          +-- (enters message loop)


EXECUTION PHASE (per instruction)
==================================

  actual_sim_loop() destructures SimEnvironment:
    let cpu     = &mut env.cpu;
    let bus     = &mut env.bus;
    let devices = &mut env.devices;
    let mut accessor = ActiveDevices(devices);

    cpu.simulate_instruction(&mut bus, &mut accessor)
      |
      | (for an IOT 03 6 instruction: KRB -- read keyboard into AC)
      |
      +-- builds PDP8IoPayload { device=03, pulse=6, ac=current_ac, ... }
      +-- handle = self.tti_handle           -- DeviceHandle(0), O(1) field read
      +-- accessor.get_device_mut(handle)    -- devices[0].device_mut(), O(1)
      +-- tti.execute_io(&payload, cpu)
            |
            +-- reads payload.pulse == 6: KRB operation
            +-- loads self.buffer into cpu.acc
            +-- clears self.flag
            +-- returns Ok(())


BORROW STRUCTURE (why SimEnvironment is destructured in simloop)
================================================================

  SimEnvironment owns: cpu, bus, devices, resource_index, ...

  simulate_instruction needs:
    &mut cpu      (to modify registers)
    &mut bus      (to read console, advance timer)
    &mut devices  (to call execute_io on a device)

  All three are disjoint fields, but the Rust borrow checker cannot see that
  through a method call on &mut SimEnvironment.  Destructuring with 'let'
  bindings in simloop.rs makes the disjointness visible to the compiler:

    let SimEnvironment { ref mut cpu, ref mut bus, ref mut devices, .. } = env;

  This is why the hot loop lives in simloop.rs rather than as a method on
  SimEnvironment itself.
```

---

## Design Invariants

1. **Handles are stable after registration.** The `devices` Vec is not reordered or resized after
   `finalize_hardware` is called.  A handle that was valid at wiring time is valid for the entire simulation
   run.

2. **`resolve_handle` is only called during wiring.** It is a O(n) linear scan of the device list.  This is
   acceptable at startup but not in the hot loop.  Never call `resolve_handle` from `simulate_instruction`.

3. **`IoPayload` is CPU-defined, not device-defined.** Devices are generic over `CPU` and receive whatever
   payload the CPU produces.  A device can only assume the payload fields that the CPU documentation promises.

4. **Missing devices are silently ignored.** `resolve_handle` returns `Option<DeviceHandle>`.  If the user has
   not registered a TTI, the handle is `None` and IOT 03 instructions become no-ops.  This matches real
   hardware behavior where an unoccupied I/O address is simply ignored.


---

## Direct CPU State Manipulation from execute_io

The `&mut CPU` parameter is not a restricted interface — it is a full mutable reference to the CPU struct.  A
device can read and write any public field on the CPU directly, with the same access as the CPU itself.

This is deliberate.  Real peripheral hardware is deeply entangled with the CPU:

- A teletype input device (TTI) must load a character into the accumulator AND set an interrupt-pending flag
  in the same operation.
- A floating-point processor (FPP) may read and write MQ, AC, and the instruction field registers in a single
  IOT sequence.
- A memory extension controller (KM8-E) must switch the instruction field register atomically with the JMP
  instruction it intercepts.

Routing all of this through return values or shared flags would require the CPU to interpret device output
after every I/O operation, reintroducing the coupling the `IoPayload` design was trying to remove.

### What a device can do with the CPU reference

```
Device capability via &mut CPU
===============================

  Read CPU state:
    cpu.acc          -- accumulator (read directly, not from payload)
    cpu.mq           -- multiplier-quotient register
    cpu.df           -- data field
    cpu.int_req      -- current interrupt request flags
    cpu.dev_enb      -- device enable mask
    cpu.memory[addr] -- directly read memory (e.g. for DMA)

  Write CPU state:
    cpu.acc          -- load a value into AC (e.g. KRB loads keyboard buffer)
    cpu.pc           -- advance PC to implement a skip (see skip section below)
    cpu.int_req.insert(InterruptFlags::TTI)  -- raise an interrupt
    cpu.int_req.remove(InterruptFlags::TTI)  -- clear an interrupt
    cpu.dev_enb      -- modify device enable mask (e.g. KIE)
    cpu.memory[addr] -- DMA write directly into memory
```

### PDP-8 examples

**KRB (IOT 03 6) — Read keyboard and clear flag:**

```rust
fn execute_io(
    &mut self,
    payload: &PDP8IoPayload,
    cpu: &mut PDP8Processor,
) -> Result<(), SimError> {
    match payload.pulse {
        // KCF (pulse 1): clear keyboard flag
        1 => { self.flag = false; }

        // KCC (pulse 4): clear AC bits 0-11 and keyboard flag
        4 => {
            cpu.acc &= !VALUE_MASK;
            self.flag = false;
        }

        // KRS (pulse 0o34): OR keyboard buffer into AC
        0o34 => {
            cpu.acc |= self.buffer as u16;
        }

        // KRB (pulse 0o36): clear AC, load buffer, clear flag, request interrupt
        0o36 => {
            // Read current link bit, replace AC with keyboard buffer.
            cpu.acc = (cpu.acc & LINK_MASK) | (self.buffer as u16);
            self.flag = false;
            if cpu.dev_enb.contains(InterruptFlags::TTI) {
                cpu.int_req.insert(InterruptFlags::TTI);
            }
        }

        _ => {}
    }
    Ok(())
}
```

Note that `cpu.acc` is read and written directly — there is no copy of the accumulator in the payload.

**KIE (pulse 0o35) — Set interrupt enable for console devices:**

```rust
0o35 => {
    // AC bit 0 controls whether console interrupts are enabled.
    // The device reads cpu.acc directly and writes cpu.dev_enb directly.
    if cpu.acc & 1 != 0 {
        cpu.dev_enb.insert(InterruptFlags::TTI | InterruptFlags::TTO);
    } else {
        cpu.dev_enb.remove(InterruptFlags::TTI | InterruptFlags::TTO);
    }
}
```

### The skip problem

Many PDP-8 IOT instructions include a "skip if flag set" operation (e.g. KSF, TSF, DSKP).  The CPU needs to
know whether to skip the next instruction.  There are two clean approaches:

**Option A — Return a skip indicator:**

Add a return value to `execute_io`:

```rust
fn execute_io(...) -> Result<bool, SimError>  // true = skip next
```

The CPU checks the return value and increments PC by an extra 1 if needed.  Simple, but it limits each IOT to
a single skip decision.

**Option B — Let the device modify PC directly:**

```rust
// In execute_io:
if self.flag {
    cpu.pc = (cpu.pc + 1) & VALUE_MASK;  // skip
}
```

This matches what real hardware does (the device drives the SKIP line on the I/O bus) and requires no changes
to the `execute_io` signature.  It is the approach that scales to multi-function IOT instructions.

Both are valid.  The choice is a simulator-wide convention, not a framework constraint.

### Safety note

Because `execute_io` receives `&mut CPU`, a device can in principle corrupt CPU state arbitrarily — write a
bad PC, corrupt memory, clear interrupt enables it does not own.  This is the correct trade-off for a
simulator: the goal is accuracy to real hardware, not protection against buggy device implementations.  Device
implementors should restrict their writes to the fields their hardware counterpart actually drives.  The type
system will not stop you from writing `cpu.memory[0] = 0o7402` from a disk controller; hardware convention and
code review should.

---



Understanding why SIMH-RS diverges from the C SIMH design helps clarify the intent of each part of the
mechanism.

### C SIMH's approach

In C SIMH, device I/O is handled through a dispatch table of function pointers stored in a `DEVICE` struct.
The CPU calls a single global function (`sim_activate`, `do_iocycle`) which walks a table of all registered
devices to find the one matching the current I/O address.  Device state is accessed via global variables or
unit-relative pointers.

```
C SIMH model:

  CPU executes IOT 03 6
    |
    +-- calls sim_IOT(03, 6, &AC)     // central dispatcher
          |
          +-- walks global device_table[] looking for dev_num == 03
          +-- calls device_table[i].io_handler(03, 6, &AC)
          +-- result written back to AC
```

Problems with this in Rust:
- Global mutable state is `unsafe`.
- Walking a table on every I/O instruction is O(n) in the number of devices.
- The function pointer table carries no type information; everything is `void *` in C, which becomes `*mut ()`
  and `unsafe` casts in Rust.

### SIMH-RS's approach

SIMH-RS inverts the relationship.  The CPU builds a typed payload and dispatches directly to a pre-indexed
device through a vtable.  No globals, no table walk, no unsafe.

```
SIMH-RS model:

  CPU executes IOT 03 6
    |
    +-- payload = PDP8IoPayload { device=03, pulse=6, ac=AC, ... }
    +-- handle  = self.tti_handle         // DeviceHandle(0), stored at wiring time
    +-- devices[handle.0].execute_io(&payload, cpu)   // O(1), typed, safe
```

The trade-off is that the CPU must call `wire_devices` at startup to resolve handles.  This is a one-time O(n)
cost that eliminates a per-instruction O(n) cost.

---

## Adding a New Device: Step-by-Step

To add a new I/O device to a SIMH-RS simulator, a contributor needs to touch exactly four things.  No other
files need to change.

### 1. Implement the device struct

```rust
// in my_device.rs

#[derive(Debug, SimResources)]
pub struct MyDevice {
    #[resource(name = "FLAG", bits = 1, fmt = format_bit)]
    pub flag: bool,
}

impl MyDevice {
    pub fn new() -> Self { Self { flag: false } }
}
```

### 2. Implement DeviceTraits including execute_io

```rust
impl DeviceTraits<PDP8Processor> for MyDevice {
    fn device_name(&self) -> &str { "MYD" }
    fn description(&self) -> &str { "My new device" }

    fn execute_io(
        &mut self,
        payload: &PDP8IoPayload,
        cpu: &mut PDP8Processor,
    ) -> Result<(), SimError> {
        // payload.device == 0o77 (device number from IOT instruction)
        // payload.pulse  == 1   (pulse bits)
        match payload.pulse {
            1 => { self.flag = false; Ok(()) }
            2 => { cpu.acc = (cpu.acc & 0o7777) | (self.flag as u16); Ok(()) }
            _ => Ok(()),
        }
    }

    fn device_reset(&mut self) { self.flag = false; }
}
```

### 3. Register the device in main.rs

```rust
pdp8_environment
    .add_standalone(Box::new(kl8e::TTI::new()))
    .add_standalone(Box::new(kl8e::TTO::new()))
    .add_standalone(Box::new(my_device::MyDevice::new()));  // <-- add here
```

### 4. Wire the handle in the CPU's wire_devices

```rust
// in cpu.rs, add a field to PDP8Processor:
pub myd_handle: Option<DeviceHandle>,

// in wire_devices:
fn wire_devices(&mut self, devices: &dyn DeviceAccessor<Self>) {
    self.tti_handle = devices.resolve_handle("TTI");
    self.tto_handle = devices.resolve_handle("TTO");
    self.myd_handle = devices.resolve_handle("MYD");  // <-- add here
}

// in simulate_instruction, in the IOT dispatch:
0o77 => self.myd_handle,   // <-- add here
```

That is the complete change set.  The device is now:
- Accessible via `EXAMINE MYD FLAG` and `DEPOSIT MYD FLAG` through the CLI
- Serviced by timer events if it implements `device_service`
- Dispatched to correctly on IOT 77 instructions during execution

---

## Thread Safety

The device dispatch mechanism is single-threaded by design.  All device access happens on the simulator
thread.  The CLI thread communicates exclusively via `mpsc` channels — it never touches `devices` directly.

```
CLI thread                     Simulator thread
----------                     ----------------
CmdREPL::run()                 actual_sim_loop()
    |                               |
    | -- SimRequest::Examine -->    |
    |                               +-- env.read_resource_by_name()
    |                               +-- returns value
    | <-- SimResponse::ExamineData -+
    |
    | -- SimRequest::Deposit -->    |
    |                               +-- env.write_resource_by_name()
    | <-- SimResponse::Ok ----------+
```

`execute_io` and `device_service` are both called from within the simulator thread's instruction loop.  The
`DeviceHandle` indices are read-only after `finalize_hardware` completes and require no synchronization.

---

## Relationship to the Resource System

The device I/O dispatch (`execute_io`, `DeviceHandle`, `IoPayload`) and the CLI resource system
(`SimResourceProvider`, `get_metadata`, `read_resource`, `write_resource`) serve different masters and operate
on different timescales.

```
Two independent access paths to a device:

  CLI path (slow, name-based, CLI thread):
    "EXAMINE TTI FLAG"
      |
      +-- resource_index.get(("TTI", "FLAG"))  -> ResourceLocator
      +-- devices[locator.device_index].read_resource(locator.local_id, 0)
      +-- returns u64 value via mpsc channel

  Execution path (fast, handle-based, simulator thread):
    IOT 03 6
      |
      +-- devices[tti_handle.0].execute_io(&payload, cpu)
      +-- device updates its own state directly
```

Both paths reach the same device struct, but through different mechanisms:
- The CLI path is mediated by `SimEnvironment` and goes through the channel.  It uses
  `ResourceLocator::device_index` (a plain `usize`) and the `SimResourceProvider` trait.
- The execution path is direct, using `DeviceHandle` and `DeviceTraits`.

A device implementor does not need to coordinate between the two paths.  The CLI always reads committed state.
During execution the CLI is blocked on the channel waiting for a response; no concurrent access occurs.

---

## Summary Reference

| Concept | What it is | When it is used |
|---|---|---|
| `DeviceHandle` | `usize` index into `devices` Vec | Every I/O instruction |
| `CPU_DEVICE_ID` | `usize::MAX` sentinel | `ResourceLocator` for CPU resources |
| `IoPayload` | CPU-defined struct with I/O context | Built per I/O instruction |
| `execute_io` | Device method that handles one I/O operation | Called with `IoPayload` |
| `wire_devices` | CPU hook called once at startup | Resolves names to handles |
| `resolve_handle` | O(n) name-to-handle lookup | Only inside `wire_devices` |
| `finalize_hardware` | `SimEnvironment` method calling `wire_devices` | After all devices registered |
| `ActiveDevices` | Zero-cost `DeviceAccessor` wrapper | Passed to `simulate_instruction` |
| `DeviceAccessor` | Trait with `resolve_handle` + `get_device_mut` | CPU's view of devices |
