// SPDX-License-Identifier: MIT

//! Integration tests for device name uniqueness validation
//!
//! Uses the nullsystem test simulator to verify that device names are properly
//! validated for uniqueness and protected names.

use nullsystem::cpu::NullProcessor;
use nullsystem::devices::{DevOne, DevZero, NullInput};
use sim_core::env::{SimEnvironment, SimError};

#[test]
fn test_add_devices_with_unique_names() {
    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    // Should all succeed - unique names
    env.add_standalone(Box::new(DevZero::new()));
    env.add_standalone(Box::new(DevOne::new()));
    env.add_standalone(Box::new(NullInput::new()));

    // Verify all devices are registered
    assert!(env.has_device("DevZero"));
    assert!(env.has_device("DevOne"));
    assert!(env.has_device("NULL_INPUT"));
}

#[test]
#[should_panic(expected = "DEVZERO")]
fn test_duplicate_device_names_panics() {
    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    // Add first device
    env.add_standalone(Box::new(DevZero::new()));

    // Attempt to add second device with same name - should panic
    env.add_standalone(Box::new(DevZero::new()));
}

#[test]
#[should_panic(expected = "CPU (reserved)")]
fn test_cpu_name_is_reserved() {
    use sim_core::env::{DeviceTraits, ResourceMetadata, SimResourceProvider, SystemBus};

    // Create a device that tries to use the "CPU" name
    #[derive(Debug)]
    struct BadDevice;

    impl DeviceTraits<NullProcessor> for BadDevice {
        fn device_name(&self) -> &str {
            "CPU"
        }

        fn description(&self) -> &str {
            "A bad device trying to use CPU name"
        }

        fn device_service(&mut self, _cpu: &mut NullProcessor, _bus: &mut SystemBus) -> Result<(), SimError> {
            Ok(())
        }

        fn device_reset(&mut self) -> () {
            // NOP
        }
    }

    impl SimResourceProvider for BadDevice {
        fn get_metadata(&self) -> Vec<ResourceMetadata> {
            // No metadata exported.
            Vec::new()
        }

        fn read_resource(&self, res_id: u32, index: usize) -> Result<u64, SimError> {
            // And no resources to read.
            Err(SimError::NoSuchResource(format!(
                "Resource ID {} index {}",
                res_id, index
            )))
        }

        fn write_resource(&mut self, res_id: u32, index: usize, _val: u64) -> Result<(), SimError> {
            // And can't deposit anything into it, either.
            Err(SimError::NoSuchResource(format!(
                "Resource ID {} index {}",
                res_id, index
            )))
        }
    }

    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    // Should panic - "CPU" is reserved
    env.add_standalone(Box::new(BadDevice));
}

#[test]
fn test_case_insensitive_name_collision() {
    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    env.add_standalone(Box::new(DevZero::new()));

    // All these variations should be detected as duplicates
    assert!(env.check_name("DevZero").is_err());
    assert!(env.check_name("devzero").is_err());
    assert!(env.check_name("DEVZERO").is_err());
    assert!(env.check_name("DeVzErO").is_err());
}

#[test]
fn test_check_device_name_before_creation() {
    let cpu = NullProcessor::new();
    let env = SimEnvironment::new(cpu);

    // Should succeed - names are available
    assert!(env.check_name("DevZero").is_ok());
    assert!(env.check_name("DevOne").is_ok());
    assert!(env.check_name("NULL_INPUT").is_ok());
}

#[test]
fn test_check_name_after_creation() {
    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    // Add a device
    env.add_standalone(Box::new(DevZero::new()));

    // Now the name should be taken
    assert!(env.check_name("DevZero").is_err());

    // But other names are still available
    assert!(env.check_name("DevOne").is_ok());
    assert!(env.check_name("NULL_INPUT").is_ok());
}

#[test]
fn test_check_cpu_name_is_reserved() {
    let cpu = NullProcessor::new();
    let env = SimEnvironment::new(cpu);

    // CPU name should always be reserved
    let result = env.check_name("CPU");
    assert!(result.is_err());

    if let Err(SimError::DuplicateDevice(msg)) = result {
        assert!(msg.contains("reserved"));
    } else {
        panic!("Expected DuplicateDevice error for reserved CPU name");
    }
}

#[test]
fn test_batch_validation_pattern() {
    let cpu = NullProcessor::new();
    let env = SimEnvironment::new(cpu);

    // Simulate creating multiple units with validation
    let unit_names = vec!["DISK0", "DISK1", "DISK2", "DISK3"];

    // Validate all names first
    for name in &unit_names {
        env.check_name(name)
            .expect(&format!("Name {} should be available", name));
    }

    // All validated - now we could safely create devices
    // (In a real scenario, we'd create actual units here)
}

#[test]
fn test_device_names_list() {
    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    env.add_standalone(Box::new(DevZero::new()));
    env.add_standalone(Box::new(DevOne::new()));

    let names = env.device_names();

    // Should include CPU and the two devices
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"CPU".to_string()));
    assert!(names.contains(&"DEVZERO".to_string()));
    assert!(names.contains(&"DEVONE".to_string()));
}

#[test]
fn test_has_device_checks() {
    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    // CPU should always exist in the manifest, but it's not in the devices table.
    assert!(!env.has_device("CPU"));

    // Other devices don't exist yet
    assert!(!env.has_device("DevZero"));

    // Add a device
    env.add_standalone(Box::new(DevZero::new()));

    // Now it exists (case-insensitive)
    assert!(env.has_device("DevZero"));
    assert!(env.has_device("devzero"));
    assert!(env.has_device("DEVZERO"));
}

#[test]
fn test_error_message_format() {
    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    env.add_standalone(Box::new(DevZero::new()));

    // Check the error format for duplicate
    let result = env.check_name("DevZero");
    assert!(result.is_err());

    if let Err(SimError::DuplicateDevice(name)) = result {
        assert_eq!(name, "DEVZERO"); // Should be uppercase
    } else {
        panic!("Expected DuplicateDevice error");
    }

    // Check the error format for reserved name
    let result = env.check_name("CPU");
    assert!(result.is_err());

    if let Err(SimError::DuplicateDevice(msg)) = result {
        assert!(msg.contains("CPU"));
        assert!(msg.contains("reserved"));
    } else {
        panic!("Expected DuplicateDevice error with reserved message");
    }
}

#[test]
fn test_multiple_devices_of_same_type() {
    use sim_core::{
        env::{DeviceTraits, SystemBus},
        SimResources,
    };

    // Create a device type that can have different names
    #[derive(Debug, SimResources)]
    struct Unit {
        name: String,
        #[resource(name = "VALUE", bits = 32)]
        value: u32,
    }

    impl Unit {
        fn new(name: String) -> Self {
            Self { name, value: 0 }
        }
    }

    impl DeviceTraits<NullProcessor> for Unit {
        fn device_name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "Test unit"
        }
        fn device_service(&mut self, _cpu: &mut NullProcessor, _bus: &mut SystemBus) -> Result<(), SimError> {
            Ok(())
        }
        fn device_reset(&mut self) -> () {
            // NOP
        }
    }

    let cpu = NullProcessor::new();
    let mut env = SimEnvironment::new(cpu);

    // Should be able to add multiple units with different names
    env.add_standalone(Box::new(Unit::new("UNIT0".to_string())));
    env.add_standalone(Box::new(Unit::new("UNIT1".to_string())));
    env.add_standalone(Box::new(Unit::new("UNIT2".to_string())));

    assert!(env.has_device("UNIT0"));
    assert!(env.has_device("UNIT1"));
    assert!(env.has_device("UNIT2"));
}
