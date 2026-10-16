fn factory_source(st: bool, arity: usize) -> String {
    let parameters = (1..=arity).map(|n| format!("x{n}")).collect::<Vec<_>>();
    let signature = format!(
        "{} -> {}",
        vec!["Int"; arity].join(" -> "),
        if st { "ST.ST r Int" } else { "Effect Int" }
    );
    let arguments = std::iter::once("argument".to_owned())
        .chain((2..=arity).map(|n| n.to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    let (imports, prefix, quantify, eliminate) = if st {
        (
            "import Control.Monad.ST as ST\nimport Control.Monad.ST.Uncurried as U",
            "STFn",
            "forall r. ",
            "ST.run",
        )
    } else {
        (
            "import Effect (Effect)\nimport Effect.Uncurried as U",
            "EffectFn",
            "",
            "unsafePerformEffect",
        )
    };
    let action_type = if st {
        "forall r. ST.ST r Int"
    } else {
        "Effect Int"
    };
    format!(
        r#"module Main where
import Prelude
import Effect.Unsafe (unsafePerformEffect)
import Effect.Console (log)
{imports}

callback :: {quantify}{signature}
callback {} = let _ = unsafePerformEffect (log "factory") in pure ({})

main :: Int
main =
  let argument = let _ = unsafePerformEffect (log "argument") in 1
      action :: {action_type}
      action = U.run{prefix}{arity} (U.mk{prefix}{arity} callback) {arguments}
      _ = unsafePerformEffect (log "stored")
      first = {eliminate} action
      second = {eliminate} action
  in if first == {} && second == {} then 42 else 0
"#,
        parameters.join(" "),
        parameters
            .iter()
            .enumerate()
            .map(|(index, name)| format!("{name} * {}", index + 1))
            .collect::<Vec<_>>()
            .join(" + "),
        arity * (arity + 1) * (2 * arity + 1) / 6,
        arity * (arity + 1) * (2 * arity + 1) / 6,
    )
}

fn factories_are_delayed(st: bool) {
    for arity in 1..=10 {
        let source = factory_source(st, arity);
        let Some(output) =
            super::super::run_program_with_prelude_wasmtime(&[("Main.purs", &source)])
        else {
            return;
        };
        assert_eq!(output.status.code(), Some(42), "arity {arity}: {output:?}");
        assert_eq!(
            output.stdout, b"argument\nstored\nfactory\nfactory\n",
            "arity {arity}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "arity {arity}: {output:?}");
    }
}

#[test]
fn st_uncurried_factories_wait_for_execution_at_every_arity() {
    factories_are_delayed(true);
}

#[test]
fn effect_uncurried_factories_wait_for_execution_at_every_arity() {
    factories_are_delayed(false);
}

#[test]
fn effect_uncurried_transports_mixed_values_and_preserves_pure_instances() {
    let source = r#"module Main where
import Prelude
import Effect.Uncurried as U
import Effect.Unsafe (unsafePerformEffect)
import Effect.Console (log)

record :: U.EffectFn2 String (Int -> Int) { label :: String, answer :: Int }
record = U.mkEffectFn2 \label f -> pure { label, answer: f 41 }

left :: U.EffectFn1 Int String
left = U.mkEffectFn1 \_ -> log "left" *> pure "L"

right :: U.EffectFn1 Int String
right = U.mkEffectFn1 \_ -> log "right" *> pure "R"

main :: Int
main = unsafePerformEffect do
  value <- U.runEffectFn2 record "mixed" (_ + 1)
  joined <- U.runEffectFn1 (left <> right) 0
  empty <- U.runEffectFn1 (mempty :: U.EffectFn1 Int String) 0
  if value.label == "mixed" && value.answer == 42 && joined == "LR" && empty == ""
    then pure 42 else pure 0
"#;
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", source)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"left\nright\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
