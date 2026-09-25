use crate::engine::monitor_stream;
use crate::render::Config;
use std::env;
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const HELP_TEXT: &str = r#"nom-rs usages:
  Wrappers:
    nom build <nix-args>
    nom shell <nix-args>
    nom develop <nix-args>
    nom copy <nix-args>
    nom flake <nix-args>

    nom-build <nix-args>
    nom-shell <nix-args>

  Direct piping:
    via json parsing:
      nix build --log-format internal-json -v <nix-args> |& nom --json
      nix-build --log-format internal-json -v <nix-args> |& nom --json

    via human-readable log parsing:
      nix-build |& nom

    Don't forget to redirect stderr, too. That's what the & does.

Flags:
  --version  Show version.
  -h, --help Show this help.
  --json     Parse input as nix internal-json

Please see the readme for more details:
https://github.com/mastershifuishere77/nom-rs
"#;

pub fn with_json(args: &[String]) -> Vec<String> {
    let mut out = vec![
        "-v".to_string(),
        "--log-format".to_string(),
        "internal-json".to_string(),
    ];
    out.extend_from_slice(args);
    out
}

pub fn replace_command_with_exit(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        if arg == "--command" || arg == "-c" {
            break;
        }
        out.push(arg.clone());
    }
    out.extend_from_slice(&[
        "--command".to_string(),
        "sh".to_string(),
        "-c".to_string(),
        "exit".to_string(),
    ]);
    out
}

