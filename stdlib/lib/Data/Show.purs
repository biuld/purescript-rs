-- | The `Show` class.
-- |
-- | `show` renders a value as text. The `Int`, `Number`, `Boolean`, `Char`,
-- | `String`, `Unit`, and `Array` instances are the ones the corpus actually
-- | applies `show` to. `Show Unit` lives here rather than in `Data.Unit`
-- | because that module is not part of this library yet; a record instance is
-- | not here because it needs `reflectSymbol`, which has no runtime.
-- |
-- | `Boolean`, `Int`, `Char`, `String`, `Unit`, and `Array` match the official
-- | spelling, including the `Char`/`String` escapes. `Number` uses the same
-- | shape as the official instance — decimal digits, `.0` on an integer token,
-- | scientific form outside `(1e-6, 1e21)` — but the digits come from the
-- | numeric primitives, not from a correctly rounded ECMAScript conversion.
-- | Integers whose absolute value is below `1e21`, powers of ten, and the
-- | dyadic fractions the digit loop reaches exactly match; other fractions
-- | print a deterministic expansion that can differ from `purs`.
module Data.Show
  ( class Show
  , show
  ) where

import Data.Semigroup ((<>))

-- | A type that can be rendered as text.
class Show a where
  show :: a -> String

instance showBoolean :: Show Boolean where
  show value = if value then "true" else "false"

instance showInt :: Show Int where
  show value =
    if intEq value minInt then "-2147483648"
    else if intLt value 0 then "-" <> showPositiveInt (intNeg value)
    else showPositiveInt value

instance showNumber :: Show Number where
  show value =
    if numberNe value value then "NaN"
    else if numberEq value positiveInfinity then "Infinity"
    else if numberEq value negativeInfinity then "-Infinity"
    else if numberLt value 0.0 then "-" <> showPositiveNumber (numberNeg value)
    else showPositiveNumber value

instance showChar :: Show Char where
  show value = "'" <> escapeCode (charToInt value) false false <> "'"

instance showString :: Show String where
  show value = "\"" <> escapeBytes (stringToBytes value) 0 <> "\""

instance showUnit :: Show Unit where
  show _ = "unit"

instance showArray :: Show a => Show (Array a) where
  show value = "[" <> showElements value 0 <> "]"

minInt :: Int
minInt = (0 - 2147483647) - 1

positiveInfinity :: Number
positiveInfinity = numberDiv 1.0 0.0

negativeInfinity :: Number
negativeInfinity = numberDiv (numberNeg 1.0) 0.0

showPositiveInt :: Int -> String
showPositiveInt value =
  if intLt value 10 then digit value
  else showPositiveInt (intDiv value 10) <> digit (intMod value 10)

digit :: Int -> String
digit value = bytesToString [intAdd 48 value]

-- | A non-negative `Number`. Integers below `1e21` print as decimal digits
-- | plus `.0`. Smaller magnitudes print as a fixed expansion. The rest use
-- | scientific form, which is what the official `Show Number` does past
-- | those thresholds.
showPositiveNumber :: Number -> String
showPositiveNumber value =
  if numberEq value 0.0 then "0.0"
  else if numberGe value 1.0e21 then showScientific value
  else if numberLt value 1.0e-6 then showScientific value
  else if numberEq value (floorNumber value) then showIntegerNumber value <> ".0"
  else showMixed value

showMixed :: Number -> String
showMixed value =
  let
    whole = floorNumber value
    fraction = numberSub value whole
    wholeText = if numberEq whole 0.0 then "0" else showIntegerNumber whole
    fractionText = digitsToString (trimZeros (collectFraction fraction 17 [])) 0
  in
    if intEq (arrayLength (stringToBytes fractionText)) 0 then wholeText <> ".0"
    else wholeText <> "." <> fractionText

