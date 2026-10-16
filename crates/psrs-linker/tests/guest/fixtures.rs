use wit_parser::Resolve;
pub const INTERFACE: &str = "test:lib/api@1.0.0";

const WIT: &str = r#"
package test:lib@1.0.0;
interface api {
    echo: func(value: string) -> string;
    bytes: func() -> result<list<u8>, u32>;
    posts: func() -> u32;
    drops: func() -> u32;
    resource token {
        constructor(value: u32);
        get: func() -> u32;
    }
}
world provider { export api; }
world application { import api; export wasi:cli/run@0.2.12; }
"#;

fn encode(wit: &str, world: &str, wat: &str) -> Vec<u8> {
    let mut resolve = Resolve::default();
    for source in psrs_runtime::WASI_WIT {
        resolve.push_str(source.path, source.contents).unwrap();
    }
    let package = resolve.push_str("fixture.wit", wit).unwrap();
    let world = resolve.packages[package].worlds[world];
    let mut bytes = wat::parse_str(wat).unwrap();
    wit_component::embed_component_metadata(
        &mut bytes,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .unwrap();
    wit_component::ComponentEncoder::default()
        .module(&bytes)
        .unwrap()
        .validate(true)
        .encode()
        .unwrap()
}

pub fn components() -> (Vec<u8>, Vec<u8>) {
    (
        encode(WIT, "application", APPLICATION),
        encode(WIT, "provider", PROVIDER),
    )
}

pub fn incompatible_provider() -> Vec<u8> {
    // A valid provider with the same name but a different checked echo shape.
    let wit = WIT.replace(
        "echo: func(value: string) -> string;",
        "echo: func(value: u32) -> u32;",
    );
    let wat = PROVIDER.replace("(param $p i32) (param $n i32) (result i32)\n      i32.const 0 local.get $p i32.store\n      i32.const 4 local.get $n i32.store\n      i32.const 0", "(param $p i32) (result i32) local.get $p");
    let wat = wat.replace(
        "(func (export \"cabi_post_test:lib/api@1.0.0#echo\") (param i32) call $post)",
        "",
    );
    encode(&wit, "provider", &wat)
}

const PROVIDER: &str = r#"
(module
  (import "[export]test:lib/api@1.0.0" "[resource-new]token" (func $new (param i32) (result i32)))
  (memory (export "memory") 2)
  (global $posts (mut i32) (i32.const 0))
  (global $drops (mut i32) (i32.const 0))
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) i32.const 4096)
  (func (export "test:lib/api@1.0.0#echo") (param $p i32) (param $n i32) (result i32)
      i32.const 0 local.get $p i32.store
      i32.const 4 local.get $n i32.store
      i32.const 0)
  (func (export "test:lib/api@1.0.0#bytes") (result i32)
      i32.const 256 i32.const 104 i32.store8
      i32.const 257 i32.const 105 i32.store8
      i32.const 258 i32.const 33 i32.store8
      i32.const 0 i32.const 0 i32.store
      i32.const 4 i32.const 256 i32.store
      i32.const 8 i32.const 3 i32.store
      i32.const 0)
  (func $post
      global.get $posts i32.const 1 i32.add global.set $posts
      i32.const 4096 i32.const 0 i32.const 32 memory.fill
      i32.const 256 i32.const 0 i32.const 3 memory.fill)
  (func (export "cabi_post_test:lib/api@1.0.0#echo") (param i32) call $post)
  (func (export "cabi_post_test:lib/api@1.0.0#bytes") (param i32) call $post)
  (func (export "test:lib/api@1.0.0#posts") (result i32) global.get $posts)
  (func (export "test:lib/api@1.0.0#drops") (result i32) global.get $drops)
  (func (export "test:lib/api@1.0.0#[constructor]token") (param i32) (result i32) local.get 0 call $new)
  (func (export "test:lib/api@1.0.0#[method]token.get") (param i32) (result i32) local.get 0)
  (func (export "test:lib/api@1.0.0#[dtor]token") (param i32)
      local.get 0 i32.const 39 i32.ne if unreachable end
      global.get $drops i32.const 1 i32.add global.set $drops))
"#;

const APPLICATION: &str = r#"
(module
  (import "test:lib/api@1.0.0" "echo" (func $echo (param i32 i32 i32)))
  (import "test:lib/api@1.0.0" "bytes" (func $bytes (param i32)))
  (import "test:lib/api@1.0.0" "posts" (func $posts (result i32)))
  (import "test:lib/api@1.0.0" "drops" (func $drops (result i32)))
  (import "test:lib/api@1.0.0" "[constructor]token" (func $new (param i32) (result i32)))
  (import "test:lib/api@1.0.0" "[method]token.get" (func $get (param i32) (result i32)))
  (import "test:lib/api@1.0.0" "[resource-drop]token" (func $drop (param i32)))
  (memory (export "memory") 2)
  (data (i32.const 1024) "firsté")
  (data (i32.const 2048) "second🙂")
  (global $bump (mut i32) (i32.const 4096))
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) (local $p i32)
      global.get $bump local.tee $p local.get 3 i32.add global.set $bump local.get $p)
  (func $eq (param $a i32) (param $b i32) (param $n i32) (local $i i32)
      block loop
          local.get $i local.get $n i32.eq br_if 1
          local.get $a local.get $i i32.add i32.load8_u
          local.get $b local.get $i i32.add i32.load8_u i32.ne if unreachable end
          local.get $i i32.const 1 i32.add local.set $i br 0
      end end)
  (func (export "wasi:cli/run@0.2.12#run") (result i32) (local $token i32)
      i32.const 1024 i32.const 7 i32.const 64 call $echo
      i32.const 2048 i32.const 10 i32.const 80 call $echo
      i32.const 68 i32.load i32.const 7 i32.ne if unreachable end
      i32.const 84 i32.load i32.const 10 i32.ne if unreachable end
      i32.const 64 i32.load i32.const 1024 i32.const 7 call $eq
      i32.const 80 i32.load i32.const 2048 i32.const 10 call $eq
      i32.const 128 call $bytes
      i32.const 128 i32.load if unreachable end
      i32.const 136 i32.load i32.const 3 i32.ne if unreachable end
      i32.const 132 i32.load i32.load8_u i32.const 104 i32.ne if unreachable end
      i32.const 132 i32.load i32.load8_u offset=2 i32.const 33 i32.ne if unreachable end
      call $posts i32.const 3 i32.ne if unreachable end
      i32.const 39 call $new local.tee $token call $get i32.const 39 i32.ne if unreachable end
      local.get $token call $drop
      call $drops i32.const 1 i32.ne if unreachable end
      i32.const 0))
"#;

pub fn forwarding_component(import: &str, export: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(component
        (type $api (instance (export "value" (func (result u32)))))
        (import "{import}" (instance $value (type $api)))
        (export "{export}" (instance $value)))"#
    ))
    .unwrap()
}

pub fn constant_component(export: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(component
        (core module $m (func (export "value") (result i32) i32.const 42))
        (core instance $m (instantiate $m))
        (func $value (result u32) (canon lift (core func $m "value")))
        (instance $api (export "value" (func $value)))
        (export "{export}" (instance $api)))"#
    ))
    .unwrap()
}

pub fn definition_only_package() -> Vec<u8> {
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str(
            "definitions.wit",
            r#"
        package test:lib@1.0.0;
        interface api { value: func() -> u32; }
    "#,
        )
        .unwrap();
    wit_component::encode(&resolve, package).unwrap()
}
