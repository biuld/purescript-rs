module Main where
import Golden as Golden
main =
  let
    r0 = Golden.arrayApply [((\offset -> \x -> intAdd offset x) 1), (\x -> intMul x 2)] [10, 20]
    r1 = Golden.arrayApply [(\x -> numberAdd (intToNumber x) 0.5)] [4, 8]
    r2 = Golden.arrayApply [(\x -> { value: intAdd x 2 })] [40]
    r3 = Golden.arrayApply [(\x -> [x, intAdd x 1])] [5, 9]
    r4 = Golden.arrayApply [(\x -> if intEq x 1 then "λ" else "😀")] [1, 2]
    r5 = Golden.arrayApply [(\x -> arrayIndex ([] :: Array Int) x)] []
    r6 = Golden.arrayApply ([] :: Array (Int -> Int)) [1, 2]
    curried = Golden.arrayApply [(\x -> \y -> intAdd x y)] [40]
  in if (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength r0) 4) (booleanAnd (intEq (arrayIndex r0 0) 11) (intEq (arrayIndex r0 1) 21))) (booleanAnd (booleanAnd (intEq (arrayIndex r0 2) 20) (intEq (arrayIndex r0 3) 40)) (booleanAnd (intEq (arrayLength r1) 2) (numberEq (arrayIndex r1 0) 4.5)))) (booleanAnd (booleanAnd (numberEq (arrayIndex r1 1) 8.5) (booleanAnd (intEq (arrayLength r2) 1) (intEq ((arrayIndex r2 0)).value 42))) (booleanAnd (booleanAnd (intEq (arrayLength r3) 2) (intEq (arrayLength (arrayIndex r3 0)) 2)) (booleanAnd (intEq (arrayIndex (arrayIndex r3 0) 0) 5) (intEq (arrayIndex (arrayIndex r3 0) 1) 6))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength (arrayIndex r3 1)) 2) (booleanAnd (intEq (arrayIndex (arrayIndex r3 1) 0) 9) (intEq (arrayIndex (arrayIndex r3 1) 1) 10))) (booleanAnd (booleanAnd (intEq (arrayLength r4) 2) (intEq (arrayLength (stringToBytes (arrayIndex r4 0))) 2)) (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 0)) 0) 206) (intEq (arrayIndex (stringToBytes (arrayIndex r4 0)) 1) 187)))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength (stringToBytes (arrayIndex r4 1))) 4) (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 0) 240)) (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 1) 159) (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 2) 152))) (booleanAnd (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 3) 128) (intEq (arrayLength r5) 0)) (booleanAnd (intEq (arrayLength r6) 0) (intEq ((arrayIndex curried 0) 2) 42)))))) then 42 else 1
