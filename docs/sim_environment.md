# Simulation environment and simulator system support

## High-Level Thread Model

```
+-------------------------------------------------------------------------+
|                          CLI Thread                                     |
|  +------------------------------------------------------------------+   |
|  | CmdREPL                                                          |   |
|  |  - Command parsing                                               |   |
|  |  - Terminal I/O                                                  |   |
|  |  - Device manifest (metadata cache)                              |   |
|  +------------------------------------------------------------------+   |
|                           |                    ^                        |
|                           | SimRequest         | SimResponse            |
|                           v                    |                        |
+-------------------------------------------------------------------------+
                            |                    |
                    +-------+--------------------+--------+
                    |      MPSC Channels (messages.rs)    |
                    +-------+--------------------+--------+
                            |                    |
+---------------------------v--------------------v------------------------+
|                       Simulator Thread                                  |
|  +------------------------------------------------------------------+   |
|  | SimEnvironment<CPU>                        (simenv.rs)           |   |
|  |  +------------------------------------------------------------+  |   |
|  |  | cpu: CPU                                                   |  |   |
|  |  |  - simulate_instruction(&mut SystemBus, &mut ActiveDevices)|  |   |
|  |  |  - wire_devices(&dyn DeviceAccessor)                       |  |   |
|  |  |  - disassemble(address) -> Option<(String, usize)>         |  |   |
|  |  +------------------------------------------------------------+  |   |
|  |  +------------------------------------------------------------+  |   |
|  |  | bus: SystemBus                          (sysbus.rs)        |  |   |
|  |  |  - timer: TimerManager                                     |  |   |
|  |  |  - console: Option<SimConsole>                             |  |   |
|  |  +------------------------------------------------------------+  |   |
|  |  +------------------------------------------------------------+  |   |
|  |  | devices: Vec<DeviceMetadata<CPU>>                          |  |   |
|  |  |  - indexed by DeviceHandle (usize)                         |  |   |
|  |  |  - CPU_DEVICE_ID = usize::MAX for CPU resources            |  |   |
|  |  +------------------------------------------------------------+  |   |
|  |  +------------------------------------------------------------+  |   |
|  |  | resource_index: HashMap<(name,res), ResourceLocator>       |  |   |
|  |  |  - maps (device_name, resource_name) -> device_index +     |  |   |
|  |  |    local_id for O(1) CLI examine/deposit dispatch          |  |   |
|  |  +------------------------------------------------------------+  |   |
|  +------------------------------------------------------------------+   |
+-------------------------------------------------------------------------+
```

## Trait Hierarchy

```
+-------------------------------------------------------------------+
| trait SimResourceProvider                                         |
|  + get_metadata(&self) -> Vec<ResourceMetadata>                  |
|  + read_resource(&self, id, idx) -> Result<u64, SimError>         |
|  + write_resource(&mut self, id, idx, val) -> Result<(), SimError>|
+-------------------------------------------------------------------+
                              |
                              | supertrait of
                              v
+-------------------------------------------------------------------+
| trait DeviceTraits<CPU: CPUTraits>: Debug + SimResourceProvider   |
|  + device_name(&self) -> &str                                     |
|  + description(&self) -> &str                                     |
|  + device_role(&self) -> DeviceRole                               |
|  + device_service(&mut self, cpu: &mut CPU, bus: &mut SystemBus)  |
|  + device_reset(&mut self)                                        |
|  + execute_io(&mut self, payload: &CPU::IoPayload, cpu: &mut CPU) |
|  + attachment(&self) -> Option<&dyn DeviceAttachmentInterface>    |
+-------------------------------------------------------------------+

+-------------------------------------------------------------------+
| trait CPUTraits: Debug + SimResourceProvider + Sized              |
|  type IoPayload  -- CPU-defined struct for I/O dispatch           |
|  + simulate_instruction(&mut SystemBus, &mut dyn DeviceAccessor)  |
|  + initial_ips_code(&mut self)                                    |
|  + current_pc(&self) -> Option<String>                            |
|  + cpu_reset(&mut self)                                           |
|  + wire_devices(&mut self, &dyn DeviceAccessor)                   |
|  + disassemble(&self, address) -> Option<(String, usize)>         |
+-------------------------------------------------------------------+
```

## Device I/O Dispatch

Device I/O during instruction execution uses a pre-resolved handle mechanism that avoids per-instruction name
lookups.  At startup the CPU calls `wire_devices` to resolve device names to `DeviceHandle` values (plain
`usize` indices into the `devices` Vec).  During execution the CPU builds an `IoPayload` — a CPU-defined
struct carrying only the instruction-encoded I/O parameters (device number, pulse bits, etc.) — and dispatches
directly to the device through the handle.  CPU register values are not copied into the payload; devices read
them directly from the `&mut CPU` parameter that `execute_io` also receives.

See [Device I/O Dispatch](docs/device_wiring.md) for full details, design rationale, step-by-step device
addition guide, and comparison with C SIMH.


