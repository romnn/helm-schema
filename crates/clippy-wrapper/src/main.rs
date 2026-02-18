//! Small helper binary used by `cargo fc lint` to invoke `cargo clippy` across
//! multiple feature combinations.

use std::path::PathBuf;
use std::process::ExitCode;

fn split_args_on_double_dash(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut cargo_args = Vec::new();
    let mut clippy_args = Vec::new();

    let mut seen_double_dash = false;
    for arg in args {
        if !seen_double_dash && arg == "--" {
            seen_double_dash = true;
            continue;
        }

        if seen_double_dash {
            clippy_args.push(arg);
        } else {
            cargo_args.push(arg);
        }
    }

    (cargo_args, clippy_args)
}

fn workspace_dir() -> PathBuf {
    let workspace_dir = std::env::var_os("CARGO_WORKSPACE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    if let Some(workspace_dir) = workspace_dir {
        return workspace_dir;
    }

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..2 {
        let Some(parent) = dir.parent() else {
            return dir;
        };
        dir = parent.to_path_buf();
    }

    dir
}

fn exit_code_from_i32(code: i32) -> u8 {
    match u8::try_from(code) {
        Ok(code) => code,
        Err(_err) => {
            if code < 0 {
                0
            } else {
                255
            }
        }
    }
}

fn exit_code_from_status(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return exit_code_from_i32(code);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            let code = 128 + signal;
            return exit_code_from_i32(code);
        }
    }

    1
}

fn run_cargo_clippy(
    cargo_args: Vec<String>,
    args: Vec<String>,
) -> Result<std::process::ExitStatus, std::io::Error> {
    let (user_cargo_args, user_clippy_args) = split_args_on_double_dash(args);

    let mut has_package = false;
    let mut has_workspace = false;
    let mut has_no_deps = false;
    let mut has_all_features = false;
    let mut has_features = false;
    let mut has_no_default_features = false;

    for arg in &user_cargo_args {
        match arg.as_str() {
            "-p" | "--package" => {
                has_package = true;
            }
            "--workspace" => {
                has_workspace = true;
            }
            "--no-deps" => {
                has_no_deps = true;
            }
            "--all-features" => {
                has_all_features = true;
            }
            "--features" => {
                has_features = true;
            }
            "--no-default-features" => {
                has_no_default_features = true;
            }
            _ => {
                if arg.starts_with("--package=") {
                    has_package = true;
                }
                if arg.starts_with("-p") && arg.len() > 2 {
                    has_package = true;
                }
                if arg.starts_with("--features=") {
                    has_features = true;
                }
            }
        }
    }

    let mut cargo_clippy_args = Vec::new();
    cargo_clippy_args.extend(cargo_args);

    if !has_package && !has_workspace {
        cargo_clippy_args.push("--workspace".to_string());
    }

    cargo_clippy_args.push("--all-targets".to_string());

    if has_package {
        if !has_no_deps {
            cargo_clippy_args.push("--no-deps".to_string());
        }
    } else if !has_all_features && !has_features && !has_no_default_features {
        cargo_clippy_args.push("--all-features".to_string());
    }

    cargo_clippy_args.extend(user_cargo_args);

    let mut command = std::process::Command::new("cargo");
    command.current_dir(workspace_dir());
    command.arg("clippy");
    command.args(cargo_clippy_args);
    command.arg("--");
    command.args(user_clippy_args);
    command.arg("-Dclippy::all");
    command.arg("-Dclippy::pedantic");

    command.status()
}

fn usage(program_name: &str) {
    eprintln!("Usage:");
    eprintln!("  {program_name} lint [cargo clippy args] [-- clippy args]");
    eprintln!("  {program_name} fixit [cargo clippy args] [-- clippy args]");
}

fn main() -> ExitCode {
    let mut args_iter = std::env::args();
    let program_name = args_iter
        .next()
        .unwrap_or_else(|| "clippy-wrapper".to_string());

    let Some(subcommand) = args_iter.next() else {
        usage(&program_name);
        return ExitCode::from(2);
    };

    let remaining_args: Vec<String> = args_iter.collect();

    let (cargo_args, args) = match subcommand.as_str() {
        "lint" => (Vec::new(), remaining_args),
        "fixit" => (
            vec![
                "--fix".to_string(),
                "--allow-dirty".to_string(),
                "--allow-staged".to_string(),
            ],
            remaining_args,
        ),
        "-h" | "--help" | "help" => {
            usage(&program_name);
            return ExitCode::from(0);
        }
        _ => {
            eprintln!("unknown subcommand: {subcommand}");
            usage(&program_name);
            return ExitCode::from(2);
        }
    };

    let status = match run_cargo_clippy(cargo_args, args) {
        Ok(status) => status,
        Err(err) => {
            eprintln!("failed to run cargo clippy: {err}");
            return ExitCode::from(1);
        }
    };

    if status.success() {
        return ExitCode::SUCCESS;
    }

    ExitCode::from(exit_code_from_status(status))
}
