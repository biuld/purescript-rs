module Golden where
foreign import "psrs:intrinsic#intToNumber" toNumber :: Int -> Number
foreign import "psrs:intrinsic#intAnd" and :: Int -> Int -> Int
foreign import "psrs:intrinsic#intOr" or :: Int -> Int -> Int
foreign import "psrs:intrinsic#intXor" xor :: Int -> Int -> Int
foreign import "psrs:intrinsic#intShl" shl :: Int -> Int -> Int
foreign import "psrs:intrinsic#intShr" shr :: Int -> Int -> Int
foreign import "psrs:intrinsic#intZshr" zshr :: Int -> Int -> Int
foreign import "psrs:intrinsic#intComplement" complement :: Int -> Int
foreign import "psrs:intrinsic#booleanEq" eqBooleanImpl :: Boolean -> Boolean -> Boolean
foreign import "psrs:intrinsic#intEq" eqIntImpl :: Int -> Int -> Boolean
foreign import "psrs:intrinsic#numberEq" eqNumberImpl :: Number -> Number -> Boolean
foreign import "psrs:intrinsic#charEq" eqCharImpl :: Char -> Char -> Boolean
foreign import "psrs:intrinsic#intSub" intSub :: Int -> Int -> Int
foreign import "psrs:intrinsic#numberSub" numSub :: Number -> Number -> Number
foreign import "psrs:intrinsic#intAdd" intAdd :: Int -> Int -> Int
foreign import "psrs:intrinsic#numberAdd" numAdd :: Number -> Number -> Number
foreign import "psrs:intrinsic#numberMul" numMul :: Number -> Number -> Number
foreign import "psrs:intrinsic#booleanAnd" boolConj :: Boolean -> Boolean -> Boolean
foreign import "psrs:intrinsic#booleanOr" boolDisj :: Boolean -> Boolean -> Boolean
foreign import "psrs:intrinsic#booleanNot" boolNot :: Boolean -> Boolean
