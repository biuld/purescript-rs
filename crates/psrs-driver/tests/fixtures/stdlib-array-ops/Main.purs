module Main where
import PSRS.Array as A

arrayFoldr :: forall a b. (a -> b -> b) -> b -> Array a -> b
arrayFoldr f b xs = go 0
  where
  go i = if intLt i (A.lengthImpl xs) then f (A.unsafeIndexImpl xs i) (go (intAdd i 1)) else b

c0 = A.rangeImpl 2 5

c1 = A.rangeImpl 5 2

c2 = A.rangeImpl 3 3

c3 = A.replicateImpl 3 7

c4 = A.replicateImpl 0 7

c5 = A.fromFoldableImpl arrayFoldr [1, 2, 3]

c6 = A.lengthImpl [1, 2, 3]

c7 = A.unconsImpl (\_ -> 0) (\x xs -> intAdd x (A.lengthImpl xs)) [1, 2, 3]

c8 = A.unconsImpl (\_ -> 42) (\_ _ -> 0) ([] :: Array Int)

c9 = A.reverseImpl [1, 2, 3]

c10 = A.reverseImpl ([] :: Array Int)

c11 = A.concatImpl [[1, 2], [], [3]]

c12 = A.concatImpl ([] :: Array (Array Int))

c13 = A.filterImpl (\x -> intLt 1 x) [0, 2, 1, 3]

c14 = A.filterImpl (\x -> intLt 9 x) [0, 2, 1, 3]

c15 = A.partitionImpl (\x -> intLt 1 x) [0, 2, 1, 3]

c16 = A.scanlImpl (\acc x -> intAdd acc x) 0 [1, 2, 3]

c17 = A.scanlImpl (\acc x -> intAdd acc x) 0 ([] :: Array Int)

c18 = A.scanrImpl (\x acc -> intAdd x acc) 0 [1, 2, 3]

c19 = A.sortByImpl (\x y -> intSub x y) (\c -> c) [3, 1, 2]

c20 = A.sortByImpl (\x y -> intSub x.key y.key) (\c -> c) [{ key: 1, id: 0 }, { key: 1, id: 1 }, { key: 0, id: 2 }]

c21 = A.sliceImpl 1 3 [0, 1, 2, 3]

c22 = A.sliceImpl (intSub 0 2) 4 [0, 1, 2, 3]

c23 = A.sliceImpl 3 1 [0, 1, 2, 3]

c24 = A.zipWithImpl (\x y -> intAdd x y) [1, 2, 3] [10, 20]

c25 = A.anyImpl (\x -> intLt 2 x) [0, 1, 2, 3]

c26 = A.anyImpl (\x -> intLt 2 x) [0, 1]

c27 = A.allImpl (\x -> intLt 0 x) [1, 2]

c28 = A.allImpl (\x -> intLt 0 x) [1, 0]

c29 = A.unsafeIndexImpl [1, 2, 3] 1

main =
  if (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength c0) 4) (intEq (arrayIndex c0 0) 2)) (booleanAnd (intEq (arrayIndex c0 1) 3) (intEq (arrayIndex c0 2) 4))) (booleanAnd (booleanAnd (intEq (arrayIndex c0 3) 5) (intEq (arrayLength c1) 4)) (booleanAnd (intEq (arrayIndex c1 0) 5) (booleanAnd (intEq (arrayIndex c1 1) 4) (intEq (arrayIndex c1 2) 3))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex c1 3) 2) (intEq (arrayLength c2) 1)) (booleanAnd (intEq (arrayIndex c2 0) 3) (booleanAnd (intEq (arrayLength c3) 3) (intEq (arrayIndex c3 0) 7)))) (booleanAnd (booleanAnd (intEq (arrayIndex c3 1) 7) (intEq (arrayIndex c3 2) 7)) (booleanAnd (intEq (arrayLength c4) 0) (booleanAnd (intEq (arrayLength c5) 3) (intEq (arrayIndex c5 0) 1)))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex c5 1) 2) (intEq (arrayIndex c5 2) 3)) (booleanAnd (intEq c6 3) (booleanAnd (intEq c7 3) (intEq c8 42)))) (booleanAnd (booleanAnd (intEq (arrayLength c9) 3) (intEq (arrayIndex c9 0) 3)) (booleanAnd (intEq (arrayIndex c9 1) 2) (booleanAnd (intEq (arrayIndex c9 2) 1) (intEq (arrayLength c10) 0))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength c11) 3) (intEq (arrayIndex c11 0) 1)) (booleanAnd (intEq (arrayIndex c11 1) 2) (booleanAnd (intEq (arrayIndex c11 2) 3) (intEq (arrayLength c12) 0)))) (booleanAnd (booleanAnd (intEq (arrayLength c13) 2) (intEq (arrayIndex c13 0) 2)) (booleanAnd (intEq (arrayIndex c13 1) 3) (booleanAnd (intEq (arrayLength c14) 0) (intEq (arrayLength (c15).yes) 2))))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex (c15).yes 0) 2) (intEq (arrayIndex (c15).yes 1) 3)) (booleanAnd (intEq (arrayLength (c15).no) 2) (booleanAnd (intEq (arrayIndex (c15).no 0) 0) (intEq (arrayIndex (c15).no 1) 1)))) (booleanAnd (booleanAnd (intEq (arrayLength c16) 3) (intEq (arrayIndex c16 0) 1)) (booleanAnd (intEq (arrayIndex c16 1) 3) (booleanAnd (intEq (arrayIndex c16 2) 6) (intEq (arrayLength c17) 0))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayLength c18) 3) (intEq (arrayIndex c18 0) 6)) (booleanAnd (intEq (arrayIndex c18 1) 5) (booleanAnd (intEq (arrayIndex c18 2) 3) (intEq (arrayLength c19) 3)))) (booleanAnd (booleanAnd (intEq (arrayIndex c19 0) 1) (intEq (arrayIndex c19 1) 2)) (booleanAnd (intEq (arrayIndex c19 2) 3) (booleanAnd (intEq (arrayLength c20) 3) (intEq ((arrayIndex c20 0)).key 0)))))) (booleanAnd (booleanAnd (booleanAnd (booleanAnd (intEq ((arrayIndex c20 0)).id 2) (intEq ((arrayIndex c20 1)).key 1)) (booleanAnd (intEq ((arrayIndex c20 1)).id 0) (booleanAnd (intEq ((arrayIndex c20 2)).key 1) (intEq ((arrayIndex c20 2)).id 1)))) (booleanAnd (booleanAnd (intEq (arrayLength c21) 2) (intEq (arrayIndex c21 0) 1)) (booleanAnd (intEq (arrayIndex c21 1) 2) (booleanAnd (intEq (arrayLength c22) 2) (intEq (arrayIndex c22 0) 2))))) (booleanAnd (booleanAnd (booleanAnd (intEq (arrayIndex c22 1) 3) (intEq (arrayLength c23) 0)) (booleanAnd (intEq (arrayLength c24) 2) (booleanAnd (intEq (arrayIndex c24 0) 11) (intEq (arrayIndex c24 1) 22)))) (booleanAnd (booleanAnd (booleanEq c25 true) (booleanEq c26 false)) (booleanAnd (booleanEq c27 true) (booleanAnd (booleanEq c28 false) (intEq c29 2)))))))) then 42 else 1