showScientific :: Number -> String
showScientific value =
  let
    exponent = powerOfTen value 0
    mantissa = scaleToUnit value exponent
  in
    showMantissa mantissa <> showExponent exponent

showMantissa :: Number -> String
showMantissa value =
  let
    whole = numberToInt (floorNumber value)
    fraction = numberSub value (intToNumber whole)
    fractionText = digitsToString (trimZeros (collectFraction fraction 17 [])) 0
  in
    if intEq (arrayLength (stringToBytes fractionText)) 0 then showPositiveInt whole
    else showPositiveInt whole <> "." <> fractionText

showExponent :: Int -> String
showExponent value =
  if intLt value 0 then "e-" <> showPositiveInt (intNeg value)
  else "e+" <> showPositiveInt value

powerOfTen :: Number -> Int -> Int
powerOfTen value exponent =
  if numberEq value 0.0 then exponent
  else if numberGe value 10.0 then powerOfTen (numberDiv value 10.0) (intAdd exponent 1)
  else if numberLt value 1.0 then powerOfTen (numberMul value 10.0) (exponent - 1)
  else exponent

scaleToUnit :: Number -> Int -> Number
scaleToUnit value exponent =
  if intEq exponent 0 then value
  else if intGt exponent 0 then scaleToUnit (numberDiv value 10.0) (exponent - 1)
  else scaleToUnit (numberMul value 10.0) (intAdd exponent 1)

-- | Floor of a non-negative number. Groups of nine digits stay inside `Int`,
-- | so a value past `2^31` still floors without a wider primitive.
floorNumber :: Number -> Number
floorNumber value =
  if numberLt value 1000000000.0 then intToNumber (numberToInt value)
  else
    let
      billions = floorNumber (numberDiv value 1000000000.0)
      base = numberMul billions 1000000000.0
      low = numberToInt (numberSub value base)
    in
      numberAdd base (intToNumber low)

showIntegerNumber :: Number -> String
showIntegerNumber value =
  if numberLt value 1000000000.0 then showPositiveInt (numberToInt value)
  else
    let
      billions = floorNumber (numberDiv value 1000000000.0)
      base = numberMul billions 1000000000.0
      low = numberToInt (numberSub value base)
    in
      showIntegerNumber billions <> pad9 low

pad9 :: Int -> String
pad9 value =
  if intLt value 10 then "00000000" <> showPositiveInt value
  else if intLt value 100 then "0000000" <> showPositiveInt value
  else if intLt value 1000 then "000000" <> showPositiveInt value
  else if intLt value 10000 then "00000" <> showPositiveInt value
  else if intLt value 100000 then "0000" <> showPositiveInt value
  else if intLt value 1000000 then "000" <> showPositiveInt value
  else if intLt value 10000000 then "00" <> showPositiveInt value
  else if intLt value 100000000 then "0" <> showPositiveInt value
  else showPositiveInt value

collectFraction :: Number -> Int -> Array Int -> Array Int
collectFraction fraction remaining digits =
  if intEq remaining 0 then digits
  else if numberEq fraction 0.0 then digits
  else
    let
      scaled = numberMul fraction 10.0
      digitValue = numberToInt scaled
      rest = numberSub scaled (intToNumber digitValue)
    in
      collectFraction rest (remaining - 1) (arrayAppend digits [digitValue])

trimZeros :: Array Int -> Array Int
trimZeros digits =
  let
    length = arrayLength digits
  in
    if intEq length 0 then digits
    else if intEq (arrayIndex digits (length - 1)) 0 then trimZeros (takePrefix digits (length - 1))
    else digits

takePrefix :: Array Int -> Int -> Array Int
takePrefix digits count = copyPrefix digits count 0

copyPrefix :: Array Int -> Int -> Int -> Array Int
copyPrefix digits count index =
  if intGe index count then []
  else arrayAppend [arrayIndex digits index] (copyPrefix digits count (intAdd index 1))

