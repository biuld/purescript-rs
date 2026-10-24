//! Checked removal of type nodes unreachable from retained Core metadata.

use super::visit;
use crate::{Module, TypeId, VerifyError};

impl Module {
    /// Removes unused type-table nodes and renumbers every retained reference.
    /// Checks the entire input first, including unused nodes, so compaction
    /// cannot hide malformed IR. Quantifier identities and source spans stay.
    /// A rejected input leaves the module unchanged.
    pub fn compact_types(&mut self) -> Result<(), Vec<VerifyError>> {
        self.verify()?;
        let mut live = vec![false; self.types.len()];
        let mut work = Vec::new();
        visit::roots(self, &mut |id| work.push(*id));
        while let Some(id) = work.pop() {
            if std::mem::replace(&mut live[id.0 as usize], true) {
                continue;
            }
            let mut ty = self.types[id.0 as usize].clone();
            visit::children(&mut ty, &mut |child| work.push(*child));
        }
        let mut mapping = vec![None; live.len()];
        let mut next = 0;
        for (index, retained) in live.iter().enumerate() {
            if *retained {
                mapping[index] = Some(TypeId(next));
                next += 1;
            }
        }
        let mut remap = |id: &mut TypeId| {
            *id = mapping[id.0 as usize].expect("reachable Core type has a compacted identity");
        };
        visit::roots(self, &mut remap);
        self.types = std::mem::take(&mut self.types)
            .into_iter()
            .zip(live)
            .filter_map(|(mut ty, retained)| {
                retained.then(|| {
                    visit::children(&mut ty, &mut remap);
                    ty
                })
            })
            .collect();
        self.verify()
    }
}
