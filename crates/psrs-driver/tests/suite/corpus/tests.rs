use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

fn temp_directory(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("psrs-case-{tag}-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("creating a temp directory");
    directory
}

fn write(directory: &Path, relative: &str, text: &str) -> PathBuf {
    let path = directory.join(relative);
    std::fs::create_dir_all(path.parent().expect("a file has a parent")).expect("a directory");
    std::fs::write(&path, text).expect("writing a module");
    path
}

fn loaded_names(case: &Case) -> Vec<String> {
    case.loaded
        .iter()
        .map(|(path, _)| {
            std::path::Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn loads_a_sibling_module_from_a_case_directory() {
    let root = temp_directory("sibling");
    write(&root, "Case/M1.purs", "module M1 where\nanswer = 1\n");
    let case_path = write(
        &root,
        "Case/M2.purs",
        "module M2 where\nimport M1\nanswer = 2\n",
    );
    let case = load_case(
        &case_path,
        &root,
        &std::fs::read_to_string(&case_path).unwrap(),
    );
    assert_eq!(loaded_names(&case), ["M1.purs"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_case_in_the_category_root_is_never_given_a_sibling() {
    // The corpus reuses module names across cases in one category root, so
    // resolving a name there would substitute another case's module.
    let root = temp_directory("root");
    write(&root, "M1.purs", "module M1 where\nanswer = 1\n");
    let case_path = write(&root, "Case.purs", "module Case where\nimport M1\n");
    let case = load_case(
        &case_path,
        &root,
        &std::fs::read_to_string(&case_path).unwrap(),
    );
    assert_eq!(loaded_names(&case), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_support_module_wins_over_one_beside_the_case() {
    let root = temp_directory("support");
    // `Case/Inner/M1.purs` is the case's own support module; `Case/M1.purs`
    // declares a different `M1` and must not be substituted for it.
    write(
        &root,
        "Case/Inner/M1.purs",
        "module M1 (wanted) where\nwanted = 1\n",
    );
    write(&root, "Case/M1.purs", "module M1 where\nother = 0\n");
    let case_path = write(
        &root,
        "Case/Inner.purs",
        "module Inner where\nimport M1 (wanted)\n",
    );
    let case = load_case(
        &case_path,
        &root,
        &std::fs::read_to_string(&case_path).unwrap(),
    );
    assert!(
        case.own
            .iter()
            .any(|(path, _)| path.ends_with("Case/Inner/M1.purs")),
        "the support module is the case's own: {:?}",
        case.own
    );
    assert_eq!(
        loaded_names(&case),
        Vec::<String>::new(),
        "the support directory is searched first, so no sibling loads"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_case_is_blocked_on_its_own_modules_not_its_siblings() {
    let root = temp_directory("blocked");
    let case_path = write(
        &root,
        "Case/Case.purs",
        "module Case where\nimport Sibling\nanswer = 1\n",
    );
    write(
        &root,
        "Case/Sibling.purs",
        "module Sibling where\nimport Absent\n",
    );
    let case = load_case(
        &case_path,
        &root,
        &std::fs::read_to_string(&case_path).unwrap(),
    );
    assert_eq!(case.own.len(), 1, "{:?}", case.own);
    assert_eq!(loaded_names(&case), ["Sibling.purs"]);
    let diagnostic = |source: psrs_driver::DiagnosticOrigin,
                      stage: &'static str,
                      code: &'static str,
                      message: &str| {
        psrs_driver::ProgramDiagnostic {
            source,
            diagnostic: psrs_driver::Diagnostic {
                stage,
                span: psrs_span::TextRange::default(),
                message: message.to_owned(),
                code: Some(code),
                kind: None,
            },
        }
    };
    use psrs_driver::DiagnosticOrigin as Origin;
    let missing = diagnostic(
        Origin::Source(0),
        "P3 resolve",
        "ModuleNotFound",
        "module `Absent` was not found",
    );
    let sibling_parse_error = diagnostic(
        Origin::Source(1),
        "P1 parse",
        "ErrorParsingModule",
        "the sibling does not parse",
    );
    assert_eq!(
        blocker(&case, std::slice::from_ref(&missing)),
        Blocker::MissingLibrary
    );
    assert_eq!(
        blocker(&case, &[sibling_parse_error]),
        Blocker::HarnessLoading,
        "a case whose own modules are clean is blocked on assembly, not on itself"
    );
    assert_eq!(
        blocker(
            &case,
            &[diagnostic(
                Origin::Library,
                "P5 typecheck",
                "TypesDoNotUnify",
                "the standard library does not type check"
            )]
        ),
        Blocker::HarnessLoading,
        "a standard-library diagnostic is never the case's own"
    );
    assert_eq!(
        blocker(
            &case,
            &[diagnostic(
                Origin::Program,
                "P10 Wasm structuring",
                "",
                "no program entry point was selected"
            )]
        ),
        Blocker::Stage("P10 Wasm structuring".to_owned()),
        "a program-wide diagnostic is this case's program failing, not a harness assembly failure"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_standard_library_diagnostic_is_never_the_cases_own() {
    // The trusted prefix is not part of the case's source list, so a library
    // diagnostic has no index in it. Treating that as source zero would let a
    // library error decide whether the case agrees.
    let case = Case {
        own: vec![("Case.purs".to_owned(), "module Case where".to_owned())],
        loaded: Vec::new(),
        directory: PathBuf::from("."),
        own_directory: true,
    };
    use psrs_driver::DiagnosticOrigin as Origin;
    assert!(!case.is_own(Origin::Library));
    assert!(!case.is_own(Origin::Program));
    assert!(case.is_own(Origin::Source(0)));
    assert!(!case.is_own(Origin::Source(1)));
}

#[test]
fn runtime_programs_keep_support_sources_without_running_them_as_commands() {
    let root = temp_directory("runtime-programs");
    let main = write(
        &root,
        "2018.purs",
        "module Main where\nimport A\nmain :: Int\nmain = answer\n",
    );
    let support = write(
        &root,
        "2018/A.purs",
        "module A where\nanswer :: Int\nanswer = 42\n",
    );
    let missing_entry = write(&root, "Missing.purs", "module Missing where\nanswer = 0\n");
    let malformed = write(&root, "Malformed.purs", "not valid PureScript");
    assert_eq!(
        collected_programs(&root, None, None),
        [main.clone(), malformed, missing_entry],
        "all roots are measured, even when malformed or missing main"
    );
    assert_eq!(collected_files(&root, None).len(), 4);
    assert_eq!(
        collected_programs(&root, Some("2018.purs"), Some(1)),
        std::slice::from_ref(&main)
    );
    let case = load_case(&main, &root, &std::fs::read_to_string(&main).unwrap());
    assert_eq!(case.own.len(), 2);
    assert_eq!(case.own[1].0, support.to_string_lossy());
    let artifact = psrs_driver::compile_program_sources(&case.inputs())
        .expect("the support value must be resolved through its parent program");
    assert!(!artifact.wasm.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn runtime_program_filter_precedes_the_limit() {
    let root = temp_directory("runtime-filter");
    write(&root, "A.purs", "module Main where\nmain = 0\n");
    let last = write(&root, "Z.purs", "module Main where\nmain = 42\n");
    assert_eq!(collected_programs(&root, Some("Z.purs"), Some(1)), [last]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn runtime_ffi_exclusion_covers_support_modules_and_foreign_files() {
    let root = temp_directory("runtime-ffi");
    let main = write(&root, "Case.purs", "module Main where\nmain = 0\n");
    let support = write(&root, "Case/A.purs", "module A where\nanswer = 42\n");
    let load = || load_case(&main, &root, &std::fs::read_to_string(&main).unwrap());
    assert!(!program_is_ffi_excluded(&load()));
    std::fs::write(&support, "module A where\nforeign import answer :: Int\n").unwrap();
    assert!(program_is_ffi_excluded(&load()));
    std::fs::write(&support, "module A where\nanswer = 42\n").unwrap();
    std::fs::write(support.with_extension("js"), "export const answer = 42;\n").unwrap();
    assert!(program_is_ffi_excluded(&load()));
    let _ = std::fs::remove_dir_all(root);
}
