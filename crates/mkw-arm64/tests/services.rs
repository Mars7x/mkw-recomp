use mkw_arm64::{decode, decode_exclusive_v10, decode_services_v3, Op};
#[test]
fn every_svc_immediate_is_scoped_to_v3_and_exception_neighbors_stay_owned() {
    for immediate in 0..=65535u32 {
        let word = 0xd4000001 | (immediate << 5);
        assert!(decode(word, 0).is_err());
        assert!(decode_exclusive_v10(word, 0).is_err());
        assert_eq!(
            decode_services_v3(word, 0x1000).unwrap().op,
            Op::SupervisorCall {
                immediate: immediate as u16
            }
        );
        for neighbor in [2, 3, 0, 4, 0x1f] {
            assert!(decode_services_v3((word & !31) | neighbor, 0).is_err());
        }
    }
    for word in [0xd4200000, 0xd4400000, 0xd4a00001, 0xd4a00002, 0xd4a00003] {
        assert!(decode_services_v3(word, 0).is_err());
    }
    let raw = [0xd4000001u32, 0xd65f03c0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let ir = mkw_arm64::lift_services_v3(&raw, 0, 0).unwrap();
    assert_eq!(ir.len(), 2);
    assert!(mkw_arm64::lift(&raw, 0, 0).is_err());
    assert!(mkw_arm64::lift_services_v3(&raw[..4], 0, 0).is_err());
}