digitsToString :: Array Int -> Int -> String
digitsToString digits index =
  if intGe index (arrayLength digits) then ""
  else digit (arrayIndex digits index) <> digitsToString digits (intAdd index 1)

showElements :: forall a. Show a => Array a -> Int -> String
showElements values index =
  if intGe index (arrayLength values) then ""
  else if intEq index 0 then show (arrayIndex values index) <> showElements values (intAdd index 1)
  else "," <> show (arrayIndex values index) <> showElements values (intAdd index 1)

-- | The body of a `Char` or `String` escape, without the surrounding quotes.
-- | `ampersand` is true when a numeric escape must not swallow a following
-- | digit, which is the official `\&` rule.
escapeCode :: Int -> Boolean -> Boolean -> String
escapeCode code ampersand inString =
  if intEq code 7 then "\\a"
  else if intEq code 8 then "\\b"
  else if intEq code 12 then "\\f"
  else if intEq code 10 then "\\n"
  else if intEq code 13 then "\\r"
  else if intEq code 9 then "\\t"
  else if intEq code 11 then "\\v"
  else if intLt code 32 then "\\" <> showPositiveInt code <> emptyAmpersand ampersand
  else if intEq code 127 then "\\127" <> emptyAmpersand ampersand
  else if intEq code 92 then "\\\\"
  else if intEq code 34 then if inString then "\\\"" else utf8 code
  else if intEq code 39 then if inString then utf8 code else "\\'"
  else utf8 code

emptyAmpersand :: Boolean -> String
emptyAmpersand needed = if needed then "\\&" else ""

escapeBytes :: Array Int -> Int -> String
escapeBytes bytes index =
  if intGe index (arrayLength bytes) then ""
  else
    let
      decoded = decode bytes index
      code = arrayIndex decoded 0
      next = arrayIndex decoded 1
    in
      escapeCode code (nextIsDigit bytes next) true <> escapeBytes bytes next

nextIsDigit :: Array Int -> Int -> Boolean
nextIsDigit bytes index =
  if intGe index (arrayLength bytes) then false
  else
    let
      code = arrayIndex (decode bytes index) 0
    in
      if intLt code 48 then false else intLe code 57

-- | One Unicode scalar and the index of the byte after it. The bytes are
-- | well-formed UTF-8 because they came from `stringToBytes`.
decode :: Array Int -> Int -> Array Int
decode bytes index =
  let
    first = arrayIndex bytes index
  in
    if intLt first 128 then [first, intAdd index 1]
    else if intLt first 224 then
      [ intAdd (intShl (intAnd first 31) 6) (continuation bytes (intAdd index 1))
      , intAdd index 2
      ]
    else if intLt first 240 then
      [ intAdd (intAdd (intShl (intAnd first 15) 12) (intShl (continuation bytes (intAdd index 1)) 6)) (continuation bytes (intAdd index 2))
      , intAdd index 3
      ]
    else
      [ intAdd (intAdd (intAdd (intShl (intAnd first 7) 18) (intShl (continuation bytes (intAdd index 1)) 12)) (intShl (continuation bytes (intAdd index 2)) 6)) (continuation bytes (intAdd index 3))
      , intAdd index 4
      ]

continuation :: Array Int -> Int -> Int
continuation bytes index = intAnd (arrayIndex bytes index) 63

utf8 :: Int -> String
utf8 code =
  if intLt code 128 then bytesToString [code]
  else if intLt code 2048 then bytesToString [intOr 192 (intShr code 6), intOr 128 (intAnd code 63)]
  else if intLt code 65536 then bytesToString [intOr 224 (intShr code 12), intOr 128 (intAnd (intShr code 6) 63), intOr 128 (intAnd code 63)]
  else bytesToString [intOr 240 (intShr code 18), intOr 128 (intAnd (intShr code 12) 63), intOr 128 (intAnd (intShr code 6) 63), intOr 128 (intAnd code 63)]
