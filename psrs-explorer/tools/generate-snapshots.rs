use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Stage {
    id: &'static str,
    args: &'static [&'static str],
}

const STAGES: &[Stage] = &[
    Stage { id: "tokens", args: &["lex"] },
    Stage { id: "layout", args: &["layout"] },
    Stage { id: "cst", args: &["parse"] },
    Stage { id: "ast", args: &["ast"] },
    Stage { id: "hir", args: &["hir"] },
    Stage { id: "core", args: &["dump", "core"] },
    Stage { id: "cc", args: &["dump", "cc"] },
    Stage { id: "mir", args: &["dump", "mir"] },
];

const EXAMPLES: &[(&str, &str)] = &[
    ("basic", "examples/basic.purs"),
    ("resolved", "examples/resolved.purs"),
    ("hello", "examples/hello.purs"),
];

fn main() -> Result<(), String> {
    let explorer_root = env::current_dir().map_err(|error| error.to_string())?;
    let repository_root = explorer_root
        .parent()
        .ok_or_else(|| "psrs-explorer must have a repository parent".to_owned())?;
    let output_dir = explorer_root.join("src/generated");
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;

    for (name, source) in EXAMPLES {
        let stages = STAGES
            .iter()
            .map(|stage| run_stage(repository_root, source, stage))
            .collect::<Result<Vec<_>, _>>()?;
        let json = format!(
            "{{\n  \"example\": {},\n  \"source\": {},\n  \"stages\": [\n{}\n  ]\n}}\n",
            json(name),
            json(source),
            stages.join(",\n"),
        );
        let path = output_dir.join(format!("{name}.json"));
        fs::write(&path, json).map_err(|error| format!("{}: {error}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn run_stage(repository_root: &Path, source: &str, stage: &Stage) -> Result<String, String> {
    let mut args = vec!["run", "--quiet", "--"];
    args.extend(stage.args);
    args.push(source);
    let output = Command::new("cargo")
        .args(&args)
        .current_dir(repository_root)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", command(&args)))?;
    let command = format!("cargo {}", command(&args));
    if !output.status.success() {
        return Ok(format!(
            "    {{ \"id\": {}, \"command\": {}, \"error\": {} }}",
            json(stage.id),
            json(&command),
            json(String::from_utf8_lossy(&output.stderr).trim()),
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("{} produced non-UTF-8 output: {error}", command))?;
    Ok(format!(
        "    {{ \"id\": {}, \"command\": {}, \"output\": {} }}",
        json(stage.id),
        json(&command),
        json(&stdout),
    ))
}

fn command(args: &[&str]) -> String {
    args.join(" ")
}

fn json(value: &str) -> String {
    format!("\"{}\"", value.escape_default())
}

#[allow(dead_code)]
fn _path_for_documentation(explorer_root: &Path, name: &str) -> PathBuf {
    explorer_root.join("src/generated").join(format!("{name}.json"))
}
