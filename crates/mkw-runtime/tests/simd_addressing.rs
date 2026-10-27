mod support;
use mkw_runtime::{abi::Cpu, bridge::MemoryOrder, GuestAddr};
use support::{memory, pair, transfer, Program};
const RET: u32 = 0xd65f03c0;
#[test]
fn simd_widths_upper_clear_and_q_pair_cross_mappings() {
    for optimization in ["-O0", "-O2"] {
        let mut words = vec![
            pair(false, 16, true, [0, 1], 31, 3, -32),
            pair(true, 16, true, [2, 3], 31, 1, 32),
        ];
        for (rt, bytes) in [(4, 1), (5, 2), (6, 4), (7, 8), (31, 16)] {
            words.push(transfer(true, bytes, true, rt, 0, 0x39000000));
        }
        words.push(RET);
        let program = Program::build("simd-widths", &words, optimization);
        let bridge = memory();
        let mut cpu = Cpu {
            sp: 0x1090,
            ..Cpu::default()
        };
        cpu.x[0] = 0x1070;
        cpu.vregs[0] = [0x0123456789abcdef, 0xfff8000000000001];
        cpu.vregs[1] = [u64::MAX, 0x8081828384858687];
        for rt in [2, 3, 4, 5, 6, 7, 31] {
            cpu.vregs[rt] = [u64::MAX; 2];
        }
        // SAFETY: the attachment retains checked callback context; source is synthetic.
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { program.run(attached.cpu_mut()) }, 0);
        }
        assert_eq!(cpu.sp, 0x1090);
        assert_eq!(cpu.vregs[2], cpu.vregs[0]);
        assert_eq!(cpu.vregs[3], cpu.vregs[1]);
        assert_eq!(cpu.vregs[4], [0xef, 0]);
        assert_eq!(cpu.vregs[5], [0xcdef, 0]);
        assert_eq!(cpu.vregs[6], [0x89abcdef, 0]);
        assert_eq!(cpu.vregs[7], [0x0123456789abcdef, 0]);
        assert_eq!(cpu.vregs[31], cpu.vregs[0]);
        let mut data = [0; 4];
        bridge
            .access(MemoryOrder::Plain, false, GuestAddr(0x1070), &mut data, 32)
            .unwrap();
        assert_eq!(
            data,
            [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                cpu.vregs[1][0],
                cpu.vregs[1][1]
            ]
        );
    }
}
#[test]
fn packed_s_pairs_w_pairs_and_signed_word_pairs() {
    for optimization in ["-O0", "-O2"] {
        let words = [
            pair(false, 4, true, [0, 1], 2, 2, 0),
            pair(true, 4, true, [2, 3], 2, 2, 0),
            pair(false, 4, false, [0, 1], 2, 2, 8),
            pair(true, 4, false, [3, 4], 2, 2, 8),
            0x69401845,
            RET,
        ];
        let program = Program::build("packed-pairs", &words, optimization);
        let bridge = memory();
        let mut cpu = Cpu::default();
        cpu.x[2] = 0x1000;
        cpu.x[0] = 0xffffffff80000001;
        cpu.x[1] = 12345;
        cpu.x[3] = u64::MAX;
        cpu.x[4] = u64::MAX;
        cpu.vregs[0] = [0xdeadbeef80000001, u64::MAX];
        cpu.vregs[1] = [12345, u64::MAX];
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { program.run(attached.cpu_mut()) }, 0);
        }
        assert_eq!(cpu.vregs[2], [0x80000001, 0]);
        assert_eq!(cpu.vregs[3], [12345, 0]);
        assert_eq!(cpu.x[3], 0x80000001);
        assert_eq!(cpu.x[4], 12345);
        assert_eq!(cpu.x[5], 0xffffffff80000001);
        assert_eq!(cpu.x[6], 12345);
    }
}
#[test]
fn q_pair_faults_preserve_registers_memory_and_writeback() {
    for load in [false, true] {
        let words = [pair(load, 16, true, [0, 1], 2, 3, 32), RET];
        let program = Program::build(
            if load {
                "q-load-fault"
            } else {
                "q-store-fault"
            },
            &words,
            "-O2",
        );
        let bridge = memory();
        let mut cpu = Cpu::default();
        cpu.x[2] = 0x10d0;
        cpu.vregs[0] = [1, 2];
        cpu.vregs[1] = [3, 4];
        let before = cpu.vregs;
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { program.run(attached.cpu_mut()) }, 5);
        }
        assert_eq!(cpu.x[2], 0x10d0);
        assert_eq!(cpu.vregs, before);
        assert_eq!(cpu.fault_address, 0x10f0);
        let mut tail = [99; 2];
        bridge
            .access(MemoryOrder::Plain, false, GuestAddr(0x10f0), &mut tail, 16)
            .unwrap();
        assert_eq!(tail, [0, 0]);
    }
}
#[test]
fn scaled_extended_unscaled_and_post_index_addresses() {
    for optimization in ["-O0", "-O2"] {
        // STR X3,[X0,W1,UXTW #3]; LDR X4,[X2,W1,SXTW #3];
        // LDUR X5,[X2,-16]; LDR X6,[X0],8; STR X6,[X0,-8]!.
        let words = [
            transfer(
                false,
                8,
                false,
                3,
                0,
                0x38200800 | (1 << 16) | (2 << 13) | (1 << 12),
            ),
            transfer(
                true,
                8,
                false,
                4,
                2,
                0x38200800 | (1 << 16) | (6 << 13) | (1 << 12),
            ),
            transfer(true, 8, false, 5, 2, 0x38000000 | (0x1f0 << 12)),
            transfer(true, 8, false, 6, 0, 0x38000000 | (8 << 12) | (1 << 10)),
            transfer(
                false,
                8,
                false,
                6,
                0,
                0x38000000 | (0x1f8 << 12) | (3 << 10),
            ),
            RET,
        ];
        let program = Program::build("address-modes", &words, optimization);
        let bridge = memory();
        let mut seed = [0xfedcba9876543210, 0];
        bridge
            .access(MemoryOrder::Plain, true, GuestAddr(0x1000), &mut seed, 8)
            .unwrap();
        let mut cpu = Cpu::default();
        cpu.x[0] = 0x1000;
        cpu.x[1] = 0xfeedbeef00000002;
        cpu.x[2] = 0x1020;
        cpu.x[3] = 0x1122334455667788;
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { program.run(attached.cpu_mut()) }, 0);
        }
        assert_eq!(cpu.x[0], 0x1000);
        assert_eq!(cpu.x[4], 0);
        assert_eq!(cpu.x[5], cpu.x[3]);
        assert_eq!(cpu.x[6], seed[0]);
        // The same register-offset instruction exercises negative SXTW extension.
        let negative = Program::build(
            "negative-index",
            &[
                transfer(
                    true,
                    8,
                    false,
                    4,
                    2,
                    0x38200800 | (1 << 16) | (6 << 13) | (1 << 12),
                ),
                RET,
            ],
            optimization,
        );
        cpu.x[1] = 0xdeadbeeffffffffe;
        cpu.x[2] = 0x1010;
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { negative.run(attached.cpu_mut()) }, 0);
        }
        assert_eq!(cpu.x[4], seed[0]);
    }
}
#[test]
fn literal_and_sign_extended_loads_rebase() {
    for optimization in ["-O0", "-O2"] {
        let words = [0x98000080u32, 0x9c000061, RET];
        let program = Program::build("literal-transfers", &words, optimization);
        let bridge = memory();
        let mut data = [0x8877665580000001, 0x0123456789abcdef];
        bridge
            .access(MemoryOrder::Plain, true, GuestAddr(0x8010), &mut data, 16)
            .unwrap();
        let mut cpu = Cpu {
            module_base: 0x8000,
            ..Cpu::default()
        };
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { program.run(attached.cpu_mut()) }, 0);
        }
        assert_eq!(cpu.x[0], 0xffffffff80000001);
        assert_eq!(cpu.vregs[1], data);
    }
}
