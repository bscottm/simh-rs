# Device Name Uniqueness Validation

## Overview

The simulator ensures all device names are unique in the global namespace through automatic validation at registration time.

## Features

### 1. Automatic Validation in `add_device()`

Every device added through `add_device()` is automatically checked:

```rust
// Validates automatically
env.add_device(Box::new(RKController::new()));     // "RKA" - OK
env.add_device(Box::new(RKUnit::new("RKA0", 0)));  // "RKA0" - OK
env.add_device(Box::new(RKUnit::new("RKA0", 1)));  // PANIC: duplicate "RKA0"
```

**Protected Names:**
- `"CPU"` is reserved for the CPU device
- All device names are case-insensitive (converted to uppercase)

### 2. Pre-Validation with `check_device_name()`

For careful code that wants to validate before creating devices:

```rust
// Check if a name is available
if let Err(e) = env.check_device_name("RKA0") {
    println!("Name not available: {}", e);
    return;
}

// Safe to create
env.add_device(Box::new(RKUnit::new("RKA0", 0)));
```

**Use cases:**
- Creating many devices in a loop
- Reading device names from configuration
- Want graceful error handling instead of panics

### 3. Batch Validation Pattern

For controllers creating multiple units:

```rust
// Validate all names upfront
let controller_name = "RKA";
env.check_device_name(controller_name)?;

for i in 0..4 {
    env.check_device_name(&format!("{}{}", controller_name, i))?;
}

// All names valid - now create them
env.add_device(Box::new(RKController::new()));
for i in 0..4 {
    env.add_device(Box::new(RKUnit::new(format!("RKA{}", i), i as u8)));
}
```

## Implementation Details

### In `add_device_internal()`

```rust
fn add_device_internal(&mut self, device: Box<dyn DeviceTraits + Send>) 
    -> Result<&mut Self, SimError> 
{
    let device_name = device.device_name().to_ascii_uppercase();
    
    // Check for reserved CPU name
    if device_name == CPU_DEVICE_NAME {
        return Err(SimError::DuplicateDevice(
            format!("{} (reserved for CPU)", device_name)
        ));
    }
    
    // Check for existing device with same name
    if let Entry::Vacant(entry) = self.devices.entry(device_name.clone()) {
        entry.insert(device);
        Ok(self)
    } else {
        Err(SimError::DuplicateDevice(device_name))
    }
}
```

### In `check_device_name()`

```rust
pub fn check_device_name(&self, name: &str) -> Result<(), SimError> {
    let name_upper = name.to_ascii_uppercase();
    
    if name_upper == CPU_DEVICE_NAME {
        return Err(SimError::DuplicateDevice(
            format!("{} (reserved for CPU)", name_upper)
        ));
    }
    
    if self.devices.contains_key(&name_upper) {
        return Err(SimError::DuplicateDevice(name_upper));
    }
    
    Ok(())
}
```

## Error Types

### `SimError::DuplicateDevice`

Returned when a device name conflicts:

```rust
// Example error messages:
SimError::DuplicateDevice("RKA0")              // Duplicate device
SimError::DuplicateDevice("CPU (reserved for CPU)")  // Reserved name
```

## Design Decisions

### Why Panic in `add_device()`?

Duplicate device names during initialization are **programmer errors**, not runtime errors:
- Configuration is typically static and known at compile time
- Duplicates indicate a bug in the initialization code
- Fail-fast behavior makes bugs obvious during development

### Why Provide `check_device_name()`?

Some scenarios benefit from graceful error handling:
- Dynamic device creation from configuration files
- Complex initialization logic with conditional devices
- Want to validate all names before allocating resources
- Need to provide user-friendly error messages

### Case Insensitivity

Device names are case-insensitive for CLI convenience:
- User can type `examine rka0 cyl` or `EXAMINE RKA0 CYL`
- Internally normalized to uppercase: `"RKA0"`
- Prevents case-variant duplicates: `"RKA0"` == `"rka0"` == `"Rka0"`

## Best Practices

### For Simple Simulators

Just use `add_device()` directly:

```rust
fn initialize_devices(env: &mut SimEnvironment<MyCPU>) {
    env.add_device(Box::new(RKController::new()));
    env.add_device(Box::new(RKUnit::new("RKA0", 0)));
    env.add_device(Box::new(RKUnit::new("RKA1", 1)));
    // Panics immediately on duplicate - easy to debug
}
```

### For Complex Simulators

Use `check_device_name()` for validation:

```rust
fn initialize_from_config(env: &mut SimEnvironment<MyCPU>, config: &Config) 
    -> Result<(), SimError> 
{
    // Validate all names first
    for device_config in &config.devices {
        env.check_device_name(&device_config.name)?;
    }
    
    // All names valid - now create devices
    for device_config in &config.devices {
        let device = create_device_from_config(device_config)?;
        env.add_device(device);
    }
    
    Ok(())
}
```

### For Controllers with Units

Use helper methods:

```rust
impl RKController {
    pub fn create_with_units(name: &str, num_units: usize) 
        -> (Self, Vec<Box<dyn DeviceTraits>>) 
    {
        let controller = RKController::new();
        let units = (0..num_units)
            .map(|i| Box::new(RKUnit::new(format!("{}{}", name, i), i as u8)) 
                as Box<dyn DeviceTraits>)
            .collect();
        
        (controller, units)
    }
}

// Usage
let (controller, units) = RKController::create_with_units("RKA", 4);
env.add_device(Box::new(controller));
for unit in units {
    env.add_device(unit);
}
```

## Testing

### Test Duplicate Detection

```rust
#[test]
#[should_panic(expected = "Duplicate")]
fn test_duplicate_device_names() {
    let mut env = SimEnvironment::new(MyCPU::new());
    
    env.add_device(Box::new(RKUnit::new("RKA0", 0)));
    env.add_device(Box::new(RKUnit::new("RKA0", 1))); // Should panic
}
```

### Test CPU Name Protection

```rust
#[test]
#[should_panic(expected = "reserved for CPU")]
fn test_cpu_name_reserved() {
    let mut env = SimEnvironment::new(MyCPU::new());
    
    env.add_device(Box::new(BadDevice::new("CPU"))); // Should panic
}
```

### Test Case Insensitivity

```rust
#[test]
fn test_case_insensitive_names() {
    let mut env = SimEnvironment::new(MyCPU::new());
    
    env.add_device(Box::new(RKUnit::new("RKA0", 0)));
    
    // These should all conflict
    assert!(env.check_device_name("RKA0").is_err());
    assert!(env.check_device_name("rka0").is_err());
    assert!(env.check_device_name("Rka0").is_err());
}
```
