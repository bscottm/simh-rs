#[cfg(test)]
mod tests {
    use crate::cpu::{EAEMode, PDP8IoPayload, PDP8Processor, LINK_MASK, VALUE_MASK};
    use crate::pdp8_defs::InterruptFlags;
    use sim_core::env::{
        CPUTraits, DeviceAccessor, DeviceHandle, DeviceTraits, SimError, SimResourceProvider, SystemBus,
    };
    use sim_core::timers::{create_platform_timer, PlatformTimer, TimerManager};

    /// Helper to create a test CPU
    fn make_cpu() -> PDP8Processor {
        PDP8Processor::new()
    }
    struct MockDeviceAccessor;

    impl DeviceAccessor<PDP8Processor> for MockDeviceAccessor {
        fn resolve_handle(&self, _name: &str) -> Option<DeviceHandle> {
            None
        }

        fn get_device_mut(
            &mut self,
            _handle: DeviceHandle,
        ) -> Option<&mut (dyn DeviceTraits<PDP8Processor> + Send)> {
            None
        }
    }

    /// Helper to set up memory and execute instructions
    fn setup_and_run(cpu: &mut PDP8Processor, setup: &[(u16, u16)], steps: usize) {
        let timer_mgr = TimerManager::new(create_platform_timer());
        let mut sysbus = SystemBus::new(timer_mgr);

        for &(addr, val) in setup {
            cpu.memory[addr as usize] = val;
        }

        for _ in 0..steps {
            if cpu.should_interrupt() {
                cpu.handle_interrupt();
            }
            let ir = cpu.read(cpu.pc);
            cpu.bump_pc();
            cpu.int_req.insert(InterruptFlags::NO_ION_PENDING);
            if cpu
                .execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
                .is_err()
            {
                break;
            }
        }
    }

    /*=========================================================================
     * Interrupt Delay Tests
     *=======================================================================*/

    #[test]
    fn test_ion_one_cycle_delay() {
        let mut cpu = make_cpu();
        let timer_mgr = TimerManager::new(create_platform_timer());
        let mut sysbus = SystemBus::new(timer_mgr);

        // Setup memory
        cpu.memory[0o200] = 0o6001; // ION
        cpu.memory[0o201] = 0o7000; // NOP
        cpu.memory[0o202] = 0o7402; // HLT
        cpu.memory[0o000] = 0o7402; // HLT (interrupt handler)

        cpu.pc = 0o200;

        // Set up interrupts
        cpu.int_enable = InterruptFlags::TEST_DEVICE;
        cpu.int_req =
            InterruptFlags::TEST_DEVICE | InterruptFlags::NO_CIF_PENDING | InterruptFlags::NO_ION_PENDING;

        // Execute ION
        let ir = cpu.read(cpu.pc);
        assert_eq!(ir, 0o6001);
        cpu.bump_pc();
        cpu.int_req.insert(InterruptFlags::NO_ION_PENDING);
        cpu.execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
            .unwrap();

        // After ION: ION flag set, NO_ION_PENDING cleared
        assert!(cpu.int_req.contains(InterruptFlags::ION));
        assert!(!cpu.int_req.contains(InterruptFlags::NO_ION_PENDING));
        assert!(cpu.int_req.contains(InterruptFlags::NO_CIF_PENDING));
        assert_eq!(cpu.pc, 0o201);

        // Check: Interrupt should NOT occur yet
        assert!(
            !cpu.should_interrupt(),
            "Interrupt should not occur during ION delay. int_req = 0x{:08x}",
            cpu.int_req.bits()
        );

        // Execute NOP (the delay instruction)
        let ir = cpu.read(cpu.pc);
        assert_eq!(ir, 0o7000);
        cpu.bump_pc();
        cpu.int_req.insert(InterruptFlags::NO_ION_PENDING); // Clear delay
        cpu.execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
            .unwrap();

        // After NOP: All three flags should be set
        assert!(
            cpu.int_req.contains(InterruptFlags::ION),
            "ION should be set. int_req = 0x{:08x}",
            cpu.int_req.bits()
        );
        assert!(
            cpu.int_req.contains(InterruptFlags::NO_ION_PENDING),
            "NO_ION_PENDING should be set. int_req = 0x{:08x}",
            cpu.int_req.bits()
        );
        assert!(
            cpu.int_req.contains(InterruptFlags::NO_CIF_PENDING),
            "NO_CIF_PENDING should be set. int_req = 0x{:08x}",
            cpu.int_req.bits()
        );
        assert_eq!(cpu.pc, 0o202);

        // Check: Interrupt SHOULD occur now
        assert!(
            cpu.should_interrupt(),
            "Interrupt should occur after delay. int_req = 0x{:08x}, int_enable = 0x{:08x}",
            cpu.int_req.bits(),
            cpu.int_enable.bits()
        );

        // Handle the interrupt
        cpu.handle_interrupt();

        // Verify interrupt was taken
        assert_eq!(cpu.pc, 1);
        assert_eq!(cpu.memory[0], 0o202);
        assert!(!cpu.int_req.contains(InterruptFlags::ION));
    }

