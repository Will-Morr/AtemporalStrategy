use atemporal_contracts::{identity::*, *};
#[test]
fn tuple_births_are_fixed_size_scoped_and_keep_site_identity() {
    let command = command_id(1, 0, 3);
    let a = queue_item(&command, 0, 0).unwrap();
    let b = queue_item(&command, 1, 0).unwrap();
    assert_ne!(production(&a, 0), production(&b, 0));
    assert_ne!(production(&a, 0), production(&a, 1));
    assert_eq!(production(&a, 0), production(&a, 0)); // no tick input
    let site = blueprint(&command, 0).unwrap();
    assert_eq!(site, structure(&site));
    assert_ne!(site, genesis(0, 0).unwrap());
    assert!(queue_item(&command, 65536, 0).is_err());
    assert!(blueprint(&command, 65536).is_err());
    assert!(queue_item(&command_id(0, 0, 0), 0, 0).is_err());
    assert!(serde_json::from_str::<EntityId>(r#""entity:old-digest""#).is_err());
}
#[test]
fn rejects_versions_slots_unsafe_integers_and_unknown_fields() {
    assert!(serde_json::from_str::<Version>("1").is_err());
    assert!(serde_json::from_str::<GroupSlot>("10").is_err());
    assert!(serde_json::from_str::<SafeInt>("9007199254740992").is_err());
    assert!(serde_json::from_str::<Order>(r#"{"kind":"idle","extra":true}"#).is_err());
    assert_eq!(serde_json::to_string(&Version::default()).unwrap(), "2");
    assert_eq!(round_precedence(1, 4).unwrap().players, [1, 2, 3, 0]);
}
