use super::purs_available;
use psrs_driver::{
    DiagnosticOrigin, check_program_with_warnings, check_source, check_source_with_warnings,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn fixture(relative: &str) -> (PathBuf, String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/prim/reports")
        .join(relative);
    let source = std::fs::read_to_string(&path).expect("read report fixture");
    (path, source)
}

fn purs_compile(path: &Path) -> Output {
    purs_compile_paths(&[path.to_owned()])
}

fn purs_compile_paths(paths: &[PathBuf]) -> Output {
    static NEXT_OUTPUT: AtomicUsize = AtomicUsize::new(0);
    let output = std::env::temp_dir().join(format!(
        "psrs-report-purs-{}-{}",
        std::process::id(),
        NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&output);
    let result = Command::new("purs")
        .arg("compile")
        .args(paths)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run official purs");
    let _ = std::fs::remove_dir_all(output);
    result
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn fail_partial_and_warn_cases_match_official_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    for (relative, expected) in [
        (
            "fail_beside_above.purs",
            "Custom error:\n\n    bad \"h e l l o\"\n    value",
        ),
        (
            "fail_multiline_beside.purs",
            "Custom error:\n\n    long!\n    x",
        ),
        ("fail_residual.purs", "Custom error:\n\n    custom residual"),
        ("partial_residual.purs", "Prim.Partial"),
    ] {
        let (path, source) = fixture(relative);
        let purs = purs_compile(&path);
        let purs_text = output_text(&purs);
        assert!(
            !purs.status.success(),
            "purs accepted {relative}: {purs_text}"
        );
        assert!(purs_text.contains(expected), "{relative}: {purs_text}");
        let errors = check_source(&path.to_string_lossy(), &source)
            .expect_err("the local compiler must reject the official failing case");
        assert_eq!(errors.len(), 1, "{relative}: {errors:?}");
        assert_eq!(
            errors[0].code,
            Some("NoInstanceFound"),
            "{relative}: {errors:?}"
        );
        if relative.starts_with("fail_") {
            let custom = expected
                .split_once("\n\n")
                .map_or(expected, |(_, rendered)| rendered)
                .replace("    ", "  ");
            assert!(
                errors[0].message.contains(&custom),
                "{relative}: {errors:?}"
            );
        } else {
            assert!(errors[0].message.contains("Partial"), "{errors:?}");
            let use_site = source.rfind("foo").expect("Partial concrete use") as u32;
            assert!(
                (errors[0].span.start..errors[0].span.end).contains(&use_site),
                "{relative}: diagnostic span {:?} should point at the Partial use {use_site}",
                errors[0].span
            );
        }
    }

    let (path, source) = fixture("warn_given_preference.purs");
    let purs = purs_compile(&path);
    let purs_text = output_text(&purs);
    assert!(
        purs.status.success(),
        "purs rejected warning case: {purs_text}"
    );
    assert_eq!(purs_text.matches("Warning ").count(), 2, "{purs_text}");
    assert!(
        purs_text.contains("A custom warning occurred"),
        "{purs_text}"
    );
    let warnings = check_source_with_warnings(&path.to_string_lossy(), &source)
        .expect("Warn is a successful typecheck with diagnostics");
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings.iter().all(|warning| {
        warning.diagnostic.code == Some("UserDefinedWarning")
            && warning.diagnostic.stage == "P5 typecheck"
    }));
    assert!(
        warnings
            .iter()
            .any(|warning| warning.diagnostic.message.contains("foo"))
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.diagnostic.message.contains("bar"))
    );
}

#[test]
fn inferred_warn_reports_once_at_its_declaration_and_is_not_retained() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let (path, source) = fixture("warn_inferred_no_residual.purs");
    let purs = purs_compile(&path);
    let purs_text = output_text(&purs);
    assert!(
        purs.status.success(),
        "purs rejected inferred Warn: {purs_text}"
    );
    assert_eq!(
        purs_text.matches("A custom warning occurred").count(),
        1,
        "{purs_text}"
    );
    assert!(
        purs_text.contains("in value declaration inferred"),
        "{purs_text}"
    );
    assert!(purs_text.contains(":8:1 - 8:30"), "{purs_text}");
    let warnings = check_source_with_warnings(&path.to_string_lossy(), &source)
        .expect("inferred Warn discharges with one warning");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(
        warnings[0].diagnostic.code,
        Some("UserDefinedWarning"),
        "{warnings:?}"
    );
    let inferred = source
        .find("inferred value")
        .expect("inferred warning owner") as u32;
    assert!(
        (warnings[0].diagnostic.span.start..warnings[0].diagnostic.span.end).contains(&inferred),
        "warning span {:?} should identify the inferred declaration at {inferred}",
        warnings[0].diagnostic.span
    );
}

