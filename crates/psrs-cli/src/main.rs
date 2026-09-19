use psrs_span::{SourceFile, TextRange};
use psrs_syntax::{LayoutTokenKind, RawToken, RawTokenKind, add_layout, lex, parse_module};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("{message}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(usage());
    };
    if command == "dump" {
        let Some(stage) = args.next() else {
            return Err(usage());
        };
        let Some(path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        return dump_ir(&stage, &path);
    }
    let Some(path) = args.next() else {
        return Err(usage());
    };
    let output_path = match args.next() {
        Some(flag) if flag == "-o" => Some(args.next().ok_or_else(usage)?),
        positional => positional,
    };
    if args.next().is_some() {
        return Err(usage());
    }
    let compile_command = matches!(command.as_str(), "build" | "wat");
    if output_path.is_some() && !compile_command {
        return Err(usage());
    }
    if !matches!(
        command.as_str(),
        "lex" | "layout" | "parse" | "ast" | "hir" | "check" | "build" | "wat"
    ) {
        return Err(usage());
    }

    let text = fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
    let source = SourceFile::new(path.as_str(), text.as_str());
    if command == "check" {
        return match psrs_driver::check_source(&path, &text) {
            Ok(()) => Ok(()),
            Err(errors) => {
                for error in errors {
                    print_diagnostic(&source, error.span, error.stage, &error.message);
                }
                Err(String::new())
            }
        };
    }
    if compile_command {
        let artifact = match psrs_driver::compile_source(&path, &text) {
            Ok(artifact) => artifact,
            Err(errors) => {
                for error in errors {
                    print_diagnostic(&source, error.span, error.stage, &error.message);
                }
                return Err(String::new());
            }
        };
        if command == "wat" {
            if let Some(output) = output_path {
                fs::write(&output, artifact.wat).map_err(|error| format!("{output}: {error}"))?;
            } else {
                print!("{}", artifact.wat);
            }
        } else {
            let output = output_path.unwrap_or_else(|| {
                std::path::Path::new(&path)
                    .with_extension("wasm")
                    .to_string_lossy()
                    .into_owned()
            });
            fs::write(&output, artifact.wasm).map_err(|error| format!("{output}: {error}"))?;
            println!("wrote {output}");
        }
        return Ok(());
    }
    let (tokens, errors) = lex(source.text());
    if !errors.is_empty() {
        for error in errors {
            print_diagnostic(&source, error.span, "lex error", &error.message);
        }
        return Err(String::new());
    }

    match command.as_str() {
        "lex" => print_raw_tokens(&source, &tokens),
        "layout" => {
            let tokens = add_layout(&source, &tokens);
            for token in tokens {
                let label = match token.kind {
                    LayoutTokenKind::LayoutStart => "LayoutStart".to_owned(),
                    LayoutTokenKind::LayoutSep => "LayoutSep".to_owned(),
                    LayoutTokenKind::LayoutEnd => "LayoutEnd".to_owned(),
                    LayoutTokenKind::Raw(kind) => format_raw_kind(&kind),
                };
                print_token(&source, token.span, &label);
            }
        }
        "parse" => {
            let tokens = add_layout(&source, &tokens);
            match parse_module(&tokens) {
                Ok(module) => println!("{module:#?}"),
                Err(error) => {
                    print_diagnostic(&source, error.span, "parse error", &error.message);
                    return Err(String::new());
                }
            }
        }
        "ast" => {
            let tokens = add_layout(&source, &tokens);
            let module = match parse_module(&tokens) {
                Ok(module) => module,
                Err(error) => {
                    print_diagnostic(&source, error.span, "parse error", &error.message);
                    return Err(String::new());
                }
            };
            match psrs_ast::lower_module(module) {
                Ok(module) => println!("{module:#?}"),
                Err(errors) => {
                    for error in errors {
                        print_diagnostic(&source, error.span, "surface lowering", error.message);
                    }
                    return Err(String::new());
                }
            }
        }
        "hir" => {
            let tokens = add_layout(&source, &tokens);
            let module = match parse_module(&tokens) {
                Ok(module) => module,
                Err(error) => {
                    print_diagnostic(&source, error.span, "parse error", &error.message);
                    return Err(String::new());
                }
            };
            let ast = match psrs_ast::lower_module(module) {
                Ok(ast) => ast,
                Err(errors) => {
                    for error in errors {
                        print_diagnostic(&source, error.span, "surface lowering", error.message);
                    }
                    return Err(String::new());
                }
            };
            let intrinsics = psrs_resolve::bootstrap_externals();
            match psrs_resolve::resolve_module_with_externals(
                ast,
                psrs_hir::ModuleId(0),
                &intrinsics,
            ) {
                Ok(module) => println!("{module:#?}"),
                Err(errors) => {
                    for error in errors {
                        print_diagnostic(&source, error.span, "resolve error", error.message());
                    }
                    return Err(String::new());
                }
            }
        }
        _ => return Err(usage()),
    }
    Ok(())
}

fn usage() -> String {
    "usage: psrs <lex|layout|parse|ast|hir|check> <file.purs>\n       psrs build <file.purs> [-o output.wasm]\n       psrs wat <file.purs> [-o output.wat]\n       psrs dump <core|cc|mir> <file.purs>".into()
}

fn dump_ir(stage: &str, path: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let source = SourceFile::new(path, text.as_str());
    let compilation = match psrs_driver::compile_source_with_dumps(path, &text) {
        Ok(compilation) => compilation,
        Err(errors) => {
            for error in errors {
                print_diagnostic(&source, error.span, error.stage, &error.message);
            }
            return Err(String::new());
        }
    };
    let Some(dump) = compilation.dumps.get(stage) else {
        return Err(usage());
    };
    print!("{dump}");
    Ok(())
}

fn print_raw_tokens(source: &SourceFile, tokens: &[RawToken]) {
    for token in tokens {
        print_token(source, token.span, &format_raw_kind(&token.kind));
    }
}

fn print_token(source: &SourceFile, span: TextRange, label: &str) {
    let (line, column) = source.line_column(span.start);
    let value = source
        .text()
        .get(span.start as usize..span.end as usize)
        .filter(|text| !text.is_empty())
        .map(|text| format!(" {text:?}"))
        .unwrap_or_default();
    println!(
        "{label:<14} {:>5}..{:<5} {line}:{column}{value}",
        span.start, span.end
    );
}

fn format_raw_kind(kind: &RawTokenKind) -> String {
    match kind {
        RawTokenKind::LowerIdent(text)
        | RawTokenKind::UpperIdent(text)
        | RawTokenKind::Integer(text)
        | RawTokenKind::Operator(text) => format!("{}({text})", kind.label()),
        RawTokenKind::String(text) => format!("String({text:?})"),
        RawTokenKind::Char(character) => format!("Char({character:?})"),
        _ => kind.label().to_owned(),
    }
}

fn print_diagnostic(source: &SourceFile, span: TextRange, kind: &str, message: &str) {
    let (line, column) = source.line_column(span.start);
    eprintln!("{}:{line}:{column}: {kind}: {message}", source.name());
}
