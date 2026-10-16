use super::{Resolver, ast, hir};

impl Resolver {
    /// A pseudo-module combines import scopes, so ambiguity is checked by
    /// member identity in its namespace, as it is for a qualified reference.
    pub(super) fn check_reexport_scope(
        &mut self,
        name: &ast::Name,
        imports: &[hir::Import],
    ) -> bool {
        let mut valid = true;
        for import in imports {
            for symbol in &import.symbols {
                let qualified = format!("{}.{}", name.text, symbol.external_name);
                valid &= self.lookup_global(&qualified, name.span).is_some();
            }
            for imported in &import.types {
                let qualified = format!("{}.{}", name.text, imported.name);
                valid &= self
                    .lookup_qualified_type(&qualified, &name.text, &imported.name, name.span)
                    .is_some();
            }
        }
        valid
    }
}
