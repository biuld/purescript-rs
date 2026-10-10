module Main where
import Golden as Golden
main =
  let
    r0 = Golden.arrayExtend (\suffix -> intAdd (arrayIndex suffix 0) (arrayLength suffix)) [10, 20, 30]
    r1 = Golden.arrayExtend (\suffix -> numberAdd (intToNumber (arrayIndex suffix 0)) 0.5) [4, 8]
    r2 = Golden.arrayExtend (\suffix -> { value: intAdd ((arrayIndex suffix 0).value) 2 }) [{ value: 40 }]
    r3 = Golden.arrayExtend (\suffix -> [arrayLength suffix, arrayIndex (arrayIndex suffix 0) 0]) [[1], [2]]
    r4 = Golden.arrayExtend (\suffix -> if intEq (arrayLength suffix) 2 then "λ" else "😀") [1, 2]
    r5 = Golden.arrayExtend (\suffix -> arrayIndex ([] :: Array Int) 0) []
    closures = Golden.arrayExtend (\suffix -> \n -> intAdd (arrayIndex suffix 0) n) [40, 41]
    counter = arrayFill 1 0
    bump suffix = let first = arrayWrite counter 0 (intAdd (arrayIndex counter 0) 1) in intAdd (arrayIndex first 0) (arrayLength suffix)
    counted = Golden.arrayExtend bump [10, 20, 30]
    sourceArray = [1, 2, 3]
    scribble suffix = let first = arrayWrite suffix 0 (intAdd (arrayIndex suffix 0) 100) in arrayIndex first 0
    scribbled = Golden.arrayExtend scribble sourceArray
  in if (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength r0) 3) (intEq (arrayIndex r0 0) 13)) (booleanAnd (intEq (arrayIndex r0 1) 22) (booleanAnd (intEq (arrayIndex r0 2) 31) (intEq (arrayLength r1) 2)))) (booleanAnd (booleanAnd (numberEq (arrayIndex r1 0) 4.5) (numberEq (arrayIndex r1 1) 8.5)) (booleanAnd (intEq (arrayLength r2) 1) (booleanAnd (intEq ((arrayIndex r2 0)).value 42) (intEq (arrayLength r3) 2))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength (arrayIndex r3 0)) 2) (intEq (arrayIndex (arrayIndex r3 0) 0) 2)) (booleanAnd (intEq (arrayIndex (arrayIndex r3 0) 1) 1) (booleanAnd (intEq (arrayLength (arrayIndex r3 1)) 2) (intEq (arrayIndex (arrayIndex r3 1) 0) 1)))) (booleanAnd (booleanAnd (intEq (arrayIndex (arrayIndex r3 1) 1) 2) (intEq (arrayLength r4) 2)) (booleanAnd (intEq (arrayLength (stringToBytes (arrayIndex r4 0))) 2) (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 0)) 0) 206) (intEq (arrayIndex (stringToBytes (arrayIndex r4 0)) 1) 187)))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength (stringToBytes (arrayIndex r4 1))) 4) (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 0) 240)) (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 1) 159) (booleanAnd (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 2) 152) (intEq (arrayIndex (stringToBytes (arrayIndex r4 1)) 3) 128)))) (booleanAnd (booleanAnd (intEq (arrayLength r5) 0) (intEq ((arrayIndex closures 0) 2) 42)) (booleanAnd (intEq ((arrayIndex closures 1) 1) 42) (booleanAnd (intEq (arrayLength counted) 3) (intEq (arrayIndex counted 0) 4))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex counted 1) 4) (intEq (arrayIndex counted 2) 4)) (booleanAnd (intEq (arrayIndex counter 0) 3) (booleanAnd (intEq (arrayLength scribbled) 3) (intEq (arrayIndex scribbled 0) 101)))) (booleanAnd (booleanAnd (intEq (arrayIndex scribbled 1) 102) (booleanAnd (intEq (arrayIndex scribbled 2) 103) (intEq (arrayLength sourceArray) 3))) (booleanAnd (intEq (arrayIndex sourceArray 0) 1) (booleanAnd (intEq (arrayIndex sourceArray 1) 2) (intEq (arrayIndex sourceArray 2) 3))))))) then 42 else 1
