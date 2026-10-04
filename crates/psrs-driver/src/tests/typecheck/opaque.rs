use super::*;

#[test]
fn lowers_an_opaque_foreign_type_to_core_without_collapsing_it_to_int() {
    let source = "\
module Main where
foreign import data Handle :: Type
foreign import data Other :: Type
keep :: Handle -> Handle
keep h = h
use :: (Handle -> Handle) -> Int
use _ = 0
main = use keep
";
    let core = lower_source_to_core("Main.purs", source).expect("opaque types lower to Core");
    let keep = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "keep")
        .expect("keep");
    let Some((parameter, result)) = psrs_core::arrow_parts(&core.types, keep.ty) else {
        panic!(
            "keep should be a function, got {:?}",
            core.types[keep.ty.0 as usize]
        );
    };
    let mut handle = None;
    for end in [parameter, result] {
        match &core.types[end.0 as usize] {
            psrs_core::Type::Constructor(psrs_core::TypeConstructor::User(id)) => {
                assert!(
                    core.opaque_ids.contains(id),
                    "Handle must be recorded as opaque"
                );
                assert!(
                    core.constructors
                        .iter()
                        .all(|constructor| constructor.type_id != *id),
                    "an opaque type has no constructors"
                );
                match handle {
                    None => handle = Some(*id),
                    Some(previous) => assert_eq!(previous, *id),
                }
            }
            other => panic!("Handle must not become {other:?}"),
        }
    }
    let handle = handle.expect("Handle");
    assert!(
        core.opaque_ids.iter().any(|id| *id != handle),
        "Other must stay a distinct opaque type, got {:?}",
        core.opaque_ids
    );
}
