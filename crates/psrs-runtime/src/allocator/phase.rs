//! Lazy provider phases. Reentry while initializing or dispatching traps.

pub const UNINITIALIZED: u8 = 0;
pub const BUSY: u8 = 1;
pub const READY: u8 = 2;

/// Moves `Uninitialized` to `Busy`. A second entry while busy is reentry.
pub fn enter(phase: &mut u8) -> Result<bool, ()> {
    match *phase {
        UNINITIALIZED => {
            *phase = BUSY;
            Ok(true)
        }
        READY => {
            *phase = BUSY;
            Ok(false)
        }
        _ => Err(()),
    }
}

pub fn ready(phase: &mut u8) {
    *phase = READY;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_call_initializes_and_reentry_traps() {
        let mut phase = UNINITIALIZED;
        assert_eq!(enter(&mut phase), Ok(true));
        assert_eq!(enter(&mut phase), Err(()));
        ready(&mut phase);
        assert_eq!(enter(&mut phase), Ok(false));
        ready(&mut phase);
        assert_eq!(phase, READY);
    }
}
