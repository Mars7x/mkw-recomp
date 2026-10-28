use mkw_arm64::terminal::{
    conditional::{Effect, Error, Kind},
    Image, SetupError,
};
fn bytes(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|w| w.to_le_bytes()).collect()
}
fn fixture() -> Image {
    let mut w = vec![0xd503201f; 32];
    w[0] = 0x14000000;
    w[1] = 0x14000000;
    w[4] = 0x94000004;
    w[5] = 0xd65f03c0;
    w[8..12].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    w[12..16].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    Image::new(0x1000, bytes(&w)).unwrap()
}
fn alias(i: &Image, at: u64, provider: u64) -> Effect {
    let f = i
        .analyze(provider, 4, &[])
        .unwrap()
        .no_return_fact()
        .unwrap();
    i.conditional_import(&i.import_thunk(at, 0).unwrap(), &Effect::from_local(&f))
        .unwrap()
}
#[test]
fn conditional_theorems_retain_exact_source_premises_and_do_not_convert_to_plain_facts() {
    let i = fixture();
    let a = alias(&i, 0x1020, 0x1000);
    assert_eq!(a.kind(), Kind::Import);
    assert!(a.local_source_fact().is_none());
    assert_eq!(a.requirements().len(), 1);
    assert_eq!(a.requirements()[0].thunk().slot(), 0x3018);
    assert_eq!(a.requirements()[0].provider().entry, 0x1000);
    let g = i
        .analyze_conditionally(0x1010, 8, std::slice::from_ref(&a))
        .unwrap();
    assert!(g.is_closed_conditionally());
    let f = g.no_return_effect().unwrap().unwrap();
    assert_eq!(f.requirements().len(), 1);
    assert_eq!(f.dependencies()[0].digest(), a.digest());
    assert_eq!(
        f.digest(),
        i.clone()
            .analyze_conditionally(0x1010, 8, &[a])
            .unwrap()
            .no_return_effect()
            .unwrap()
            .unwrap()
            .digest()
    );
    assert!(f.local_source_fact().is_none());
    assert!(i
        .analyze(0x1010, 8, &[])
        .unwrap()
        .no_return_fact()
        .is_none());
    assert_eq!(i.analyze(0x1010, 4, &[]).unwrap().issues().len(), 1);
    drop(i);
    assert_eq!(f.requirements()[0].thunk().sources()[0].bytes, 16);
}
#[test]
fn same_slot_conflicts_and_foreign_duplicate_recursive_assumptions_refuse() {
    let i = fixture();
    let a = alias(&i, 0x1020, 0x1000);
    let other = fixture();
    assert!(matches!(
        other.analyze_conditionally(0x1010, 8, std::slice::from_ref(&a)),
        Err(Error::Source(SetupError::ForeignImageFact))
    ));
    assert!(matches!(
        i.analyze_conditionally(0x1010, 8, &[a.clone(), a.clone()]),
        Err(Error::Source(SetupError::DuplicateFact))
    ));
    assert!(matches!(
        i.conditional_import(&i.import_thunk(0x1020, 0).unwrap(), &a),
        Err(Error::Source(SetupError::RecursiveFact))
    ));
    assert!(matches!(
        other.conditional_import(&other.import_thunk(0x1020, 0).unwrap(), &a),
        Err(Error::Source(SetupError::ForeignImageFact))
    ));
    // Use both aliases on different reachable conditional paths.
    let mut w = vec![0xd503201f; 32];
    w[0] = 0x14000000;
    w[1] = 0x14000000;
    w[4] = 0xb4000060;
    w[5] = 0x94000003;
    w[6] = 0xd65f03c0;
    w[7] = 0x94000005;
    w[8..12].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    w[12..16].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    let j = Image::new(0x1000, bytes(&w)).unwrap();
    let a = alias(&j, 0x1020, 0x1000);
    let b = alias(&j, 0x1030, 0x1004);
    assert!(matches!(
        j.analyze_conditionally(0x1010, 16, &[a, b]),
        Err(Error::ConflictingSlot)
    ));
}
#[test]
fn empty_effects_match_unchanged_local_analysis_including_all_issues() {
    let ops = [
        0xd503201fu32,
        0xffffffff,
        0x14000000,
        0x14000002,
        0x94000003,
        0xd65f03c0,
        0xd65f0fff,
        0xd61f0000,
        0xd63f0000,
        0xb4000040,
    ];
    for a in ops {
        for b in ops {
            for c in ops {
                let i = Image::new(0, bytes(&[a, b, c])).unwrap();
                let old = i.analyze(0, 12, &[]).unwrap();
                let new = i.analyze_conditionally(0, 12, &[]).unwrap();
                assert_eq!(old.source(), new.source());
                assert_eq!(
                    format!("{:?}", old.instructions()),
                    format!("{:?}", new.instructions())
                );
                assert_eq!(old.issues(), new.issues());
                assert_eq!(old.possible_normal_returns(), new.possible_normal_returns());
                assert_eq!(old.terminal_sites(), new.terminal_sites());
                assert_eq!(
                    old.no_return_fact().is_some(),
                    new.no_return_effect().unwrap().is_some()
                );
                assert!(new.requirements().is_empty());
            }
        }
    }
}
#[test]
fn transitive_aliases_preserve_every_requirement_and_bypassing_edges_stay_visible() {
    let mut w = vec![0xd503201f; 64];
    w[0] = 0x14000000;
    w[4] = 0x94000004;
    w[8..12].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    w[12..16].copy_from_slice(&[0xd0000010, 0xf9401211, 0x91008210, 0xd61f0220]);
    w[16] = 0x97fffffc;
    w[20] = 0xb4000040;
    w[21] = 0x97fffff7;
    let i = Image::new(0x1000, bytes(&w)).unwrap();
    let a = alias(&i, 0x1020, 0x1000);
    let wrapper = i
        .analyze_conditionally(0x1010, 4, &[a])
        .unwrap()
        .no_return_effect()
        .unwrap()
        .unwrap();
    let b = i
        .conditional_import(&i.import_thunk(0x1030, 0).unwrap(), &wrapper)
        .unwrap();
    let top = i
        .analyze_conditionally(0x1040, 4, std::slice::from_ref(&b))
        .unwrap();
    assert!(top.no_return_effect().unwrap().is_some());
    assert_eq!(top.requirements().len(), 2);
    let bypass = i.analyze_conditionally(0x1050, 8, &[b]).unwrap();
    assert!(!bypass.is_closed_conditionally());
    assert_eq!(bypass.issues().len(), 1);
}
#[test]
fn bounded_proof_depth_and_input_count_refuse_without_unbounded_derivation() {
    let mut w = vec![0x14000000];
    for at in 1..=128u32 {
        w.push(0x94000000 | ((-(at as i32) as u32) & 0x03ffffff));
    }
    let i = Image::new(0, bytes(&w)).unwrap();
    let base = Effect::from_local(&i.analyze(0, 4, &[]).unwrap().no_return_fact().unwrap());
    assert!(base.local_source_fact().is_some());
    assert!(matches!(
        i.analyze_conditionally(4, 4, &vec![base.clone(); 1025]),
        Err(Error::TooManyFacts)
    ));
    let mut w = vec![0x14000000];
    w.extend(vec![0x97ffffff; 128]);
    let i = Image::new(0, bytes(&w)).unwrap();
    let mut effect = Effect::from_local(&i.analyze(0, 4, &[]).unwrap().no_return_fact().unwrap());
    for at in 1..128u64 {
        effect = i
            .analyze_conditionally(at * 4, 4, &[effect])
            .unwrap()
            .no_return_effect()
            .unwrap()
            .unwrap();
    }
    assert!(matches!(
        i.analyze_conditionally(512, 4, &[effect])
            .unwrap()
            .no_return_effect(),
        Err(Error::DepthLimit)
    ));
}

#[test]
fn bounded_import_requirements_refuse_more_than_sixty_four_premises() {
    let mut words = vec![0xd503201fu32; 16 + 66 * 4];
    words[0] = 0x14000000;
    for n in 0..66u32 {
        let at = 16 + n as usize * 4;
        let offset = n * 8;
        words[at..at + 4].copy_from_slice(&[
            0xd0000010,
            0xf9400211 | ((offset / 8) << 10),
            0x91000210 | (offset << 10),
            0xd61f0220,
        ]);
    }
    let i = Image::new(0x1000, bytes(&words)).unwrap();
    let mut effect =
        Effect::from_local(&i.analyze(0x1000, 4, &[]).unwrap().no_return_fact().unwrap());
    for n in 0..64u64 {
        effect = i
            .conditional_import(&i.import_thunk(0x1040 + n * 16, 0).unwrap(), &effect)
            .unwrap();
        assert_eq!(effect.requirements().len(), n as usize + 1);
    }
    assert!(matches!(
        i.conditional_import(&i.import_thunk(0x1440, 0).unwrap(), &effect),
        Err(Error::TooManyRequirements)
    ));
}
