use core::panic::PanicInfo;
use core::sync::atomic::{self, Ordering};
use log::error;

#[inline(never)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    //Disable all interrupts
    cortex_m::interrupt::disable();

    //Try to log the panic
    error!("{}", info);

    //This is my life now
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
        cortex_m::asm::nop();
    }
}
