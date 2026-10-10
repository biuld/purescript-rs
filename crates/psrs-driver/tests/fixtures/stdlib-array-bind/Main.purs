module Main where
import Golden as Golden
main =
  let
    r0 = Golden.arrayBind [4, 0, 9] ((\offset -> \x -> if intEq x 0 then [] else [intAdd offset x, intMul x 2]) 1)
    r1 = Golden.arrayBind [4, 8] (\x -> [numberAdd (intToNumber x) 0.5])
    r2 = Golden.arrayBind [40] (\x -> [{ value: intAdd x 2 }])
    r3 = Golden.arrayBind [1, 2] (\x -> [[x], [intAdd x 10]])
    r4 = Golden.arrayBind [1, 2] (\x -> [if intEq x 1 then "λ" else "😀"])
    r5 = Golden.arrayBind [] (\x -> [arrayIndex ([] :: Array Int) x])
    r6 = Golden.arrayBind [1, 2] (\x -> ([] :: Array Int))
    curried = Golden.arrayBind [40] (\x -> [\y -> intAdd x y])
    shared = arrayFill 2 0
    mutate x = let first = arrayWrite shared 0 x in arrayWrite first 1 (intAdd (arrayIndex first 1) 1)
    observed = Golden.arrayBind [10,20] mutate
  in if (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength r0) 4) (intEq (arrayIndex r0 0) 5)) (booleanAnd (intEq (arrayIndex r0 1) 8) (intEq (arrayIndex r0 2) 10))) (booleanAnd (booleanAnd (intEq (arrayIndex r0 3) 18) (intEq (arrayLength r1) 2)) (booleanAnd (numberEq (arrayIndex r1 0) 4.5) (booleanAnd (numberEq (arrayIndex r1 1) 8.5) (intEq (arrayLength r2) 1))))) (booleanAnd (booleanAnd (booleanAnd (intEq ((arrayIndex r2 0)).value 42) (intEq (arrayLength r3) 4)) (booleanAnd (intEq (arrayLength (arrayIndex r3 0)) 1) (intEq (arrayIndex (arrayIndex r3 0) 0) 1))) (booleanAnd (booleanAnd (intEq (arrayLength (arrayIndex r3 1)) 1) (intEq (arrayIndex (arrayIndex r3 1) 0) 11)) (booleanAnd (intEq (arrayLength (arrayIndex r3 2)) 1) (booleanAnd (intEq (arrayIndex (arrayIndex r3 2) 0) 2) (intEq (arrayLength (arrayIndex r3 3)) 1)))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex (arrayIndex r3 3) 0) 12) (intEq (arrayLength r4) 2)) (booleanAnd (intEq (arrayLength (stringToBytes (arrayIndex r4 0))) 2) (intEq (arrayIndex (stringToBytes (arrayIndex r4 0)) 0) 206))) (booleanAnd (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 0)) 1) 187) (intEq (arrayLength (stringToBytes (arrayIndex r4 1))) 4)) (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 0) 240) (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 1) 159) (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 2) 152))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 3) 128) (intEq (arrayLength r5) 0)) (booleanAnd (intEq (arrayLength r6) 0) (booleanAnd (intEq ((arrayIndex curried 0) 2) 42) (intEq (arrayLength observed) 4)))) (booleanAnd (booleanAnd (intEq (arrayIndex observed 0) 10) (intEq (arrayIndex observed 1) 1)) (booleanAnd (intEq (arrayIndex observed 2) 20) (booleanAnd (intEq (arrayIndex observed 3) 2) (intEq (arrayIndex shared 1) 2))))))) then 42 else 1
