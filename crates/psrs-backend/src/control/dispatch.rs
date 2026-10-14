//! Required dispatch alternatives of the control machine, independent of IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Carrier {
    Computation,
    Queue,
    Context,
    Reverse,
}

impl Carrier {
    pub(crate) fn tags(self) -> &'static [i32] {
        match self {
            Self::Computation => &[0, 1, 2, 3, 4, 5],
            Self::Queue => &[0, 1, 2, 3],
            Self::Context => &[0, 1],
            Self::Reverse => &[0, 1, 3],
        }
    }

    pub(crate) fn verify(self, tags: impl Iterator<Item = i32>) -> Result<(), &'static str> {
        let mut actual = tags.collect::<Vec<_>>();
        actual.sort_unstable();
        if actual != self.tags() {
            return Err("control dispatch must cover every required alternative exactly once");
        }
        Ok(())
    }
}
