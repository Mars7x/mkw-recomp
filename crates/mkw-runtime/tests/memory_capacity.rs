use mkw_runtime::{
    abi::{Cpu, CPU_ABI_VERSION},
    bridge::{MemoryBridge, MemoryOrder},
    dispatch::NativeTarget,
    floating::FloatingBackend,
    GuestAddr, Memory, Permissions,
};
#[allow(dead_code)]
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[test]
fn extended_plain_transfers_validate_full_ranges_and_buffers() {
    for bytes in [3usize, 6, 12, 24, 48, 64] {
        let mut m = Memory::default();
        m.map(
            GuestAddr(0x1000),
            vec![0x5a; bytes / 2],
            Permissions::RW,
            "synthetic first half",
        )
        .unwrap();
        m.map(
            GuestAddr(0x1000 + (bytes / 2) as u64),
            vec![0xa5; bytes - bytes / 2],
            Permissions::RW,
            "synthetic second half",
        )
        .unwrap();
        m.seal();
        let bridge = MemoryBridge::new(m);
        let initial =
            std::array::from_fn::<_, 8, _>(|n| 0x0123456789abcdefu64.wrapping_add(n as u64));
        let mut words = initial;
        bridge
            .access(
                MemoryOrder::Plain,
                true,
                GuestAddr(0x1000),
                &mut words,
                bytes,
            )
            .unwrap();
        let mut output = [u64::MAX; 8];
        bridge
            .access(
                MemoryOrder::Plain,
                false,
                GuestAddr(0x1000),
                &mut output,
                bytes,
            )
            .unwrap();
        let needed = bytes.div_ceil(8);
        let expected_bytes: Vec<_> = initial
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .take(bytes)
            .collect();
        let actual_bytes: Vec<_> = output[..needed]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        assert_eq!(&actual_bytes[..bytes], expected_bytes);
        assert!(actual_bytes[bytes..].iter().all(|&v| v == 0));
        assert!(output[needed..].iter().all(|&v| v == u64::MAX));
        let mut short = [0xdeadbeefu64; 8];
        let before = short;
        assert!(bridge
            .access(
                MemoryOrder::Plain,
                false,
                GuestAddr(0x1000),
                &mut short[..bytes.div_ceil(8) - 1],
                bytes
            )
            .is_err());
        assert_eq!(short, before);
        assert!(bridge
            .access(
                MemoryOrder::Plain,
                true,
                GuestAddr(0x1000),
                &mut short[..bytes.div_ceil(8) - 1],
                bytes
            )
            .is_err());
        assert!(bridge
            .access(
                MemoryOrder::Plain,
                false,
                GuestAddr(0x1001),
                &mut short,
                bytes
            )
            .is_err());
        assert_eq!(short, before);
        assert!(bridge
            .access(
                MemoryOrder::Plain,
                true,
                GuestAddr(0x1001),
                &mut short,
                bytes
            )
            .is_err());
        let mut unchanged = [0; 8];
        bridge
            .access(
                MemoryOrder::Plain,
                false,
                GuestAddr(0x1000),
                &mut unchanged,
                bytes,
            )
            .unwrap();
        let unchanged_bytes: Vec<_> = unchanged[..needed]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .take(bytes)
            .collect();
        assert_eq!(unchanged_bytes, expected_bytes);
        for order in [
            MemoryOrder::AcquireRcpc,
            MemoryOrder::AcquireRcsc,
            MemoryOrder::Release,
        ] {
            assert!(bridge
                .access(
                    order,
                    order == MemoryOrder::Release,
                    GuestAddr(0x1000),
                    &mut words,
                    bytes
                )
                .is_err());
        }
        let mut cpu = Cpu::default();
        {
            let mut attachment = bridge.as_ref().attach(&mut cpu);
            let c = attachment.cpu_mut();
            let callback = c.memory.unwrap();
            let mut sentinel = [0x12345678u64; 8];
            for invalid in [0u32, 5, 7, 9, 40, 65, 128] {
                assert_eq!(
                    unsafe {
                        callback(c.memory_context, 0, 0x1000, sentinel.as_mut_ptr(), invalid)
                    },
                    3
                );
                assert_eq!(sentinel, [0x12345678; 8]);
            }
            let ordered = c.ordered_memory.unwrap();
            for invalid in [3u32, 6, 12, 24, 48, 64] {
                assert_eq!(
                    unsafe {
                        ordered(
                            c.memory_context,
                            1,
                            0,
                            0x1000,
                            sentinel.as_mut_ptr(),
                            invalid,
                        )
                    },
                    3
                );
                assert_eq!(sentinel, [0x12345678; 8]);
            }
        }
    }
}
#[test]
fn abi9_rejects_stale_native_entries_and_fp_backends_before_loading() {
    assert_eq!(CPU_ABI_VERSION, 9);
    let path = folder("abi9-refusal");
    let code="unsigned current_budget_abi_version(void){return MKW_CPU_ABI_VERSION;}\nint current_budget(MkwCpu*c,uint64_t*b){(void)c;(void)b;return 0;}\nunsigned stale_budget_abi_version(void){return 8;}\nint stale_budget(MkwCpu*c,uint64_t*b){(void)c;(void)b;return 0;}\nint missing_budget(MkwCpu*c,uint64_t*b){(void)c;(void)b;return 0;}\nunsigned mkw_fp_abi_version(void){return 8;}\n";
    for opt in ["-O0", "-O2"] {
        let library = build(code, &path, opt);
        unsafe {
            assert!(
                NativeTarget::load(&library, b"current_budget\0", GuestAddr(0x9000), 0x9000)
                    .is_ok()
            );
            let error = NativeTarget::load(&library, b"stale_budget\0", GuestAddr(0x9000), 0x9000)
                .err()
                .unwrap();
            assert!(error.contains("ABI version mismatch"));
            assert!(
                NativeTarget::load(&library, b"missing_budget\0", GuestAddr(0x9000), 0x9000)
                    .is_err()
            );
            let error = FloatingBackend::load(&library).err().unwrap();
            assert!(error.contains("ABI version mismatch"));
        }
    }
}