pub fn run_app() -> ExitCode {
    let prog_name = env::args()
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "nom".to_string());

    let args: Vec<String> = env::args().skip(1).collect();

    if env::var("NIX_GET_COMPLETIONS").is_ok() {
        return handle_completions(&prog_name, &args);
    }

    match (prog_name.as_str(), args.as_slice()) {
        (_, [arg]) if arg == "--version" => {
            eprintln!("nom-rs {}", VERSION);
            let mut cmd = Command::new("nix");
            cmd.arg("--version");
            let err = cmd.exec();
            eprintln!("Failed to execute nix: {}", err);
            ExitCode::from(1)
        }
        ("nom-build", args) if args.iter().any(|a| a == "--help") => {
            let mut cmd = Command::new("nix-build");
            cmd.args(args);
            let err = cmd.exec();
            eprintln!("Failed to execute nix-build: {}", err);
            ExitCode::from(1)
        }
        ("nom-build", args) => {
            let config = Config {
                silent: false,
                piping: false,
            };
            run_monitored_command("nix-build", &with_json(args), config)
        }
        ("nom-shell", args) => {
            let mut check_args = with_json(args);
            check_args.extend_from_slice(&["--run".to_string(), "exit".to_string()]);
            let check_config = Config {
                silent: true,
                piping: false,
            };
            let code = run_monitored_command("nix-shell", &check_args, check_config);
            if code != ExitCode::SUCCESS {
                return code;
            }
            run_process("nix-shell", args)
        }
        ("nom", [sub, rest @ ..]) if sub == "build" && rest.iter().any(|a| a == "--help") => {
            let mut cmd = Command::new("nix");
            cmd.arg("build").args(rest);
            let err = cmd.exec();
            eprintln!("Failed to execute nix: {}", err);
            ExitCode::from(1)
        }
        ("nom", [sub, rest @ ..]) if sub == "build" => {
            let mut full_args = vec!["build".to_string()];
            full_args.extend(with_json(rest));
            let config = Config {
                silent: false,
                piping: false,
            };
            run_monitored_command("nix", &full_args, config)
        }
        ("nom", [sub, rest @ ..]) if sub == "copy" && rest.iter().any(|a| a == "--help") => {
            let mut cmd = Command::new("nix");
            cmd.arg("copy").args(rest);
            let err = cmd.exec();
            eprintln!("Failed to execute nix: {}", err);
            ExitCode::from(1)
        }
        ("nom", [sub, rest @ ..]) if sub == "copy" => {
            let mut full_args = vec!["copy".to_string()];
            full_args.extend(with_json(rest));
            let config = Config {
                silent: false,
                piping: false,
            };
            run_monitored_command("nix", &full_args, config)
        }
        ("nom", [sub, rest @ ..]) if sub == "shell" => {
            let filtered = replace_command_with_exit(rest);
            let mut check_args = vec!["shell".to_string()];
            check_args.extend(with_json(&filtered));
            let check_config = Config {
                silent: true,
                piping: false,
            };
            let code = run_monitored_command("nix", &check_args, check_config);
            if code != ExitCode::SUCCESS {
                return code;
            }
            let mut run_args = vec!["shell".to_string()];
            run_args.extend_from_slice(rest);
            run_process("nix", &run_args)
        }
        ("nom", [sub, rest @ ..]) if sub == "develop" => {
            let filtered = replace_command_with_exit(rest);
            let mut check_args = vec!["develop".to_string()];
            check_args.extend(with_json(&filtered));
            let check_config = Config {
                silent: true,
                piping: false,
            };
            let code = run_monitored_command("nix", &check_args, check_config);
            if code != ExitCode::SUCCESS {
                return code;
            }
            let mut run_args = vec!["develop".to_string()];
            run_args.extend_from_slice(rest);
            run_process("nix", &run_args)
        }
        ("nom", [sub, rest @ ..]) if sub == "flake" && rest.iter().any(|a| a == "--help") => {
            let mut cmd = Command::new("nix");
            cmd.arg("flake").args(rest);
            let err = cmd.exec();
            eprintln!("Failed to execute nix: {}", err);
            ExitCode::from(1)
        }
        ("nom", [sub, rest @ ..]) if sub == "flake" => {
            let mut full_args = vec!["flake".to_string()];
            full_args.extend(with_json(rest));
            let config = Config {
                silent: false,
                piping: false,
            };
            run_monitored_command("nix", &full_args, config)
        }
        ("nom", []) => {
            let config = Config {
                silent: false,
                piping: true,
            };
            let final_state = monitor_stream(io::stdin(), false, config);
            if final_state.full_summary.failed_builds.is_empty()
                && final_state.nix_errors.is_empty()
            {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        ("nom", [arg]) if arg == "--json" => {
            let config = Config {
                silent: false,
                piping: true,
            };
            let final_state = monitor_stream(io::stdin(), true, config);
            if final_state.full_summary.failed_builds.is_empty()
                && final_state.nix_errors.is_empty()
            {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        _ => {
            eprint!("{}", HELP_TEXT);
            if args.iter().any(|a| a == "-h" || a == "--help") {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
    }
}

fn handle_completions(prog_name: &str, args: &[String]) -> ExitCode {
    let known_sub_commands = ["build", "copy", "shell", "develop"];
    let known_flags = ["--version", "-h", "--help", "--json"];

    match (prog_name, args) {
        ("nom", [input]) => {
            println!("normal");
            let mut all = Vec::new();
            all.extend_from_slice(&known_sub_commands);
            all.extend_from_slice(&known_flags);
            for item in all {
                if item.starts_with(input) {
                    println!("{}", item);
                }
            }
            ExitCode::SUCCESS
        }
        ("nom", [sub_cmd, rest @ ..]) if known_sub_commands.contains(&sub_cmd.as_str()) => {
            let mut cmd = Command::new("nix");
            cmd.arg(sub_cmd).args(rest);
            let err = cmd.exec();
            eprintln!("Failed to execute nix completion: {}", err);
            ExitCode::from(1)
        }
        _ => {
            eprintln!("No completion support for {} {}", prog_name, args.join(" "));
            ExitCode::from(1)
        }
    }
}

fn run_monitored_command(program: &str, args: &[String], config: Config) -> ExitCode {
    let mut child = match Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "\x1b[31;1mnom-rs:\x1b[0m Command '{}' not available from $PATH: {}",
                program, e
            );
            return ExitCode::from(1);
        }
    };

    let stderr = child.stderr.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let _final_state = monitor_stream(stderr, true, config);
    let status = child.wait().unwrap();

    let mut stdout_buf = Vec::new();
    let _ = stdout.read_to_end(&mut stdout_buf);
    if !stdout_buf.is_empty() {
        let _ = io::stdout().write_all(&stdout_buf);
        let _ = io::stdout().flush();
    }

    if let Some(code) = status.code() {
        ExitCode::from(code as u8)
    } else {
        ExitCode::from(1)
    }
}

fn run_process(program: &str, args: &[String]) -> ExitCode {
    let status = match Command::new(program).args(args).status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to execute {}: {}", program, e);
            return ExitCode::from(1);
        }
    };

    if let Some(code) = status.code() {
        ExitCode::from(code as u8)
    } else {
        ExitCode::from(1)
    }
}
