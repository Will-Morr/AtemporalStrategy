use atemporal_contracts::{identity::*, *};
#[test]
fn births_are_tick_independent_bounded_and_cause_sensitive() {
    let mut registry = IdentityRegistry::new();
    let factory = genesis(&mut registry, 0, 0).unwrap();
    let command = command_id(1, 0, 3);
    let item = queue_item(&mut registry, &factory, &command, 0).unwrap();
    let first = production(&mut registry, &factory, &item, 0).unwrap();
    assert_eq!(
        first,
        production(&mut registry, &factory, &item, 0).unwrap()
    );
    assert_ne!(
        first,
        production(&mut registry, &factory, &item, 1).unwrap()
    );
    let other = queue_item(&mut registry, &factory, &command_id(2, 0, 3), 0).unwrap();
    assert_ne!(
        first,
        production(&mut registry, &factory, &other, 0).unwrap()
    );
    let mut last = factory;
    for i in 0..100 {
        last = production(&mut registry, &last, &item, i).unwrap();
        assert_eq!(last.len(), 71);
    }
    registry.insert(first.clone(), "wrong preimage".into());
    let factory = genesis(&mut registry, 0, 0).unwrap();
    assert!(production(&mut registry, &factory, &item, 0).is_err());
}
#[test]
fn rejects_versions_slots_unsafe_integers_and_unknown_fields() {
    assert!(serde_json::from_str::<ClientEnvelope>(r#"{"schema_version":2,"message":{"kind":"hello","protocol_version":1,"last_revision":null,"slot_token":null}}"#).is_err());
    assert!(serde_json::from_str::<GroupSlot>("10").is_err());
    assert!(serde_json::from_str::<SafeInt>("9007199254740992").is_err());
    assert!(serde_json::from_str::<Order>(r#"{"kind":"idle","extra":true}"#).is_err());
    assert_eq!(serde_json::to_string(&Version::default()).unwrap(), "1");
    assert_eq!(round_precedence(1, 4).unwrap().players, [1, 2, 3, 0]);
}
