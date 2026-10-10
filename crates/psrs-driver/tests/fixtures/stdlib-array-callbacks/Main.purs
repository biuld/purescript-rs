module Main where
import PSRS.Array as A
main =
  let
    state0 = arrayFill 2 0
    visit0 x = let counted = arrayWrite state0 0 (intAdd (arrayIndex state0 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result0 = A.filterImpl (\x -> let next = visit0 x in if intLt 0 (arrayIndex next 0) then intLt 1 x else false) ([1, 2, 3] :: Array Int)
    state1 = arrayFill 2 0
    visit1 x = let counted = arrayWrite state1 0 (intAdd (arrayIndex state1 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result1 = A.filterImpl (\x -> let next = visit1 x in if intLt 0 (arrayIndex next 0) then intLt 0 x else false) ([1, 2, 3] :: Array Int)
    state2 = arrayFill 2 0
    visit2 x = let counted = arrayWrite state2 0 (intAdd (arrayIndex state2 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result2 = A.filterImpl (\x -> let next = visit2 x in if intLt 0 (arrayIndex next 0) then intLt 9 x else false) ([1, 2, 3] :: Array Int)
    state3 = arrayFill 2 0
    visit3 x = let counted = arrayWrite state3 0 (intAdd (arrayIndex state3 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result3 = A.filterImpl (\x -> let next = visit3 x in if intLt 0 (arrayIndex next 0) then intLt 0 x else false) ([] :: Array Int)
    state4 = arrayFill 2 0
    visit4 x = let counted = arrayWrite state4 0 (intAdd (arrayIndex state4 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result4 = A.partitionImpl (\x -> let next = visit4 x in if intLt 0 (arrayIndex next 0) then intLt 1 x else false) ([1, 2, 3] :: Array Int)
    state5 = arrayFill 2 0
    visit5 x = let counted = arrayWrite state5 0 (intAdd (arrayIndex state5 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result5 = A.partitionImpl (\x -> let next = visit5 x in if intLt 0 (arrayIndex next 0) then intLt 0 x else false) ([1, 2, 3] :: Array Int)
    state6 = arrayFill 2 0
    visit6 x = let counted = arrayWrite state6 0 (intAdd (arrayIndex state6 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result6 = A.partitionImpl (\x -> let next = visit6 x in if intLt 0 (arrayIndex next 0) then intLt 9 x else false) ([1, 2, 3] :: Array Int)
    state7 = arrayFill 2 0
    visit7 x = let counted = arrayWrite state7 0 (intAdd (arrayIndex state7 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result7 = A.partitionImpl (\x -> let next = visit7 x in if intLt 0 (arrayIndex next 0) then intLt 0 x else false) ([] :: Array Int)
    state8 = arrayFill 2 0
    visit8 x = let counted = arrayWrite state8 0 (intAdd (arrayIndex state8 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result8 = A.zipWithImpl (\x y -> let next = visit8 x in intAdd (intMul (arrayIndex next 0) 0) (intAdd x y)) ([1, 2, 3] :: Array Int) ([10, 20] :: Array Int)
    state9 = arrayFill 2 0
    visit9 x = let counted = arrayWrite state9 0 (intAdd (arrayIndex state9 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result9 = A.zipWithImpl (\x y -> let next = visit9 x in intAdd (intMul (arrayIndex next 0) 0) (intAdd x y)) ([1] :: Array Int) ([10, 20] :: Array Int)
    state10 = arrayFill 2 0
    visit10 x = let counted = arrayWrite state10 0 (intAdd (arrayIndex state10 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result10 = A.zipWithImpl (\x y -> let next = visit10 x in intAdd (intMul (arrayIndex next 0) 0) (intAdd x y)) ([] :: Array Int) ([10] :: Array Int)
    state11 = arrayFill 2 0
    visit11 x = let counted = arrayWrite state11 0 (intAdd (arrayIndex state11 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result11 = A.zipWithImpl (\x y -> let next = visit11 x in intAdd (intMul (arrayIndex next 0) 0) (intAdd x y)) ([1] :: Array Int) ([] :: Array Int)
    state12 = arrayFill 2 0
    visit12 x = let counted = arrayWrite state12 0 (intAdd (arrayIndex state12 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result12 = A.anyImpl (\x -> let next = visit12 x in if intLt 0 (arrayIndex next 0) then intLt 1 x else false) ([1, 2, 3] :: Array Int)
    state13 = arrayFill 2 0
    visit13 x = let counted = arrayWrite state13 0 (intAdd (arrayIndex state13 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result13 = A.allImpl (\x -> let next = visit13 x in if intLt 0 (arrayIndex next 0) then intLt 1 x else false) ([3, 1, 2] :: Array Int)
    state14 = arrayFill 2 0
    visit14 x = let counted = arrayWrite state14 0 (intAdd (arrayIndex state14 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result14 = A.anyImpl (\x -> let next = visit14 x in if intLt 0 (arrayIndex next 0) then intLt 0 x else false) ([] :: Array Int)
    state15 = arrayFill 2 0
    visit15 x = let counted = arrayWrite state15 0 (intAdd (arrayIndex state15 0) 1) in arrayWrite counted 1 (intAdd (intMul (arrayIndex counted 1) 10) x)
    result15 = A.allImpl (\x -> let next = visit15 x in if intLt 0 (arrayIndex next 0) then intLt 0 x else false) ([] :: Array Int)
  in if (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength result0) 2) (intEq (arrayIndex result0 0) 2)) (booleanAnd (intEq (arrayIndex result0 1) 3) (intEq (arrayIndex state0 0) 3))) (booleanAnd (booleanAnd (intEq (arrayIndex state0 1) 123) (intEq (arrayLength result1) 3)) (booleanAnd (intEq (arrayIndex result1 0) 1) (intEq (arrayIndex result1 1) 2)))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex result1 2) 3) (intEq (arrayIndex state1 0) 3)) (booleanAnd (intEq (arrayIndex state1 1) 123) (intEq (arrayLength result2) 0))) (booleanAnd (booleanAnd (intEq (arrayIndex state2 0) 3) (intEq (arrayIndex state2 1) 123)) (booleanAnd (intEq (arrayLength result3) 0) (booleanAnd (intEq (arrayIndex state3 0) 0) (intEq (arrayIndex state3 1) 0)))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength (result4).yes) 2) (intEq (arrayIndex (result4).yes 0) 2)) (booleanAnd (intEq (arrayIndex (result4).yes 1) 3) (intEq (arrayLength (result4).no) 1))) (booleanAnd (booleanAnd (intEq (arrayIndex (result4).no 0) 1) (intEq (arrayIndex state4 0) 3)) (booleanAnd (intEq (arrayIndex state4 1) 123) (intEq (arrayLength (result5).yes) 3)))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex (result5).yes 0) 1) (intEq (arrayIndex (result5).yes 1) 2)) (booleanAnd (intEq (arrayIndex (result5).yes 2) 3) (intEq (arrayLength (result5).no) 0))) (booleanAnd (booleanAnd (intEq (arrayIndex state5 0) 3) (intEq (arrayIndex state5 1) 123)) (booleanAnd (intEq (arrayLength (result6).yes) 0) (booleanAnd (intEq (arrayLength (result6).no) 3) (intEq (arrayIndex (result6).no 0) 1))))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex (result6).no 1) 2) (intEq (arrayIndex (result6).no 2) 3)) (booleanAnd (intEq (arrayIndex state6 0) 3) (intEq (arrayIndex state6 1) 123))) (booleanAnd (booleanAnd (intEq (arrayLength (result7).yes) 0) (intEq (arrayLength (result7).no) 0)) (booleanAnd (intEq (arrayIndex state7 0) 0) (intEq (arrayIndex state7 1) 0)))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength result8) 2) (intEq (arrayIndex result8 0) 11)) (booleanAnd (intEq (arrayIndex result8 1) 22) (intEq (arrayIndex state8 0) 2))) (booleanAnd (booleanAnd (intEq (arrayIndex state8 1) 12) (intEq (arrayLength result9) 1)) (booleanAnd (intEq (arrayIndex result9 0) 11) (booleanAnd (intEq (arrayIndex state9 0) 1) (intEq (arrayIndex state9 1) 1)))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength result10) 0) (intEq (arrayIndex state10 0) 0)) (booleanAnd (intEq (arrayIndex state10 1) 0) (intEq (arrayLength result11) 0))) (booleanAnd (booleanAnd (intEq (arrayIndex state11 0) 0) (intEq (arrayIndex state11 1) 0)) (booleanAnd (booleanEq result12 true) (booleanAnd (intEq (arrayIndex state12 0) 2) (intEq (arrayIndex state12 1) 12))))) (booleanAnd (booleanAnd (booleanAnd (booleanEq result13 false) (intEq (arrayIndex state13 0) 2)) (booleanAnd (intEq (arrayIndex state13 1) 31) (booleanEq result14 false))) (booleanAnd (booleanAnd (intEq (arrayIndex state14 0) 0) (intEq (arrayIndex state14 1) 0)) (booleanAnd (booleanEq result15 true) (booleanAnd (intEq (arrayIndex state15 0) 0) (intEq (arrayIndex state15 1) 0)))))))) then 42 else 1
