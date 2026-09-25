use crate::TargetCapabilities;
use crate::abi::WasiRegistry;

type DisablePackage = fn(&mut TargetCapabilities);

#[test]
fn current_wasi_service_packages_have_independent_profile_gates() {
    let cases: [(&str, &str, DisablePackage); 4] = [
        ("wasi:cli/stdout", "get-stdout", |target| {
            target.wasi_cli = false;
        }),
        ("wasi:io/poll", "[method]pollable.ready", |target| {
            target.wasi_io = false;
        }),
        ("wasi:clocks/monotonic-clock", "now", |target| {
            target.wasi_clocks = false;
        }),
        ("wasi:random/random", "get-random-bytes", |target| {
            target.wasi_random = false;
        }),
    ];

    for (interface, function, disable_package) in cases {
        let mut target = TargetCapabilities::default();
        disable_package(&mut target);
        let mut registry = WasiRegistry::load_with_capabilities(target)
            .expect("WASI WIT should load with one service disabled");
        let disabled = registry
            .import(interface, function)
            .expect("the disabled service declaration should still resolve");
        assert!(disabled.unsupported.as_deref().is_some_and(|reason| {
            reason.contains("disabled by the selected target capability profile")
        }));

        let (control_interface, control_function) = if interface == "wasi:cli/stdout" {
            ("wasi:random/random", "get-random-bytes")
        } else {
            ("wasi:cli/stdout", "get-stdout")
        };
        let control = registry
            .import(control_interface, control_function)
            .expect("an unrelated enabled service should resolve");
        assert!(control.unsupported.is_none());
    }
}