#[test]
fn constrained_forall_ascriptions_match_official_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let (path, source) = fixture("ascribed_fail_mono.purs");
    let purs = purs_compile(&path);
    let purs_text = output_text(&purs);
    assert!(!purs.status.success(), "purs accepted Fail: {purs_text}");
    assert!(purs_text.contains("Custom error:"), "{purs_text}");
    assert!(purs_text.contains("polymorphic failure"), "{purs_text}");
    let errors = check_source(&path.to_string_lossy(), &source).expect_err("Fail rejects use");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, Some("NoInstanceFound"), "{errors:?}");
    assert!(
        errors[0].message.contains("polymorphic failure"),
        "{errors:?}"
    );

    let (path, source) = fixture("ascribed_warn_mono.purs");
    let purs = purs_compile(&path);
    let purs_text = output_text(&purs);
    assert!(purs.status.success(), "purs rejected Warn: {purs_text}");
    assert!(purs_text.contains("UserDefinedWarning"), "{purs_text}");
    assert!(purs_text.contains("polymorphic warning"), "{purs_text}");
    let warnings = check_source_with_warnings(&path.to_string_lossy(), &source)
        .expect("Warn remains a successful typecheck");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].diagnostic.code, Some("UserDefinedWarning"));

    let (path, source) = fixture("ascribed_identity_rank_n.purs");
    let purs = purs_compile(&path);
    assert!(
        purs.status.success(),
        "purs rejected constrained rank-N evidence: {}",
        output_text(&purs)
    );
    check_source(&path.to_string_lossy(), &source)
        .expect("ordinary class evidence must survive a polymorphic ascription");
}

#[test]
fn imported_warn_origin_and_scoped_given_match_official_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let (library_path, library) = fixture("warn_imported_lib.purs");
    let (main_path, main) = fixture("warn_imported_main.purs");
    let purs = purs_compile_paths(&[library_path, main_path]);
    let purs_text = output_text(&purs);
    assert!(
        purs.status.success(),
        "purs rejected imported Warn: {purs_text}"
    );
    assert_eq!(
        purs_text.matches("Warning found:").count(),
        1,
        "{purs_text}"
    );
    assert!(purs_text.contains("from library"), "{purs_text}");
    assert!(
        purs_text.contains("warn_imported_main.purs:8:1 - 8:15"),
        "official warning should point to the consumer declaration: {purs_text}"
    );

    let warnings = check_program_with_warnings(&[("Lib.purs", &library), ("Main.purs", &main)])
        .expect("an imported Warn obligation succeeds with a diagnostic");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].source, DiagnosticOrigin::Source(1));
    assert_eq!(warnings[0].diagnostic.code, Some("UserDefinedWarning"));
    assert!(warnings[0].diagnostic.message.contains("from library"));
    let trigger = main
        .find("trigger :: Int")
        .expect("caller declaration is present") as u32;
    assert!(
        (warnings[0].diagnostic.span.start..warnings[0].diagnostic.span.end).contains(&trigger),
        "warning span {:?} should point at the importing module's declaration at {trigger}",
        warnings[0].diagnostic.span
    );
}

#[test]
fn warn_origin_matches_local_let_and_instance_method_boundaries() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    for (relative, anchor, expected_purs_location) in [
        ("warn_local_let.purs", "outer :: Int", ":8:1 - 8:13"),
        (
            "warn_instance_method.purs",
            "instance runInt :: Run Int where",
            ":11:1 - 12:27",
        ),
    ] {
        let (path, source) = fixture(relative);
        let purs = purs_compile(&path);
        let purs_text = output_text(&purs);
        assert!(
            purs.status.success(),
            "purs rejected {relative}: {purs_text}"
        );
        assert_eq!(
            purs_text.matches("Warning found:").count(),
            1,
            "{purs_text}"
        );
        assert!(purs_text.contains(expected_purs_location), "{purs_text}");

        let warnings = check_source_with_warnings(&path.to_string_lossy(), &source)
            .unwrap_or_else(|errors| panic!("local typecheck rejected {relative}: {errors:?}"));
        assert_eq!(warnings.len(), 1, "{relative}: {warnings:?}");
        let expected_start = source.find(anchor).expect("expected warning owner") as u32;
        assert!(
            (warnings[0].diagnostic.span.start..warnings[0].diagnostic.span.end)
                .contains(&expected_start),
            "{relative}: warning span {:?} should include {anchor:?} at {expected_start}",
            warnings[0].diagnostic.span
        );
    }
}