    #[test]
    fn test_cif_one_cycle_delay() {
        let mut cpu = make_cpu();
        let timer_mgr = TimerManager::new(create_platform_timer());
        let mut sysbus = SystemBus::new(timer_mgr);

        // Program in field 0
        cpu.memory[0o200] = 0o6212; // CIF 1 (change to field 1)
        cpu.memory[0o201] = 0o7000; // NOP (delay instruction)
        cpu.memory[0o202] = 0o5010; // JMP 0010 (PAGE ZERO!)
        cpu.memory[0o10010] = 0o7402; // HLT in field 1

        cpu.pc = 0o200;
        cpu.ib = 0o00000; // Start with IB = field 0

        // Execute CIF 1
        let ir = cpu.read(cpu.pc);
        cpu.bump_pc();
        cpu.int_req.insert(InterruptFlags::NO_ION_PENDING);
        cpu.execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
            .unwrap();

        // After CIF: IB = field 1, NO_CIF_PENDING cleared
        assert_eq!(cpu.ib, 0o10000, "IB should be field 1");
        assert!(!cpu.int_req.contains(InterruptFlags::NO_CIF_PENDING));
        assert_eq!(cpu.pc, 0o201);

        // Execute NOP (delay instruction)
        let ir = cpu.read(cpu.pc);
        cpu.bump_pc();
        cpu.int_req.insert(InterruptFlags::NO_ION_PENDING);
        cpu.execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
            .unwrap();

        // After NOP: Still in field 0 (CIF hasn't taken effect yet)
        assert_eq!(cpu.pc, 0o202, "PC still in field 0");

        // Execute JMP (this commits the field change)
        let ir = cpu.read(cpu.pc);
        cpu.bump_pc();
        cpu.int_req.insert(InterruptFlags::NO_ION_PENDING);
        cpu.execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
            .unwrap();

        // After JMP: Now in field 1, address 0010
        assert_eq!(cpu.pc, 0o10010, "PC should be in field 1, address 0010");
        assert!(
            cpu.int_req.contains(InterruptFlags::NO_CIF_PENDING),
            "NO_CIF_PENDING should be set after commit"
        );
    }
    #[test]
    fn test_iof_immediate() {
        let mut cpu = make_cpu();
        let timer_mgr = TimerManager::new(create_platform_timer());
        let mut sysbus = SystemBus::new(timer_mgr);

        // Setup: ION, IOF, NOP
        cpu.memory[0o200] = 0o6001; // ION
        cpu.memory[0o201] = 0o6002; // IOF
        cpu.memory[0o202] = 0o7000; // NOP

        cpu.pc = 0o200;
        cpu.int_enable = InterruptFlags::all();
        cpu.int_req = InterruptFlags::TEST_DEVICE;

        // Execute ION
        setup_and_run(&mut cpu, &[], 1);
        assert!(cpu.int_req.contains(InterruptFlags::ION));

        // Execute IOF - should disable immediately
        let ir = cpu.read(cpu.pc);
        cpu.bump_pc();
        cpu.int_req.insert(InterruptFlags::NO_ION_PENDING);
        cpu.execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
            .unwrap();

        // ION should be cleared immediately
        assert!(!cpu.int_req.contains(InterruptFlags::ION));
        assert!(!cpu.should_interrupt());
    }

    /*=========================================================================
     * AND Instruction Tests
     *=======================================================================*/

