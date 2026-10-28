use mkw_arm64::terminal::{conditional::Effect, contract::Contract, Image};
fn bytes(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|w| w.to_le_bytes()).collect()
}
#[test]
fn exact_contracts_reject_foreign_proofs_and_plain_source_cannot_claim_no_return() {
    let image = Image::new(0, bytes(&[0x14000000])).unwrap();
    let fact = image.analyze(0, 4, &[]).unwrap().no_return_fact().unwrap();
    let e = Effect::from_local(&fact);
    let c = Contract::effect(&image, &e).unwrap();
    assert!(c.matches(&e));
    assert_ne!(c.origin([0; 32]), c.origin([1; 32]));
    assert!(!Contract::plain_services(&image, 0, 4).unwrap().matches(&e));
    let foreign = Image::new(0, bytes(&[0x14000000])).unwrap();
    assert!(Contract::effect(&foreign, &e).is_err());
    drop(image);
    assert_eq!(c.source(), fact.source());
    assert_eq!(c.claim().unwrap().digest(), e.digest());
}
#[test]
fn unsupported_veneer_and_old_local_dependency_dags_refuse_explicitly() {
    let w = [
        0x94000001, 0x14000000, 0x14000001, 0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220,
    ];
    let image = Image::new(0, bytes(&w)).unwrap();
    let fact = image.analyze(4, 4, &[]).unwrap().no_return_fact().unwrap();
    let wrapper = image
        .analyze(0, 4, std::slice::from_ref(&fact))
        .unwrap()
        .no_return_fact()
        .unwrap();
    assert_eq!(wrapper.supporting_facts().len(), 1);
    assert!(Contract::effect(&image, &Effect::from_local(&wrapper)).is_err());
    let local = Effect::from_local(&fact);
    let alias = image
        .conditional_import(&image.import_thunk(8, 1).unwrap(), &local)
        .unwrap();
    assert_eq!(alias.requirements()[0].thunk().sources().len(), 2);
    assert!(Contract::effect(&image, &alias).is_err());
    assert!(Contract::analyze(&image, 0, 4, &[]).is_err());
}
#[test]
fn plain_services_are_source_verified_and_do_not_widen_conditional_analysis() {
    let image = Image::new(0, bytes(&[0xd4000541, 0xd65f03c0])).unwrap();
    assert!(!image
        .analyze_conditionally(0, 8, &[])
        .unwrap()
        .issues()
        .is_empty());
    assert!(Contract::analyze(&image, 0, 8, &[]).is_err());
    let c = Contract::plain_services(&image, 0, 8).unwrap();
    assert!(c.claim().is_none() && c.effects().is_empty());
    assert_eq!(c.instructions().len(), 2);
    assert!(Contract::plain_services(&image, 0, 4).is_err());
}