```
STARTUP (once, via finalize_hardware):
  cpu.wire_devices(&accessor)
    +-- accessor.resolve_handle("TTI") -> Some(DeviceHandle(0))
    +-- cpu.tti_handle = Some(DeviceHandle(0))

EXECUTION (per IOT instruction):
  cpu.simulate_instruction(&mut bus, &mut accessor)
    +-- payload = PDP8IoPayload { device: 03, pulse: 6 }
    +-- accessor.get_device_mut(self.tti_handle)   -- O(1) Vec index
    +-- tti.execute_io(&payload, &mut cpu)
          +-- reads cpu.acc, writes cpu.acc, sets cpu.int_req
```

## Device Storage

```ignore
SimEnvironment<CPU>
  devices: Vec<DeviceMetadata<CPU>>
      |
      +-- [0] DeviceNode::Standalone(StandaloneDevice { device: Box<dyn DeviceTraits> })
      +-- [1] DeviceNode::Standalone(...)
      +-- [2] DeviceNode::Controller(Controller {
                  controller: Box<dyn DeviceTraits>,
                  units: Vec<UnitMetadata<CPU>>
              })
```

`DeviceHandle(n)` indexes directly into this Vec.  `CPU_DEVICE_ID` (`usize::MAX`) is the sentinel used in
`ResourceLocator` to indicate that a resource belongs to the CPU rather than any registered device.

## Borrow Structure in the Simulation Loop

`simulate_instruction` requires simultaneous mutable access to the CPU, the system bus, and the device list —
three disjoint fields of `SimEnvironment`.  Because the borrow checker cannot verify disjointness through a
`&mut self` method call, `simloop.rs` destructures the environment with explicit `let` bindings before the hot
loop:

```ignore
let SimEnvironment {
    ref mut cpu,
    ref mut bus,
    ref mut devices,
    ..
} = env;

let mut accessor = ActiveDevices(devices);
cpu.simulate_instruction(&mut *bus, &mut accessor)?;
```

`ActiveDevices` is a zero-cost newtype over `&mut Vec<DeviceMetadata<CPU>>` that implements
`DeviceAccessor<CPU>` with `#[inline(always)]` on `get_device_mut`.  The indirection is erased at compile
time.

## Resource Access (CLI Path)

```ignore
CLI Thread: "EXAMINE TTI FLAG"
        |
        | SimRequest::Examine [{ device="TTI", resource="FLAG", ... }]
        v
SimEnvironment::handle_request()
  +-- resource_index.get(("TTI", "FLAG"))
  |     -> ResourceLocator { device_index: 0, local_id: 0 }
  +-- devices[0].read_resource(local_id=0, index=0)
  |     -> Ok(1u64)
  +-- SimResponse::ExamineData([Ok(Values([1]))])
        |
        v
CLI Thread: renders "FLAG     1"
```

## Service Handler Execution

Timer-driven device service uses `device_service`, which receives both `&mut CPU` and `&mut SystemBus` —
giving devices the same direct access to CPU state that `execute_io` provides:

```ignore
simloop: env.service_device("TTI")
  +-- devices[n].device_service(&mut cpu, &mut bus)
        |
        | TTI::device_service():
        |   if !cpu.int_req.contains(ION) { return Ok(()) }
        |   if let Some(ch) = bus.console_read() {
        |       self.buffer = ch as u8 & 0x7F;
        |       self.flag = true;
        |       cpu.int_req.insert(InterruptFlags::TTI);
        |   }
        v
```

## Message Protocol

```ignore
SimRequest                            SimResponse
----------                            -----------
Examine(Vec<ExamineRequest>)          -> ExamineData(Vec<Result<ExamineResult>>)
Deposit { device, resource, values }  -> Ok / Error
Attach { device_name, resource }      -> Ok / Error
Detach { device_name }                -> Ok / Error
LoadFile { flags, path }              -> Ok / Error
Step(n)                               -> Ok  (starts execution)
Stop                                  -> Ok
Reset(Option<String>)                 -> Ok
ScheduleDevice { name, delay }        -> Ok
SetDebugState(...) / DebugDisable     -> Ok
Quit                                  -> (thread exits, no response)
```

## Key Design Principles

1. **Thread separation**: CLI and Simulator threads communicate only via `mpsc` channels.  No shared mutable
   state except `Arc<ConsoleQueues>`.

2. **CPU, bus, and devices are peers**: `SystemBus` does not carry the CPU.  All three are separate fields of
   `SimEnvironment`, making disjoint borrowing explicit and compiler-verified.

3. **O(1) I/O dispatch**: Device handles are resolved once at startup via `wire_devices`.  Instruction
   execution indexes directly into the `devices` Vec — no string lookups, no hash maps, no overhead beyond the
   vtable call.

4. **Typed I/O payload**: `CPU::IoPayload` carries only instruction-encoded parameters.  CPU registers are not
   copied into the payload; devices read and write them directly via `&mut CPU`.

5. **Trait objects for heterogeneous devices**: Devices are stored as `Box<dyn DeviceTraits<CPU> + Send>`,
   allowing different concrete types in the same `Vec` with full type safety at the trait boundary.

6. **Derive-macro resources**: `#[derive(SimResources)]` generates `SimResourceProvider` from
   `#[resource(...)]` field attributes.  Device state structs are the single source of truth for both
   simulation state and CLI visibility.