    #[test]
    fn test_and_direct_page_zero() {
        let mut cpu = make_cpu();
        cpu.acc = 0o7777;
        cpu.memory[0o100] = 0o3456;
        cpu.memory[0o200] = 0o0100; // AND 0100 (direct, page zero)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o3456);
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_and_direct_current_page() {
        let mut cpu = make_cpu();
        cpu.acc = 0o7777;
        cpu.memory[0o250] = 0o1234;
        cpu.memory[0o200] = 0o0250; // AND 0250 (direct, current page)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o1234);
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_and_indirect_page_zero() {
        let mut cpu = make_cpu();
        cpu.acc = 0o7777;
        cpu.memory[0o100] = 0o0456; // Pointer to 0456
        cpu.memory[0o456] = 0o0707; // Data
        cpu.memory[0o200] = 0o0500; // AND I 0100 (indirect, page zero)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o0707);
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_and_indirect_autoincrement() {
        let mut cpu = make_cpu();
        cpu.acc = 0o7777;
        cpu.memory[0o010] = 0o0100; // Auto-increment location
        cpu.memory[0o101] = 0o5252; // Data at incremented address
        cpu.memory[0o200] = 0o0410; // AND I 0010
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o010], 0o0101); // Pointer incremented
        assert_eq!(cpu.acc & VALUE_MASK, 0o5252);
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_and_preserves_link() {
        let mut cpu = make_cpu();
        cpu.acc = 0o17777; // Link set
        cpu.memory[0o100] = 0o0000;
        cpu.memory[0o200] = 0o0100; // AND 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc, 0o10000); // AC cleared, link preserved
    }

    #[test]
    fn test_and_cross_field() {
        let mut cpu = make_cpu();
        cpu.acc = 0o7777;
        cpu.df = 0o10000; // Data field 1
        cpu.memory[0o100] = 0o0200; // Pointer to address 0200
        cpu.memory[0o10200] = 0o1357; // Data in field 1
        cpu.memory[0o200] = 0o0500; // AND I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o1357);
    }

    /*=========================================================================
     * TAD Instruction Tests
     *=======================================================================*/

    #[test]
    fn test_tad_simple_add() {
        let mut cpu = make_cpu();
        cpu.acc = 0o1234;
        cpu.memory[0o100] = 0o0543;
        cpu.memory[0o200] = 0o1100; // TAD 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc, 0o1777);
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_tad_with_link_carry() {
        let mut cpu = make_cpu();
        cpu.acc = 0o7777;
        cpu.memory[0o100] = 0o0001;
        cpu.memory[0o200] = 0o1100; // TAD 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc, 0o10000); // Link set, AC = 0
    }

    #[test]
    fn test_tad_link_to_link_carry() {
        let mut cpu = make_cpu();
        cpu.acc = 0o17777; // Link set, AC = 7777
        cpu.memory[0o100] = 0o0001;
        cpu.memory[0o200] = 0o1100; // TAD 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc, 0o00000); // Link cleared (carried out)
    }

    #[test]
    fn test_tad_negative_numbers() {
        let mut cpu = make_cpu();
        cpu.acc = 0o0005;
        cpu.memory[0o100] = 0o7773; // -5 in 12-bit two's complement
        cpu.memory[0o200] = 0o1100; // TAD 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc, 0o10000); // Result is 0 with link set
    }

    #[test]
    fn test_tad_indirect() {
        let mut cpu = make_cpu();
        cpu.acc = 0o1000;
        cpu.memory[0o100] = 0o0456;
        cpu.memory[0o456] = 0o2000;
        cpu.memory[0o200] = 0o1500; // TAD I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc, 0o3000);
    }

    /*=========================================================================
     * ISZ Instruction Tests
     *=======================================================================*/

    #[test]
    fn test_isz_no_skip() {
        let mut cpu = make_cpu();
        cpu.memory[0o100] = 0o1234;
        cpu.memory[0o200] = 0o2100; // ISZ 0100
        cpu.memory[0o201] = 0o7402; // HLT
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o100], 0o1235);
        assert_eq!(cpu.pc, 0o201); // No skip
    }

    #[test]
    fn test_isz_skip_on_zero() {
        let mut cpu = make_cpu();
        cpu.memory[0o100] = 0o7777; // -1, will become 0
        cpu.memory[0o200] = 0o2100; // ISZ 0100
        cpu.memory[0o201] = 0o7402; // HLT (skipped)
        cpu.memory[0o202] = 0o7000; // NOP
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o100], 0o0000);
        assert_eq!(cpu.pc, 0o202); // Skipped
    }

    #[test]
    fn test_isz_wrap_around() {
        let mut cpu = make_cpu();
        cpu.memory[0o100] = 0o7777;
        cpu.memory[0o200] = 0o2100; // ISZ 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o100], 0o0000); // Wrapped to 0
    }

    #[test]
    fn test_isz_indirect() {
        let mut cpu = make_cpu();
        cpu.memory[0o100] = 0o0456;
        cpu.memory[0o456] = 0o7777;
        cpu.memory[0o200] = 0o2500; // ISZ I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o456], 0o0000);
        assert_eq!(cpu.pc, 0o202); // Skip
    }

    #[test]
    fn test_isz_cross_field() {
        let mut cpu = make_cpu();
        cpu.df = 0o10000; // Data field 1
        cpu.memory[0o100] = 0o0200;
        cpu.memory[0o10200] = 0o7777;
        cpu.memory[0o200] = 0o2500; // ISZ I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o10200], 0o0000);
        assert_eq!(cpu.pc, 0o202);
    }

    /*=========================================================================
     * DCA Instruction Tests
     *=======================================================================*/

    #[test]
    fn test_dca_direct() {
        let mut cpu = make_cpu();
        cpu.acc = 0o13456; // Link set
        cpu.memory[0o200] = 0o3100; // DCA 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o100], 0o3456); // Only AC stored
        assert_eq!(cpu.acc, 0o10000); // AC cleared, link preserved
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_dca_clears_ac_preserves_link() {
        let mut cpu = make_cpu();
        cpu.acc = 0o17777;
        cpu.memory[0o200] = 0o3100; // DCA 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o100], 0o7777);
        assert_eq!(cpu.acc, 0o10000); // Link preserved
    }

    #[test]
    fn test_dca_indirect() {
        let mut cpu = make_cpu();
        cpu.acc = 0o5555;
        cpu.memory[0o100] = 0o0456;
        cpu.memory[0o200] = 0o3500; // DCA I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o456], 0o5555);
        assert_eq!(cpu.acc, 0o0000);
    }

    #[test]
    fn test_dca_cross_field() {
        let mut cpu = make_cpu();
        cpu.acc = 0o1234;
        cpu.df = 0o10000;
        cpu.memory[0o100] = 0o0200;
        cpu.memory[0o200] = 0o3500; // DCA I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o10200], 0o1234);
    }

    /*=========================================================================
     * JMS Instruction Tests
     *=======================================================================*/

    #[test]
    fn test_jms_direct_page_zero() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o4100; // JMS 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o100], 0o201); // Return address
        assert_eq!(cpu.pc, 0o101); // Jump to 0100 + 1
    }

    #[test]
    fn test_jms_direct_current_page() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o4250; // JMS 0250
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o250], 0o201);
        assert_eq!(cpu.pc, 0o251);
    }

    #[test]
    fn test_jms_indirect() {
        let mut cpu = make_cpu();
        cpu.memory[0o100] = 0o0456;
        cpu.memory[0o200] = 0o4500; // JMS I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o456], 0o201);
        assert_eq!(cpu.pc, 0o457);
    }

    #[test]
    fn test_jms_with_cif() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o6212; // CIF 1
        cpu.memory[0o201] = 0o4100; // JMS 0100
        cpu.pc = 0o200;
        cpu.ib = 0o00000;

        // Execute CIF
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.ib, 0o10000);

        // Execute JMS - should write to field 1
        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o10100], 0o202); // Written to field 1
        assert_eq!(cpu.pc, 0o10101); // PC in field 1
    }

    #[test]
    fn test_jms_autoincrement() {
        let mut cpu = make_cpu();
        cpu.memory[0o010] = 0o0100;
        cpu.memory[0o200] = 0o4410; // JMS I 0010
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o010], 0o0101); // Auto-incremented
        assert_eq!(cpu.memory[0o101], 0o201); // Return address
        assert_eq!(cpu.pc, 0o102);
    }

    #[test]
    fn test_jms_updates_pcq() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o4100; // JMS 0100
        cpu.pc = 0o200;
        let old_pcq_idx = cpu.pcq_idx;

        setup_and_run(&mut cpu, &[], 1);

        // PCQ should have been updated
        assert_ne!(cpu.pcq_idx, old_pcq_idx);
        assert_eq!(cpu.pcq[cpu.pcq_idx], 0o201);
    }

    #[test]
    fn test_jms_user_mode_same_field() {
        let mut cpu = make_cpu();
        cpu.uf = true;
        cpu.ub = false;
        cpu.memory[0o200] = 0o4100; // JMS 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // Should execute normally (same field)
        assert_eq!(cpu.memory[0o100], 0o201);
        assert_eq!(cpu.pc, 0o101);
        assert_eq!(cpu.uf, false); // UF updated from UB
    }

    #[test]
    fn test_jms_tsc_trap() {
        let mut cpu = make_cpu();
        cpu.uf = true;
        cpu.tsc_enab = true;
        cpu.memory[0o200] = 0o4100; // JMS 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // Should trap instead of normal JMS
        assert_eq!(cpu.memory[0o100], 0o0000); // NOT written
        assert_eq!(cpu.pc, 0o101); // PC still updated
        assert_eq!(cpu.tsc_pc, 0o200); // Saved PC
        assert_eq!(cpu.tsc_ir, 0o4100); // Saved IR
        assert!(cpu.int_req.contains(InterruptFlags::TSC));
    }

    /*=========================================================================
     * JMP Instruction Tests
     *=======================================================================*/

    #[test]
    fn test_jmp_direct_page_zero() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o5100; // JMP 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.pc, 0o100);
    }

    #[test]
    fn test_jmp_direct_current_page() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o5250; // JMP 0250
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.pc, 0o250);
    }

    #[test]
    fn test_jmp_indirect() {
        let mut cpu = make_cpu();
        cpu.memory[0o100] = 0o0456;
        cpu.memory[0o200] = 0o5500; // JMP I 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.pc, 0o456);
    }

    #[test]
    fn test_jmp_with_cif() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o6212; // CIF 1
        cpu.memory[0o201] = 0o5100; // JMP 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 2);

        assert_eq!(cpu.pc, 0o10100); // Should be in field 1
    }

    #[test]
    fn test_jmp_autoincrement() {
        let mut cpu = make_cpu();
        cpu.memory[0o010] = 0o0100;
        cpu.memory[0o200] = 0o5410; // JMP I 0010
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o010], 0o0101);
        assert_eq!(cpu.pc, 0o101);
    }

    #[test]
    fn test_jmp_updates_pcq() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o5100; // JMP 0100
        cpu.pc = 0o200;
        let old_pcq_idx = cpu.pcq_idx;

        setup_and_run(&mut cpu, &[], 1);

        assert_ne!(cpu.pcq_idx, old_pcq_idx);
        assert_eq!(cpu.pcq[cpu.pcq_idx], 0o201);
    }

    #[test]
    fn test_jmp_clears_cif_delay() {
        let mut cpu = make_cpu();
        cpu.memory[0o200] = 0o6212; // CIF 1
        cpu.memory[0o201] = 0o5100; // JMP 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);
        assert!(!cpu.int_req.contains(InterruptFlags::NO_CIF_PENDING));

        setup_and_run(&mut cpu, &[], 1);
        assert!(cpu.int_req.contains(InterruptFlags::NO_CIF_PENDING));
    }

    #[test]
    fn test_jmp_user_mode() {
        let mut cpu = make_cpu();
        cpu.uf = true;
        cpu.ub = false;
        cpu.memory[0o200] = 0o5100; // JMP 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.pc, 0o100);
        assert_eq!(cpu.uf, false); // Updated from UB
    }

    #[test]
    fn test_jmp_tsc_trap() {
        let mut cpu = make_cpu();
        cpu.uf = true;
        cpu.tsc_enab = true;
        cpu.memory[0o200] = 0o5100; // JMP 0100
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.pc, 0o100); // Still jumps
        assert_eq!(cpu.tsc_pc, 0o200);
        assert_eq!(cpu.tsc_ir, 0o5100);
        assert!(cpu.int_req.contains(InterruptFlags::TSC));
    }

    /*=========================================================================
     * Combined Instruction Tests
     *=======================================================================*/

    #[test]
    fn test_subroutine_call_and_return() {
        let mut cpu = make_cpu();

        // Main program
        cpu.memory[0o200] = 0o4300; // JMS 0300 (call subroutine)
        cpu.memory[0o201] = 0o7402; // HLT

        // Subroutine
        cpu.memory[0o300] = 0o0000; // Return address (will be filled)
        cpu.memory[0o301] = 0o1100; // TAD 0100
        cpu.memory[0o302] = 0o5700; // JMP I 0300 (return)

        // Data
        cpu.memory[0o100] = 0o1234;

        cpu.pc = 0o200;
        cpu.acc = 0o0000;

        // Execute JMS
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o301);
        assert_eq!(cpu.memory[0o300], 0o201);

        // Execute TAD
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.acc, 0o1234);

        // Execute JMP I (return)
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_field_crossing_operations() {
        let mut cpu = make_cpu();

        // Program in field 0
        cpu.memory[0o200] = 0o6211; // CDF 1
        cpu.memory[0o201] = 0o1500; // TAD I 0100 (INDIRECT - reads pointer from IF, data from DF)
        cpu.memory[0o202] = 0o7402; // HLT

        // Pointer in field 0
        cpu.memory[0o100] = 0o0200; // Points to address 0200

        // Data in field 1
        cpu.memory[0o10200] = 0o5555; // Data at field 1, address 0200

        cpu.pc = 0o200;
        cpu.acc = 0o0000;

        setup_and_run(&mut cpu, &[], 2);

        assert_eq!(cpu.acc, 0o5555);
    }

    #[test]
    fn test_field_crossing_operations_direct() {
        let mut cpu = make_cpu();

        // Program in field 0
        cpu.memory[0o200] = 0o1100; // TAD 0100 (direct - reads from IF)
        cpu.memory[0o201] = 0o7402; // HLT

        // Data in field 0 (same as IF)
        cpu.memory[0o100] = 0o5555;

        cpu.pc = 0o200;
        cpu.acc = 0o0000;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc, 0o5555);
    }

    #[test]
    fn test_loop_with_isz() {
        let mut cpu = make_cpu();
        let timer_mgr = TimerManager::new(create_platform_timer());
        let mut sysbus = SystemBus::new(timer_mgr);

        // Loop counter
        cpu.memory[0o100] = 0o7775; // -3

        // Program: loop 3 times
        cpu.memory[0o200] = 0o1101; // TAD 0101
        cpu.memory[0o201] = 0o2100; // ISZ 0100
        cpu.memory[0o202] = 0o5200; // JMP 0200
        cpu.memory[0o203] = 0o7402; // HLT

        // Data to add
        cpu.memory[0o101] = 0o0001;

        cpu.pc = 0o200;
        cpu.acc = 0o0000;

        // Run until halt
        for _ in 0..20 {
            if cpu.pc == 0o203 {
                break;
            }
            let ir = cpu.read(cpu.pc);
            cpu.bump_pc();
            cpu.int_req.insert(InterruptFlags::NO_ION_PENDING);
            if cpu
                .execute_instruction(ir, &mut sysbus, &mut MockDeviceAccessor)
                .is_err()
            {
                break;
            }
        }

        assert_eq!(cpu.acc, 0o0003); // Added 1 three times
        assert_eq!(cpu.memory[0o100], 0o0000); // Counter reached zero
        assert_eq!(cpu.pc, 0o203); // At HLT
    }

    #[test]
    fn test_nested_subroutines() {
        let mut cpu = make_cpu();

        // Main
        cpu.memory[0o200] = 0o4240; // JMS 0240 (current page)
        cpu.memory[0o201] = 0o7402; // HLT

        // Subroutine 1 (moved to 0240, within reach)
        cpu.memory[0o240] = 0o0000;
        cpu.memory[0o241] = 0o4050; // JMS 0050 (page zero)
        cpu.memory[0o242] = 0o5640; // JMP I 0240

        // Subroutine 2 (moved to 0050, page zero)
        cpu.memory[0o050] = 0o0000;
        cpu.memory[0o051] = 0o7200; // CLA
        cpu.memory[0o052] = 0o5450; // JMP I 0050

        cpu.pc = 0o200;

        // Execute main JMS
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o241);
        assert_eq!(cpu.memory[0o240], 0o201);

        // Execute nested JMS
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o051);
        assert_eq!(cpu.memory[0o050], 0o242);

        // Execute CLA
        setup_and_run(&mut cpu, &[], 1);

        // Return from nested
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o242);

        // Return from main
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o201);
    }

    #[test]
    fn test_nested_subroutines_indirect() {
        let mut cpu = make_cpu();

        // Main
        cpu.memory[0o200] = 0o4500; // JMS I 0100
        cpu.memory[0o201] = 0o7402; // HLT
        cpu.memory[0o100] = 0o0300; // Pointer to subroutine 1

        // Subroutine 1
        cpu.memory[0o300] = 0o0000;
        cpu.memory[0o301] = 0o4510; // JMS I 0110
        cpu.memory[0o302] = 0o5700; // JMP I 0300
        cpu.memory[0o110] = 0o0400; // Pointer to subroutine 2

        // Subroutine 2
        cpu.memory[0o400] = 0o0000;
        cpu.memory[0o401] = 0o7200; // CLA
        cpu.memory[0o402] = 0o5600; // JMP I 0400

        cpu.pc = 0o200;

        // Execute main JMS
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o301);
        assert_eq!(cpu.memory[0o300], 0o201);

        // Execute nested JMS
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o401);
        assert_eq!(cpu.memory[0o400], 0o302);

        // Execute CLA
        setup_and_run(&mut cpu, &[], 1);

        // Return from nested
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o302);

        // Return from main
        setup_and_run(&mut cpu, &[], 1);
        assert_eq!(cpu.pc, 0o201);
    }

    /*=====================================================================
     * Mode Switching Tests
     *===================================================================*/

    #[test]
    fn test_swab_mode_switch() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o1234;
        cpu.memory[0o200] = 0o7431; // SWAB
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert!(matches!(cpu.emode, EAEMode::ModeB));
        assert_eq!(cpu.mq, 0o1234); // AC moved to MQ
        assert_eq!(cpu.acc & VALUE_MASK, 0); // AC cleared
    }

    #[test]
    fn test_swba_mode_switch() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeB;
        cpu.eae_gtf = true;
        cpu.memory[0o200] = 0o7447; // SWBA
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert!(matches!(cpu.emode, EAEMode::ModeA));
        assert_eq!(cpu.eae_gtf, false);
    }

    /*=====================================================================
     * Basic EAE Tests
     *===================================================================*/

    #[test]
    fn test_sca_shift_counter_to_ac() {
        let mut cpu = make_cpu();
        cpu.eae_sc = 0o15;
        cpu.acc = 0o7000;
        cpu.memory[0o200] = 0o7641; // CLA SCA (was 0o7441)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(
            cpu.acc & VALUE_MASK,
            0o15,
            "Shift count should be loaded into AC, AC = {:04o}, SC = {:04o}",
            cpu.acc & VALUE_MASK,
            cpu.eae_sc
        );
    }

    #[test]
    fn test_mqa_mq_to_ac() {
        let mut cpu = make_cpu();
        cpu.mq = 0o1234;
        cpu.acc = 0o5000;
        cpu.memory[0o200] = 0o7501; // MQA
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o5234); // OR'd together
    }

    #[test]
    fn test_mql_ac_to_mq() {
        let mut cpu = make_cpu();
        cpu.acc = 0o13456; // Link set
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7421; // MQL
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.mq, 0o3456);
        assert_eq!(cpu.acc, 0o10000); // AC cleared, link preserved
    }

    #[test]
    fn test_swp_swap_ac_mq() {
        let mut cpu = make_cpu();
        cpu.acc = 0o11234; // Link set, AC = 0o1234 (668 decimal)
        cpu.mq = 0o5432; // Valid octal (2842 decimal)
        cpu.memory[0o200] = 0o7521; // SWP (MQA + MQL)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // After SWP: AC and MQ are swapped
        assert_eq!(cpu.acc & VALUE_MASK, 0o5432); // MQ value now in AC
        assert_eq!(cpu.mq, 0o1234); // AC value now in MQ
        assert_eq!(cpu.acc & LINK_MASK, 0o10000); // Link preserved
    }

    /*=====================================================================
     * Multiply Tests
     *===================================================================*/

    #[test]
    fn test_muy_simple_multiply() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.mq = 0o0012; // 10 decimal
        cpu.acc = 0o0000;
        cpu.memory[0o200] = 0o7405; // MUY
        cpu.memory[0o201] = 0o0015; // 13 decimal
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // 10 * 13 = 130 decimal = 0o202
        let result = ((cpu.acc & VALUE_MASK) as u32) << 12 | cpu.mq as u32;
        assert_eq!(result, 0o202);
        assert_eq!(cpu.eae_sc, 0o014);
    }

    #[test]
    fn test_muy_with_initial_ac() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.mq = 0o0005;
        cpu.acc = 0o0003; // Initial value added to product
        cpu.memory[0o200] = 0o7405; // MUY
        cpu.memory[0o201] = 0o0004;
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // (5 * 4) + 3 = 23 decimal = 0o27
        let result = ((cpu.acc & VALUE_MASK) as u32) << 12 | cpu.mq as u32;
        assert_eq!(result, 0o27);
    }

    #[test]
    fn test_muy_large_numbers() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.mq = 0o7777; // Max 12-bit value
        cpu.acc = 0o0000;
        cpu.memory[0o200] = 0o7405; // MUY
        cpu.memory[0o201] = 0o7777;
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // 4095 * 4095 = 16769025 = 0o77760001
        assert_eq!(cpu.acc & VALUE_MASK, 0o7776);
        assert_eq!(cpu.mq, 0o0001);
    }

    /*=====================================================================
     * Divide Tests
     *===================================================================*/

    #[test]
    fn test_dvi_simple_divide() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o0000;
        cpu.mq = 0o0144; // 100 decimal
        cpu.memory[0o200] = 0o7407; // DVI
        cpu.memory[0o201] = 0o0012; // 10 decimal
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.mq, 0o0012); // Quotient: 10
        assert_eq!(cpu.acc & VALUE_MASK, 0o0000); // Remainder: 0
        assert_eq!(cpu.eae_sc, 0o015);
    }

    #[test]
    fn test_dvi_with_remainder() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o0000;
        cpu.mq = 0o0027; // 23 decimal
        cpu.memory[0o200] = 0o7407; // DVI
        cpu.memory[0o201] = 0o0005; // 5 decimal
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.mq, 0o0004); // Quotient: 4
        assert_eq!(cpu.acc & VALUE_MASK, 0o0003); // Remainder: 3
    }

    #[test]
    fn test_dvi_overflow() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o0012; // Dividend high >= divisor
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7407; // DVI
        cpu.memory[0o201] = 0o0010;
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert!(cpu.acc & LINK_MASK != 0); // Link set on overflow
        assert_eq!(cpu.eae_sc, 0);
    }

    /*=====================================================================
     * Normalize Tests
     *===================================================================*/

    #[test]
    fn test_nmi_normalize_positive() {
        let mut cpu = make_cpu();
        cpu.acc = 0o0200; // Bit 7 of AC
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7411; // NMI
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // Bit 7 of AC (bit 19 of temp) shifts to bit 23 (4 shifts)
        assert_eq!(cpu.acc & VALUE_MASK, 0o4000); // Normalized positive
        assert_eq!(cpu.eae_sc, 4); // Shifted 4 times
    }

    #[test]
    fn test_nmi_normalize_positive_mq() {
        let mut cpu = make_cpu();
        cpu.acc = 0o0000;
        cpu.mq = 0o0100; // Bit 6 of MQ
        cpu.memory[0o200] = 0o7411; // NMI
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // Bit 6 of MQ shifts to bit 23 (17 shifts)
        assert_eq!(cpu.acc & VALUE_MASK, 0o4000); // Normalized positive
        assert_eq!(cpu.eae_sc, 17); // Shifted 17 times
    }

    #[test]
    fn test_nmi_normalize_negative() {
        let mut cpu = make_cpu();
        cpu.acc = 0o17700; // Link=1, AC negative
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7411; // NMI
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // After normalization: L=1, AC=0 (still represents negative)
        assert!(cpu.acc & LINK_MASK != 0, "Link should be set (negative)");
        assert!(cpu.eae_sc > 0, "Should have shifted");
    }

    #[test]
    fn test_nmi_normalize_negative_mq() {
        let mut cpu = make_cpu();
        cpu.acc = 0o17700; // Link=1, AC negative
        cpu.mq = 0o4000; // Some bits in MQ
        cpu.memory[0o200] = 0o7411; // NMI
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // Should normalize negative number
        // After shifting, AC should still represent negative
        assert!(cpu.acc & LINK_MASK != 0, "Link should be set");
        assert!(cpu.eae_sc > 0, "Should have shifted");
        // The exact AC value depends on how many bits were set
    }

    #[test]
    fn test_nmi_already_normalized_negative() {
        let mut cpu = make_cpu();
        cpu.acc = 0o10000; // L=1, AC=0 (normalized negative: 100...000)
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7411; // NMI
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.eae_sc, 0); // No shifts needed
    }

    /*=====================================================================
     * Shift Tests
     *===================================================================*/

    #[test]
    fn test_shl_shift_left() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o0001;
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7413; // SHL
        cpu.memory[0o201] = 0o0002; // Shift 3 positions (2+1)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o0010); // 1 << 3 = 8
    }

    #[test]
    fn test_asr_arithmetic_shift_right() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o4000; // Negative number
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7415; // ASR
        cpu.memory[0o201] = 0o0000; // Shift 1 position (0+1)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o6000); // Sign extended
    }

    #[test]
    fn test_lsr_logical_shift_right() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o4000;
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7417; // LSR
        cpu.memory[0o201] = 0o0000; // Shift 1 position (0+1)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.acc & VALUE_MASK, 0o2000); // No sign extension
    }

    #[test]
    fn test_asr_negative_sign_extension() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o7777; // All 1s (negative)
        cpu.mq = 0o7777;
        cpu.memory[0o200] = 0o7415; // ASR
        cpu.memory[0o201] = 0o0003; // Shift 4 positions (3+1)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // Should remain all 1s (sign extended)
        assert_eq!(cpu.acc & VALUE_MASK, 0o7777);
        assert_eq!(cpu.mq, 0o7777);
    }

    #[test]
    fn test_asr_positive_no_extension() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;
        cpu.acc = 0o3777; // Positive
        cpu.mq = 0o7777;
        cpu.memory[0o200] = 0o7415; // ASR
        cpu.memory[0o201] = 0o0002; // Shift 3 positions (2+1) - FIXED!
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // Should shift in 0s from the left
        assert_eq!(cpu.acc & VALUE_MASK, 0o0377);
        assert_eq!(cpu.mq, 0o7777);
    }

    /*=====================================================================
     * Mode B Double Precision Tests
     *===================================================================*/

    #[test]
    fn test_dad_double_add() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeB;
        cpu.acc = 0o0001; // High word
        cpu.mq = 0o0000; // Low word
        cpu.memory[0o200] = 0o7443; // DAD
        cpu.memory[0o201] = 0o0100; // Pointer
        cpu.memory[0o100] = 0o7777; // Low word to add
        cpu.memory[0o101] = 0o0001; // High word to add
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // 1'0000 + 1'7777 = 2'7777
        assert_eq!(cpu.acc & VALUE_MASK, 0o0002);
        assert_eq!(cpu.mq, 0o7777);
    }

    #[test]
    fn test_dst_double_store() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeB;
        cpu.acc = 0o1357; // 751 decimal
        cpu.mq = 0o2460; // 1328 decimal
        cpu.memory[0o200] = 0o7445; // DST
        cpu.memory[0o201] = 0o0100; // Pointer
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.memory[0o100], 0o2460); // Low word (MQ)
        assert_eq!(cpu.memory[0o101], 0o1357); // High word (AC)
    }

    #[test]
    fn test_dpsz_skip_if_zero() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeB;
        cpu.acc = 0o0000;
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7451; // DPSZ
        cpu.memory[0o201] = 0o7402; // HLT (should be skipped)
        cpu.memory[0o202] = 0o7000; // NOP
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.pc, 0o202); // Skipped
    }

    #[test]
    fn test_dpsz_no_skip() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeB;
        cpu.acc = 0o0001;
        cpu.mq = 0o0000;
        cpu.memory[0o200] = 0o7451; // DPSZ
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.pc, 0o201); // No skip
    }

    #[test]
    fn test_sam_subtract_ac_from_mq_no_borrow() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeB;
        cpu.mq = 0o0100; // 64 decimal
        cpu.acc = 0o0040; // 32 decimal
        cpu.memory[0o200] = 0o7457; // MQA + SCA + SAM (Mode B)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // SAM uses original AC (0o0040) before MQA modifies it
        // MQ - AC = 64 - 32 = 32
        assert_eq!(cpu.acc & VALUE_MASK, 0o0040);
        assert_eq!(cpu.eae_gtf, true); // No borrow (MQ >= AC)
    }

    #[test]
    fn test_sam_subtract_ac_from_mq_with_borrow() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeB;
        cpu.mq = 0o0040; // 32 decimal
        cpu.acc = 0o0100; // 64 decimal
        cpu.memory[0o200] = 0o7457; // MQA + SCA + SAM (Mode B)
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        // SAM uses original AC (0o0100) before MQA modifies it
        // MQ - AC = 32 - 64 = -32 (two's complement)
        assert_eq!(cpu.acc & VALUE_MASK, 0o7740); // -32 in two's complement
        assert_eq!(cpu.eae_gtf, false); // Borrow occurred (MQ < AC)
    }

    /*=====================================================================
     * Combined Operation Tests
     *===================================================================*/

    #[test]
    fn test_multiply_then_divide() {
        let mut cpu = make_cpu();
        cpu.emode = EAEMode::ModeA;

        // Multiply 10 * 12 = 120
        cpu.mq = 0o0012; // 10
        cpu.acc = 0o0000;
        cpu.memory[0o200] = 0o7405; // MUY
        cpu.memory[0o201] = 0o0014; // 12
        cpu.pc = 0o200;

        setup_and_run(&mut cpu, &[], 1);

        let product = ((cpu.acc & VALUE_MASK) as u32) << 12 | cpu.mq as u32;
        assert_eq!(product, 0o170); // 120 decimal

        // Divide 120 / 6 = 20
        cpu.memory[0o202] = 0o7407; // DVI
        cpu.memory[0o203] = 0o0006; // 6
        cpu.pc = 0o202;

        setup_and_run(&mut cpu, &[], 1);

        assert_eq!(cpu.mq, 0o0024); // 20 decimal
        assert_eq!(cpu.acc & VALUE_MASK, 0o0000); // No remainder
    }

    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // IOT testing!
    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    #[derive(Debug)]
    struct MockIotDevice {
        pub call_count: u32,
    }

    impl SimResourceProvider for MockIotDevice {
        fn get_metadata(&self) -> Vec<sim_core::env::ResourceMetadata> {
            Vec::new()
        }

        fn read_resource(&self, _res_id: u32, _index: usize) -> Result<u64, SimError> {
            Err(SimError::NoSuchResource("MockIotDevice".to_owned()))
        }

        fn write_resource(&mut self, _res_id: u32, _index: usize, _val: u64) -> Result<(), SimError> {
            Err(SimError::NoSuchResource("MockIotDevice".to_owned()))
        }
    }

    impl DeviceTraits<PDP8Processor> for MockIotDevice {
        fn device_name(&self) -> &str {
            "MOCK"
        }

        fn description(&self) -> &str {
            "Mock IOT device"
        }

        fn device_service(&mut self, _cpu: &mut PDP8Processor, _bus: &mut SystemBus) -> Result<(), SimError> {
            // NOP
            Ok(())
        }

        fn device_reset(&mut self) {
            self.call_count = 0;
        }

        fn execute_io(
            &mut self,
            _io_data: &PDP8IoPayload,
            cpu: &mut PDP8Processor,
            _bus: &mut SystemBus,
        ) -> Result<(), SimError> {
            self.call_count += 1;
            cpu.acc += 1;
            Ok(())
        }
    }

    struct IntegrationAccessor<'a> {
        pub device: &'a mut MockIotDevice,
    }

    impl<'a> DeviceAccessor<PDP8Processor> for IntegrationAccessor<'a> {
        fn resolve_handle(&self, name: &str) -> Option<DeviceHandle> {
            if name == "MOCK" {
                Some(DeviceHandle(0))
            } else {
                None
            }
        }

        fn get_device_mut(
            &mut self,
            _handle: DeviceHandle,
        ) -> Option<&mut (dyn DeviceTraits<PDP8Processor> + Send)> {
            // We only have one device at index 0
            Some(self.device)
        }
    }

    #[test]
    fn test_iot_dispatch_integration() {
        // loop through all of the devices, except for the several special IOTs
        for iot_device in 0o00u16..=0o77u16 {
            match iot_device {
                // opr_iot handles these devices directly.
                0o00 | 0o10 | 0o20..=0o27 => continue,
                // Everything else: Test the dispatch
                _ => {
                    let mut cpu = make_cpu();
                    let mut mock_hw = MockIotDevice { call_count: 0 };
                    let timer_mgr = TimerManager::new(create_platform_timer());
                    let mut sysbus = SystemBus::new(timer_mgr);

                    // 1. Setup Intent: Map IOT to a device named "MOCK"
                    let mut accessor = IntegrationAccessor { device: &mut mock_hw };
                    cpu.mock_iot_device(iot_device, "MOCK", &mut accessor);

                    // 2. Wiring Phase: Simulate the environment's finalize_hardware()
                    cpu.wire_devices(&accessor);

                    // 3. Execution: Run an IOT 6101 (Device 10, Pulse 1)
                    // In our Mock, pulse doesn't matter, it just increments AC.
                    cpu.acc = 0o0005;
                    cpu.memory[0o200] = 0o6001 | (iot_device << 3);
                    cpu.pc = 0o200;

                    // We run setup_and_run manually here to pass our specific accessor
                    // let mut accessor = IntegrationAccessor { device: &mut mock_hw };
                    cpu.execute_instruction(0o6001 | (iot_device << 3), &mut sysbus, &mut accessor)
                        .expect("IOT should succeed");

                    // 4. Verify: The device was called, and it modified the CPU's Accumulator
                    assert_eq!(mock_hw.call_count, 1);
                    assert_eq!(cpu.acc, 0o0006);
                }
            }
        }
    }
}
