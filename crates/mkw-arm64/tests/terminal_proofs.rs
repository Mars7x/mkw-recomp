use mkw_arm64::{
    lift_function,
    terminal::{Image, Issue, SetupError, SiteKind},
};
fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
fn branch(pc: u64, target: u64, link: bool) -> u32 {
    (if link { 0x94000000 } else { 0x14000000 })
        | ((((target as i64 - pc as i64) / 4) as u32) & 0x3ffffff)
}
fn image() -> Image {
    let mut b = bytes(&[0xd503201f; 0x30]);
    // local leaf, wrapper, caller with a genuine returning branch, and bad exit.
    b[..4].copy_from_slice(&branch(0x1000, 0x1000, false).to_le_bytes());
    b[0x20..0x24].copy_from_slice(&branch(0x1020, 0x1000, true).to_le_bytes());
    b[0x40..0x4c].copy_from_slice(&bytes(&[
        0xb4000040,
        0xd65f03c0,
        branch(0x1048, 0x1020, true),
    ]));
    b[0x60..0x68].copy_from_slice(&bytes(&[0xb4000040, branch(0x1064, 0x1000, true)]));
    Image::new(0x1000, b).unwrap()
}
#[test]
fn closed_loop_and_local_dependency_chain_mint_only_normal_return_evidence() {
    let i = image();
    let leaf = i.analyze(0x1000, 4, &[]).unwrap();
    assert!(leaf.is_closed());
    let fact = leaf.no_return_fact().unwrap();
    assert_eq!(fact.dependencies().count(), 0);
    let raw = bytes(&[branch(0x1020, 0x1000, true)]);
    assert!(lift_function(&raw, 0x1020, 0x1020).is_err());
    let unproved = i.analyze(0x1020, 4, &[]).unwrap();
    assert!(!unproved.is_closed());
    assert!(unproved.no_return_fact().is_none());
    let wrapper = i.analyze(0x1020, 4, std::slice::from_ref(&fact)).unwrap();
    assert!(wrapper.is_closed());
    assert_eq!(wrapper.terminal_sites()[0].kind, SiteKind::Call);
    let second = wrapper.no_return_fact().unwrap();
    let caller = i
        .analyze(0x1040, 12, std::slice::from_ref(&second))
        .unwrap();
    assert!(caller.is_closed());
    assert_eq!(caller.possible_normal_returns(), [0x1044]);
    assert!(caller.no_return_fact().is_none());
    assert_eq!(
        second.supporting_facts()[0].proof_digest(),
        fact.proof_digest()
    );
    assert_ne!(second.proof_digest(), fact.proof_digest());
    let clone = i.clone();
    assert_eq!(
        clone
            .analyze(0x1020, 4, &[fact])
            .unwrap()
            .no_return_fact()
            .unwrap()
            .proof_digest(),
        second.proof_digest()
    );
    drop(i);
    drop(clone);
    drop(wrapper);
    assert_eq!(second.supporting_facts().len(), 1);
    assert_eq!(second.source().entry, 0x1020);
}
#[test]
fn conditional_escape_bypassing_a_terminal_call_and_every_decode_error_remain_visible() {
    let i = image();
    let fact = i.analyze(0x1000, 4, &[]).unwrap().no_return_fact().unwrap();
    let a = i.analyze(0x1060, 8, &[fact]).unwrap();
    assert!(!a.is_closed());
    assert!(a
        .issues()
        .iter()
        .any(|x| matches!(x, Issue::MissingEdge { pc: 0x1068 })));
    assert!(a.no_return_fact().is_none());
    let j = Image::new(0, bytes(&[0xffffffff, 0x91000400, 0xffffffff, 0xd65f03c0])).unwrap();
    let a = j.analyze(0, 16, &[]).unwrap();
    assert_eq!(a.issues().len(), 2);
    assert_eq!(a.instructions().len(), 2);
    assert!(a.no_return_fact().is_none());
    assert!(lift_function(
        &bytes(&[0xffffffff, 0x91000400, 0xffffffff, 0xd65f03c0]),
        0,
        0
    )
    .is_err());
}
#[test]
fn immutable_image_namespaces_reject_foreign_duplicate_recursive_and_wrong_target_facts() {
    let a = image();
    let b = image();
    let fact = a.analyze(0x1000, 4, &[]).unwrap().no_return_fact().unwrap();
    assert_eq!(a.digest(), b.digest());
    assert!(matches!(
        b.analyze(0x1020, 4, std::slice::from_ref(&fact)),
        Err(SetupError::ForeignImageFact)
    ));
    assert!(matches!(
        a.analyze(0x1020, 4, &[fact.clone(), fact.clone()]),
        Err(SetupError::DuplicateFact)
    ));
    assert!(matches!(
        a.analyze(0x1000, 4, std::slice::from_ref(&fact)),
        Err(SetupError::RecursiveFact)
    ));
    let unrelated = a.analyze(0x1004, 4, &[fact]).unwrap();
    assert!(!unrelated.is_closed());
    assert!(unrelated.no_return_fact().is_none());
}
#[test]
fn normal_auth_returns_dynamic_tails_and_unproven_external_tails_are_not_terminal_facts() {
    for word in [
        0xd65f03c0,
        0xd65f0bff,
        0xd65f0fff,
        0xd61f0000,
        branch(0, 0x100, false),
    ] {
        let i = Image::new(0, bytes(&[word])).unwrap();
        let a = i.analyze(0, 4, &[]).unwrap();
        assert!(a.is_closed());
        assert!(!a.possible_normal_returns().is_empty());
        assert!(a.no_return_fact().is_none());
    }
    let i = Image::new(0, bytes(&[0xd63f0000, 0x14000000])).unwrap();
    assert!(i.analyze(0, 8, &[]).unwrap().no_return_fact().is_some());
}
#[test]
fn only_exact_proven_direct_calls_prune_unreachable_bytes_and_tail_dependencies() {
    let mut b = bytes(&[
        branch(0, 0x10, true),
        0xffffffff,
        0,
        0,
        0x14000000,
        branch(0x14, 0x10, false),
    ]);
    let i = Image::new(0, b.clone()).unwrap();
    let fact = i.analyze(0x10, 4, &[]).unwrap().no_return_fact().unwrap();
    assert!(!i.analyze(0, 8, &[]).unwrap().is_closed());
    let a = i.analyze(0, 8, std::slice::from_ref(&fact)).unwrap();
    assert!(a.is_closed());
    assert_eq!(a.instructions().len(), 1);
    assert!(a.no_return_fact().is_some());
    let tail = i.analyze(0x14, 4, &[fact]).unwrap();
    assert_eq!(tail.terminal_sites()[0].kind, SiteKind::Tail);
    assert!(tail.no_return_fact().is_some());
    // Source mutation changes identity and cannot reuse evidence from the old image.
    b[0x10..0x14].copy_from_slice(&0xd65f03c0u32.to_le_bytes());
    let other = Image::new(0, b).unwrap();
    assert_ne!(other.digest(), i.digest());
    assert!(other
        .analyze(0x10, 4, &[])
        .unwrap()
        .no_return_fact()
        .is_none());
}
#[test]
fn image_and_span_validation_precede_any_graph_certificate() {
    assert!(matches!(Image::new(0, vec![]), Err(SetupError::EmptyImage)));
    assert!(matches!(
        Image::new(1, bytes(&[0x14000000])),
        Err(SetupError::Unaligned)
    ));
    assert!(matches!(
        Image::new(u64::MAX - 3, bytes(&[0x14000000])),
        Err(SetupError::Overflow)
    ));
    let i = image();
    for (entry, size, error) in [
        (0x1000, 0, SetupError::InvalidSpan),
        (0x1001, 4, SetupError::Unaligned),
        (0x1000, 3, SetupError::Unaligned),
        (0x1000, 1024 * 1024 + 4, SetupError::BodyTooLarge),
        (0xffc, 4, SetupError::InvalidSpan),
        (u64::MAX - 3, 4, SetupError::Overflow),
    ] {
        assert!(matches!(i.analyze(entry,size,&[]),Err(e) if e==error));
    }
}
