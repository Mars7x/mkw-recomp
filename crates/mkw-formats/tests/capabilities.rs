use mkw_formats::capabilities::{CapabilityError, KernelCapabilityFacts, ThreadConstraints};
#[test]
fn every_syscall_bit_and_group_is_decoded_without_granting_missing_entries() {
    assert!(KernelCapabilityFacts::inspect(&[])
        .unwrap()
        .allowed_svc_numbers
        .is_empty());
    for number in 0u16..192 {
        let word = 0xf | (1 << (5 + number % 24)) | (u32::from(number / 24) << 29);
        let facts = KernelCapabilityFacts::inspect(&[word]).unwrap();
        assert_eq!(
            facts.allowed_svc_numbers.into_iter().collect::<Vec<_>>(),
            [number]
        );
    }
    let all = (0..8)
        .map(|g| 0xf | (0xffffff << 5) | (g << 29))
        .collect::<Vec<_>>();
    assert_eq!(
        KernelCapabilityFacts::inspect(&all)
            .unwrap()
            .allowed_svc_numbers
            .into_iter()
            .collect::<Vec<_>>(),
        (0..192).collect::<Vec<_>>()
    );
    assert_eq!(
        KernelCapabilityFacts::inspect(&[0xf, 0xf]),
        Err(CapabilityError::Duplicate { index: 1 })
    );
}
#[test]
fn numeric_ranges_and_quotas_remain_facts_without_default_target_configuration() {
    for min in [4u8, 28, 63] {
        for max in [min, 63] {
            let word = 7 | (u32::from(max) << 4) | (u32::from(min) << 10) | (2 << 16) | (5 << 24);
            assert_eq!(
                KernelCapabilityFacts::inspect(&[word]).unwrap().threads,
                Some(ThreadConstraints {
                    minimum_priority: min,
                    maximum_priority: max,
                    minimum_virtual_core: 2,
                    maximum_virtual_core: 5
                })
            );
        }
    }
    for size in [0u16, 1, 511, 512, 1023] {
        assert_eq!(
            KernelCapabilityFacts::inspect(&[0x7fff | (u32::from(size) << 16)])
                .unwrap()
                .handle_table_entries,
            Some(size)
        );
    }
    let facts = KernelCapabilityFacts::inspect(&[
        0x1fff | (1 << 14),
        0x3fff | (9 << 19) | (1 << 15),
        0xffff | (1 << 17),
    ])
    .unwrap();
    assert_eq!(
        (
            facts.program_type,
            facts.intended_kernel_version,
            facts.debug_flags
        ),
        (Some(1), Some((9, 1)), Some(1))
    );
    assert_eq!(
        KernelCapabilityFacts::inspect(&[u32::MAX]).unwrap(),
        KernelCapabilityFacts::default()
    );
}
#[test]
fn malformed_duplicate_and_unsupported_records_refuse_the_whole_inspection() {
    for word in [0x1fff | (1 << 17), 0x7fff | (1 << 26), 0xffff | (1 << 20)] {
        assert_eq!(
            KernelCapabilityFacts::inspect(&[word]),
            Err(CapabilityError::ReservedBits { index: 0 })
        );
    }
    for word in [
        7,
        7 | (4 << 10) | (3 << 4),
        7 | (4 << 10) | (4 << 4) | (5 << 16) | (2 << 24),
        0x3fff,
        0xffff | (3 << 17),
    ] {
        assert_eq!(
            KernelCapabilityFacts::inspect(&[word]),
            Err(CapabilityError::InvalidRange { index: 0 })
        );
    }
    for word in [
        7 | (4 << 10) | (4 << 4),
        0x1fff,
        0x3fff | (1 << 19),
        0x7fff,
        0xffff,
    ] {
        assert_eq!(
            KernelCapabilityFacts::inspect(&[word, word]),
            Err(CapabilityError::Duplicate { index: 1 })
        );
    }
    for low_ones in [0, 1, 2, 5, 6, 7, 8, 9, 10, 11, 12, 17, 31] {
        assert_eq!(
            KernelCapabilityFacts::inspect(&[(1u32 << low_ones) - 1]),
            Err(CapabilityError::UnsupportedKind { index: 0, low_ones })
        );
    }
}
