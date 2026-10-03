use super::*;

const PROXY: &str = "data Proxy :: forall k. k -> Type\ndata Proxy a = Proxy\n";

fn module(body: &str) -> String {
    format!("module Main where\nimport Prim\n{body}")
}

fn differential(name: &str, source: &str, expected_acceptance: bool) -> Option<String> {
    let case_dir =
        std::env::temp_dir().join(format!("psrs-purs-row-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&case_dir);
    std::fs::create_dir_all(&case_dir).expect("create row differential directory");
    let path = case_dir.join("Main.purs");
    std::fs::write(&path, source).expect("write row differential fixture");
    let purs_accepted = Command::new("purs")
        .arg("compile")
        .arg(&path)
        .arg("-o")
        .arg(case_dir.join("output"))
        .output()
        .expect("failed to run purs")
        .status
        .success();
    let _ = std::fs::remove_dir_all(&case_dir);
    let psrs_result = psrs_driver::check_source("Main.purs", source);
    let psrs_accepted = psrs_result.is_ok();
    if purs_accepted != expected_acceptance || psrs_accepted != expected_acceptance {
        return Some(format!(
            "{name}: purs accepted={purs_accepted}, psrs accepted={psrs_accepted}, expected={expected_acceptance}, diagnostics={:?}",
            psrs_result.err()
        ));
    }
    None
}

#[test]
fn differential_row_lacks_and_union_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let accepts = [
        (
            "lacks-generalizes-open-tail",
            module(&format!(
                "import Prim.Row (class Lacks)\n{PROXY}lacks :: forall row. Lacks \"x\" row => Proxy row -> Proxy Int\nlacks _ = Proxy\nextend :: forall row. Proxy row -> Proxy (y :: Int | row)\nextend _ = Proxy\ninferred proxy = lacks (extend proxy)\n"
            )),
        ),
        (
            "lacks-empty-closed-row-unknown-label",
            module(&format!(
                "import Prim.Row (class Lacks)\n{PROXY}empty :: forall label. Lacks label (a :: Int) => Proxy label -> Proxy Int\nempty _ = Proxy\nproof = empty (Proxy :: Proxy \"x\")\n"
            )),
        ),
        (
            "lacks-propagates-through-known-prefix",
            module(&format!(
                "import Prim.Row (class Lacks)\n{PROXY}lacks :: forall row. Lacks \"x\" row => Proxy row -> Proxy Int\nlacks _ = Proxy\npropagate :: forall row. Lacks \"x\" row => Proxy (y :: Int | row) -> Proxy Int\npropagate proxy = lacks proxy\n"
            )),
        ),
        (
            "lacks-defers-through-a-non-type-row-kind",
            module(&format!(
                "import Prim.Row (class Lacks)\n{PROXY}lacks :: forall (row :: Row Symbol). Lacks \"x\" row => Proxy row -> Proxy Int\nlacks _ = Proxy\npropagate :: forall (row :: Row Symbol). Lacks \"x\" row => Proxy (y :: \"value\" | row) -> Proxy Int\npropagate proxy = lacks proxy\n"
            )),
        ),
        (
            "lacks-proves-absence-in-a-closed-non-type-row",
            module(
                "import Prim.Row (class Lacks)\nproof :: Lacks \"x\" (a :: \"value\") => Int\nproof = 0\n",
            ),
        ),
        (
            "union-generalizes-open-left-remainder",
            module(&format!(
                "import Prim.Row (class Union)\n{PROXY}extend :: forall row. Proxy row -> Proxy (y :: Int | row)\nextend _ = Proxy\nunion :: forall left right output. Union left right output => Proxy left -> Proxy right -> Proxy output\nunion _ _ = Proxy\ninferred left right = union (extend left) right\n"
            )),
        ),
        (
            "union-merges-a-non-type-row-kind",
            module(
                "import Prim.Row (class Union)\ndata SProxy :: Row Symbol -> Type\ndata SProxy a = SProxy\nunion :: forall (left :: Row Symbol) (right :: Row Symbol) (output :: Row Symbol). Union left right output => SProxy left -> SProxy right -> SProxy output\nunion _ _ = SProxy\nresult :: SProxy (\"a\" :: \"x\" | (\"b\" :: \"y\"))\nresult = union (SProxy :: SProxy (\"a\" :: \"x\")) (SProxy :: SProxy (\"b\" :: \"y\"))\n",
            ),
        ),
        (
            "union-defers-an-open-non-type-row-remainder",
            module(
                "import Prim.Row (class Union)\ndata SProxy :: Row Symbol -> Type\ndata SProxy a = SProxy\nextend :: forall (row :: Row Symbol). SProxy row -> SProxy (y :: \"value\" | row)\nextend _ = SProxy\nunion :: forall (left :: Row Symbol) (right :: Row Symbol) (output :: Row Symbol). Union left right output => SProxy left -> SProxy right -> SProxy output\nunion _ _ = SProxy\ninferred left right = union (extend left) right\n",
            ),
        ),
        (
            "union-merges-duplicate-left-labels",
            module(&format!(
                "import Prim.Row (class Union)\n{PROXY}union :: forall left right output. Union left right output => Proxy left -> Proxy right -> Proxy output\nunion _ _ = Proxy\nboth :: Proxy (a :: Int | (a :: Boolean | (z :: String)))\nboth = union (Proxy :: Proxy (a :: Int | (a :: Boolean))) (Proxy :: Proxy (z :: String))\n"
            )),
        ),
        (
            "union-splits-closed-right-and-output",
            module(&format!(
                "import Prim.Row (class Union)\n{PROXY}union :: forall left right output. Union left right output => Proxy left -> Proxy right -> Proxy output\nunion _ _ = Proxy\nunknownLeft = Proxy\nsplit :: Proxy (b :: Boolean | (a :: Int))\nsplit = union unknownLeft (Proxy :: Proxy (a :: Int))\n"
            )),
        ),
        (
            "union-residual-inside-selected-instance-context",
            module(&format!(
                "import Prim.Row (class Union)\n{PROXY}extend :: forall row. Proxy row -> Proxy (y :: Int | row)\nextend _ = Proxy\nclass Joined left right output where\n  joined :: Proxy left -> Proxy right -> Proxy output\ninstance joinedUnion :: Union left right output => Joined left right output where\n  joined _ _ = Proxy\nnested left right = joined (extend left) right\n"
            )),
        ),
        (
            "union-residual-with-no-known-prefix-in-instance-context",
            module(&format!(
                "import Prim.Row (class Union)\n{PROXY}class Joined left right output where\n  joined :: Proxy left -> Proxy right -> Proxy output\ninstance joinedUnion :: Union left right output => Joined left right output where\n  joined _ _ = Proxy\nnested left right = joined left right\n"
            )),
        ),
    ];
    let rejects = [
        (
            "lacks-present-label-over-open-tail",
            module(&format!(
                "import Prim.Row (class Lacks)\n{PROXY}lacks :: forall row. Lacks \"x\" row => Proxy row -> Proxy Int\nlacks _ = Proxy\ntest :: Proxy Int\ntest = lacks (Proxy :: Proxy (x :: Int | (y :: Boolean)))\n"
            )),
            "NoInstanceFound",
        ),
        (
            "union-output-disagrees-with-closed-left",
            module(&format!(
                "import Prim.Row (class Union)\n{PROXY}union :: forall left right output. Union left right output => Proxy left -> Proxy right -> Proxy output\nunion _ _ = Proxy\ntest :: Proxy (c :: String)\ntest = union (Proxy :: Proxy (a :: Int)) (Proxy :: Proxy (b :: Boolean))\n"
            )),
            "TypesDoNotUnify",
        ),
        (
            "union-contradictory-relation-precedes-matching-given",
            module(&format!(
                "import Prim.Row (class Union)\n{PROXY}need :: forall right output. Union (a :: Int) right output => Proxy right -> Proxy output\nneed _ = Proxy\nbad :: forall right. Union (a :: Int) right (b :: Boolean) => Proxy right -> Proxy (b :: Boolean)\nbad = need\n"
            )),
            "TypesDoNotUnify",
        ),
    ];

    let mut failures = Vec::new();
    for (name, source) in &accepts {
        failures.extend(differential(name, source, true));
    }
    for (name, source, expected_code) in &rejects {
        if let Some(failure) = differential(name, source, false) {
            failures.push(failure);
            continue;
        }
        let errors = psrs_driver::check_source("Main.purs", source)
            .expect_err("a rejected row differential case must produce a diagnostic");
        if errors.len() != 1 || errors[0].code != Some(*expected_code) {
            failures.push(format!(
                "{name}: expected one `{expected_code}` diagnostic, found {:?}",
                errors.iter().map(|error| error.code).collect::<Vec<_>>()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "row relation differential failures:\n{}",
        failures.join("\n")
    );
}
