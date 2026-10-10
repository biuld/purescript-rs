module Main where
import PSRS.ST as S

c0 :: forall r. S.Action r Int
c0 = S.bindAction (S.mapAction (\x -> intAdd 1 x) (S.pureAction 40)) (\x -> S.pureAction (intAdd x 1))

c1 :: forall r. S.Action r Int
c1 = S.bindAction (S.newCell 40) \ref -> S.bindAction (S.modifyCell (\x -> { state: intAdd x 1, value: intAdd x 2 }) ref) \_ -> S.readCell ref

c2 :: forall r. S.Action r Int
c2 = S.bindAction (S.newCell 0) \ref -> S.bindAction (S.writeCell 7 ref) \w -> S.bindAction (S.readCell ref) \r -> S.pureAction (intAdd w r)

c3 :: forall r. S.Action r Int
c3 = S.bindAction (S.newCell 1) \a -> S.bindAction (S.newCell 2) \b -> S.bindAction (S.writeCell 10 a) \_ -> S.bindAction (S.writeCell 20 b) \_ -> S.bindAction (S.readCell a) \x -> S.bindAction (S.readCell b) \y -> S.pureAction (intAdd x y)

c4 :: forall r. S.Action r Int
c4 = S.bindAction (S.newCell 0) \count -> S.bindAction (S.whileAction (S.mapAction (\c -> intLt c 3) (S.readCell count)) (S.bindAction (S.readCell count) \c -> S.writeCell (intAdd c 1) count)) \_ -> S.readCell count

c5 :: forall r. S.Action r Int
c5 = S.bindAction (S.newCell 0) \total -> S.bindAction (S.forAction 0 5 (\i -> S.bindAction (S.readCell total) \t -> S.writeCell (intAdd t i) total)) \_ -> S.readCell total

c6 :: forall r. S.Action r Int
c6 = S.bindAction (S.newCell 0) \total -> S.bindAction (S.foreachAction [1, 2, 3] (\i -> S.bindAction (S.readCell total) \t -> S.bindAction (S.writeCell (intAdd t i) total) \_ -> S.pureAction unit)) \_ -> S.readCell total

c7 :: forall r. S.Action r Int
c7 = S.bindAction (S.newCell 5) \total -> S.bindAction (S.foreachAction [] (\_ -> S.bindAction (S.pureAction unit) (\_ -> S.bindAction (S.pureAction (arrayIndex ([] :: Array Int) 0)) \_ -> S.pureAction unit))) \_ -> S.readCell total

c8 :: forall r. S.Action r Int
c8 = S.bindAction (S.newCell 5) \total -> S.bindAction (S.forAction 5 5 (\_ -> S.bindAction (S.pureAction unit) (\_ -> S.bindAction (S.pureAction (arrayIndex ([] :: Array Int) 0)) \_ -> S.pureAction unit))) \_ -> S.readCell total

c9 :: forall r. S.Action r Int
c9 = S.bindAction (S.newCell 5) \total -> S.bindAction (S.whileAction (S.pureAction false) (S.bindAction (S.pureAction unit) (\_ -> S.bindAction (S.pureAction (arrayIndex ([] :: Array Int) 0)) \_ -> S.pureAction unit))) \_ -> S.readCell total

main =
  if (booleanAnd (booleanAnd (booleanAnd (intEq (S.runAction c0) 42) (intEq (S.runAction c1) 41)) (booleanAnd (intEq (S.runAction c2) 14) (booleanAnd (intEq (S.runAction c3) 30) (intEq (S.runAction c4) 3)))) (booleanAnd (booleanAnd (intEq (S.runAction c5) 10) (intEq (S.runAction c6) 6)) (booleanAnd (intEq (S.runAction c7) 5) (booleanAnd (intEq (S.runAction c8) 5) (intEq (S.runAction c9) 5))))) then 42 else 1
