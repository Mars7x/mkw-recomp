use mkw_arm64::terminal::{import::ImportError, Image};
fn words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn thunk(pages: i32, offset: u32) -> Vec<u32> {
    let imm = pages as u32 & 0x1fffff;
    vec![
        0x90000010 | ((imm & 3) << 29) | ((imm >> 2) << 5),
        0xf9400211 | ((offset / 8) << 10),
        0x91000210 | (offset << 10),
        0xd61f0220,
    ]
}
#[test]
fn exact_forms_cover_signed_pages_offsets_and_owned_source_namespace() {
    for page in [-0x100000, -4097, -1, 0, 1, 4097, 0xfffff] {
        for offset in (0..4096).step_by(8) {
            let raw = words(&thunk(page, offset));
            let image = Image::new(0x200000000, raw.clone()).unwrap();
            let w = image.import_thunk(0x200000000, 0).unwrap();
            assert_eq!(
                w.slot(),
                (0x200000000i64 + i64::from(page) * 4096 + i64::from(offset)) as u64
            );
            assert_eq!(w.entry(), w.stub_entry());
            assert_eq!(w.sources()[0].bytes, 16);
            assert!(w.belongs_to(&image.clone()));
            assert!(!w.belongs_to(&Image::new(0x200000000, raw).unwrap()));
            drop(image);
            assert_eq!(w.sources().len(), 1);
        }
    }
}
#[test]
fn rejects_all_changed_fixed_bits_and_disagreeing_offsets() {
    let raw = thunk(2, 24);
    let masks = [0x9f00001fu32, 0xffc003ff, 0xffc003ff, 0xffffffff];
    for i in 0..4 {
        for bit in 0..32 {
            if masks[i] & (1 << bit) != 0 {
                let mut changed = raw.clone();
                changed[i] ^= 1 << bit;
                assert!(
                    Image::new(0x400000, words(&changed))
                        .unwrap()
                        .import_thunk(0x400000, 0)
                        .is_err(),
                    "word {i}, bit {bit}"
                );
            }
        }
    }
    for offset in (0..4096).step_by(8) {
        if offset != 24 {
            let mut changed = raw.clone();
            changed[2] = 0x91000210 | (offset << 10);
            assert!(matches!(
                Image::new(0, words(&changed)).unwrap().import_thunk(0, 0),
                Err(ImportError::UnsupportedForm)
            ));
        }
    }
    for immediate in 0..4096u32 {
        let mut changed = raw.clone();
        changed[1] = 0xf9400211 | (immediate << 10);
        let result = Image::new(0, words(&changed)).unwrap().import_thunk(0, 0);
        assert_eq!(result.is_ok(), immediate == 3);
    }
    // An ordinary returning/dynamic tail must not acquire a terminal fact.
    let image = Image::new(0, words(&raw)).unwrap();
    assert!(image
        .analyze(0, 16, &[])
        .unwrap()
        .no_return_fact()
        .is_none());
}
#[test]
fn bounded_veneers_retain_each_exact_source_and_reject_loops_calls_and_bad_targets() {
    for count in 0..=8 {
        let mut code = vec![0x14000001; count];
        code.extend(thunk(2, 24));
        let image = Image::new(0x400000, words(&code)).unwrap();
        let w = image.import_thunk(0x400000, count).unwrap();
        assert_eq!(w.sources().len(), count + 1);
        assert_eq!(w.stub_entry(), 0x400000 + count as u64 * 4);
        assert_eq!(w.slot(), 0x402018);
        if count > 0 {
            assert!(matches!(
                image.import_thunk(0x400000, count - 1),
                Err(ImportError::VeneerLimit)
            ));
        }
    }
    let mut back = thunk(2, 24);
    back.push(0x17fffffc);
    let i = Image::new(0x400000, words(&back)).unwrap();
    assert_eq!(i.import_thunk(0x400010, 1).unwrap().stub_entry(), 0x400000);
    for code in [vec![0x14000000], vec![0x14000001, 0x17ffffff]] {
        assert!(matches!(
            Image::new(0, words(&code)).unwrap().import_thunk(0, 8),
            Err(ImportError::VeneerCycle)
        ));
    }
    for code in [
        vec![0x94000001, 0, 0, 0],
        vec![0x14000008],
        vec![0x17ffffff],
    ] {
        assert!(Image::new(0, words(&code))
            .unwrap()
            .import_thunk(0, 8)
            .is_err());
    }
}
#[test]
fn refuses_bounds_overflow_and_unrequested_chain_limits() {
    let i = Image::new(0x1000, words(&thunk(2, 24))).unwrap();
    assert!(matches!(
        i.import_thunk(0x1000, 9),
        Err(ImportError::VeneerLimit)
    ));
    for at in [0, 0x1001, 0x1010, u64::MAX] {
        assert!(i.import_thunk(at, 0).is_err());
    }
    for size in [4, 8, 12] {
        assert!(matches!(
            Image::new(0, words(&thunk(2, 24))[..size].to_vec())
                .unwrap()
                .import_thunk(0, 0),
            Err(ImportError::Truncated)
        ));
    }
    assert!(matches!(
        Image::new(0, words(&thunk(-1, 0)))
            .unwrap()
            .import_thunk(0, 0),
        Err(ImportError::ArithmeticOverflow)
    ));
    let high = u64::MAX - 0xfff;
    assert!(matches!(
        Image::new(high, words(&thunk(1, 0)))
            .unwrap()
            .import_thunk(high, 0),
        Err(ImportError::ArithmeticOverflow)
    ));
}
