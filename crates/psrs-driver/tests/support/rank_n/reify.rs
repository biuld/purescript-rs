pub const EQUATIONS: &str = r#"module Main where
data First = First
data Second = Second
data Proxy a = Proxy
class Tag a where
  tag :: Proxy a -> Int
instance tagFirst :: Tag First where
  tag _ = 21
instance tagSecond :: Tag Second where
  tag _ = 42
reify :: forall r. Boolean -> (forall a. Tag a => Proxy a -> r) -> r
reify true f = f (Proxy :: Proxy First)
reify false f = f (Proxy :: Proxy Second)
main :: Int
main = case reify true tag, reify false tag of
  21, 42 -> 42
  _, _ -> 1
"#;

pub const MULTIPLE: &str = r#"module Main where
data First = First
data Second = Second
data Proxy a = Proxy
class Tag a where
  tag :: Proxy a -> Int
instance tagFirst :: Tag First where
  tag _ = 21
instance tagSecond :: Tag Second where
  tag _ = 42
reify :: forall r. Boolean -> (forall a. Tag a => Proxy a -> r) -> r
reify flag f = case flag, f of
  true, continuation -> continuation (Proxy :: Proxy First)
  false, continuation -> continuation (Proxy :: Proxy Second)
main :: Int
main = case reify true tag, reify false tag of
  21, 42 -> 42
  _, _ -> 1
"#;

pub const GUARDED: &str = r#"module Main where
data First = First
data Second = Second
data Proxy a = Proxy
class Tag a where
  tag :: Proxy a -> Int
instance tagFirst :: Tag First where
  tag _ = 21
instance tagSecond :: Tag Second where
  tag _ = 42
reify :: forall r. Boolean -> (forall a. Tag a => Proxy a -> r) -> r
reify true f | true = f (Proxy :: Proxy First)
reify false f | true = f (Proxy :: Proxy Second)
main :: Int
main = case reify true tag, reify false tag of
  21, 42 -> 42
  _, _ -> 1
"#;

pub const RECORD: &str = r#"module Main where
data First = First
data Second = Second
data Proxy a = Proxy
class Tag a where
  tag :: Proxy a -> Int
instance tagFirst :: Tag First where
  tag _ = 21
instance tagSecond :: Tag Second where
  tag _ = 42
reify :: forall r. Boolean -> (forall a. Tag a => Proxy a -> r) -> r
reify flag f = case { flag, f } of
  { flag: true, f: continuation } -> continuation (Proxy :: Proxy First)
  { flag: false, f: continuation } -> continuation (Proxy :: Proxy Second)
main :: Int
main = case reify true tag, reify false tag of
  21, 42 -> 42
  _, _ -> 1
"#;
