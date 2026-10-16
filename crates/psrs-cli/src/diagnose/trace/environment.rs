use super::EnvironmentMetadata;

pub fn observed_environment() -> EnvironmentMetadata {
    EnvironmentMetadata {
        host_os: Some(std::env::consts::OS.into()),
        host_arch: Some(std::env::consts::ARCH.into()),
        rustc_observed_version: command_version("rustc", "--version"),
        cargo_observed_version: command_version("cargo", "--version"),
    }
}

fn command_version(command: &str, argument: &str) -> Option<String> {
    std::process::Command::new(command)
        .arg(argument)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
