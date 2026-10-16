//! Nominal one-shot ownership keeps input, output and functor identities checked.
fn source(declarations: &str) -> String {
    format!(
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
data Other next = Other (Int -> next)
owner :: C.Resumption Op Int Int
owner = C.once (\n -> C.return n)
{declarations}
"#
    )
}

#[test]
fn accepts_the_owned_surface() {
    psrs_driver::check_source("Main.purs", &source("program :: C.Control Op Int\nprogram = C.resume owner 42\ncancelled :: Unit\ncancelled = C.cancel owner")).unwrap();
}

#[test]
fn rejects_a_wrong_resume_argument() {
    assert!(psrs_driver::check_source("Main.purs", &source("bad = C.resume owner true")).is_err());
}

#[test]
fn rejects_a_wrong_resume_result() {
    assert!(
        psrs_driver::check_source(
            "Main.purs",
            &source("bad :: C.Control Op Boolean\nbad = C.resume owner 42")
        )
        .is_err()
    );
}

#[test]
fn rejects_a_wrong_resume_functor() {
    assert!(
        psrs_driver::check_source(
            "Main.purs",
            &source("bad :: C.Control Other Int\nbad = C.resume owner 42")
        )
        .is_err()
    );
}

#[test]
fn rejects_cancellation_of_an_unrestricted_function() {
    assert!(
        psrs_driver::check_source("Main.purs", &source("bad = C.cancel (\\n -> C.return n)"))
            .is_err()
    );
}

#[test]
fn nominal_ownership_rejects_answer_coercion() {
    let text = source(
        "newtype Answer = Answer Int\nbad :: C.Resumption Op Answer Int\nbad = coerce owner",
    )
    .replace(
        "import Prim.Control as C",
        "import Prim.Control as C\nimport Safe.Coerce (coerce)",
    );
    let identity = text.replace("C.Resumption Op Answer Int", "C.Resumption Op Int Int");
    psrs_driver::check_source("Main.purs", &identity).unwrap();
    assert!(psrs_driver::check_source("Main.purs", &text).is_err());
}
