pub(super) fn core_main(wat: &str) -> &str {
    let start = wat
        .find("(core module $main")
        .expect("the component should contain the core module");
    let rest = &wat[start..];
    let mut depth = 0;
    for (index, character) in rest.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..=index];
                }
            }
            _ => {}
        }
    }
    panic!("the core module is unclosed");
}

pub(super) struct CoreFunc {
    index: u32,
    body: String,
}

pub(super) fn core_functions(core: &str) -> Vec<CoreFunc> {
    let bytes = core.as_bytes();
    let mut functions = Vec::new();
    let mut depth = 0;
    let mut index = 0;
    while index < bytes.len() {
        if depth == 1 && core[index..].starts_with("(func (;") {
            let number_start = index + "(func (;".len();
            let number_end = core[number_start..]
                .find(";)")
                .expect("a core func should carry an index")
                + number_start;
            let func_index = core[number_start..number_end]
                .parse()
                .expect("a core func index should be a decimal");
            let mut inner = 0;
            let mut end = index;
            let mut cursor = index;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'(' => inner += 1,
                    b')' => {
                        inner -= 1;
                        if inner == 0 {
                            end = cursor + 1;
                            break;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            functions.push(CoreFunc {
                index: func_index,
                body: core[index..end].to_string(),
            });
            index = end;
            continue;
        }
        match bytes[index] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    functions
}

pub(super) fn imported_func(core: &str, module: &str, name: &str) -> u32 {
    let needle = format!(r#"(import "{module}" "{name}" (func (;"#);
    let at = core
        .find(&needle)
        .unwrap_or_else(|| panic!("missing import {module}#{name}"));
    let after = &core[at + needle.len()..];
    let end = after.find(";)").expect("an import func index");
    after[..end]
        .parse()
        .expect("an import func index should be a decimal")
}

pub(super) fn exported_func(core: &str, name: &str) -> u32 {
    let needle = format!(r#"(export "{name}" (func "#);
    let at = core
        .find(&needle)
        .unwrap_or_else(|| panic!("missing export {name}"));
    let after = &core[at + needle.len()..];
    let end = after.find(')').expect("an export func index");
    after[..end]
        .trim()
        .parse()
        .expect("an export func index should be a decimal")
}

fn direct_calls(body: &str) -> Vec<u32> {
    let mut calls = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("call ") {
        let boundary = at == 0 || !rest.as_bytes()[at - 1].is_ascii_alphanumeric();
        let after = &rest[at + "call ".len()..];
        let digits = after
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if boundary && !digits.is_empty() {
            calls.push(digits.parse().expect("call operand"));
        }
        rest = &rest[at + "call ".len()..];
    }
    calls
}

fn ref_func_targets(body: &str) -> Vec<u32> {
    let mut targets = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("ref.func ") {
        let after = &rest[at + "ref.func ".len()..];
        let digits = after
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if !digits.is_empty() {
            targets.push(digits.parse().expect("ref.func operand"));
        }
        rest = &rest[at + "ref.func ".len()..];
    }
    targets
}

pub(super) fn reachable_by_call(
    functions: &[CoreFunc],
    entry: u32,
) -> std::collections::HashSet<u32> {
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![entry];
    while let Some(current) = stack.pop() {
        if !seen.insert(current) {
            continue;
        }
        let Some(function) = functions.iter().find(|function| function.index == current) else {
            continue;
        };
        stack.extend(direct_calls(&function.body));
    }
    seen
}

pub(super) fn calls_import(
    functions: &[CoreFunc],
    reached: &std::collections::HashSet<u32>,
    import: u32,
) -> bool {
    functions.iter().any(|function| {
        reached.contains(&function.index) && direct_calls(&function.body).contains(&import)
    })
}

pub(super) fn has_call_ref(
    functions: &[CoreFunc],
    reached: &std::collections::HashSet<u32>,
) -> bool {
    functions
        .iter()
        .any(|function| reached.contains(&function.index) && function.body.contains("call_ref "))
}

/// The import call is not in the command entry's direct-call tree. It is in a
/// function reached from `ref.func`, which is the effect closure.
pub(super) fn import_is_reached_only_from_a_closure(
    functions: &[CoreFunc],
    entry: u32,
    import: u32,
) -> bool {
    if calls_import(functions, &reachable_by_call(functions, entry), import) {
        return false;
    }
    let mut from_closures = std::collections::HashSet::new();
    for function in functions {
        for target in ref_func_targets(&function.body) {
            from_closures.extend(reachable_by_call(functions, target));
        }
    }
    calls_import(functions, &from_closures, import)
}
