//! Trap-based panic behavior shared by both executable Wasm artifacts.

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
