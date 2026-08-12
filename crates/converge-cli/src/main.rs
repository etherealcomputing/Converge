#![forbid(unsafe_code)]

use std::path::Path;

use converge_lang::parser::{format_diagnostic, parse_program};
use converge_lang::validate::validate;
use converge_perturb::{Operator, SweepConfig};
use converge_sim::simulate;

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".to_string());

    match cmd.as_str() {
        "check" => cmd_check(args),
        "ast" => cmd_ast(args),
        "cvir" => cmd_cvir(args),
        "sim" => cmd_sim(args),
        "perturb" => cmd_perturb(args),
        "help" | "-h" | "--help" => {
            print_usage();
            std::process::exit(0);
        }
        _ => {
            eprintln!("error: unknown command `{cmd}`\n");
            print_usage();
            std::process::exit(2);
        }
    }
}

fn cmd_check(mut args: impl Iterator<Item = String>) {
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("error: expected a file path\n");
            print_usage();
            std::process::exit(2);
        }
    };

    let src = read_file(&path);
    let program = match parse_program(&src) {
        Ok(p) => p,
        Err(diag) => {
            eprintln!("{}", format_diagnostic(&src, &diag));
            std::process::exit(1);
        }
    };

    if let Err(diags) = validate(&program) {
        for diag in diags {
            eprintln!("{}", format_diagnostic(&src, &diag));
        }
        std::process::exit(1);
    }
}

fn cmd_ast(mut args: impl Iterator<Item = String>) {
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("error: expected a file path\n");
            print_usage();
            std::process::exit(2);
        }
    };

    let src = read_file(&path);
    match parse_program(&src) {
        Ok(program) => {
            println!("{program:#?}");
        }
        Err(diag) => {
            eprintln!("{}", format_diagnostic(&src, &diag));
            std::process::exit(1);
        }
    }
}

fn cmd_cvir(mut args: impl Iterator<Item = String>) {
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("error: expected a file path\n");
            print_usage();
            std::process::exit(2);
        }
    };

    let src = read_file(&path);
    let program = match parse_program(&src) {
        Ok(p) => p,
        Err(diag) => {
            eprintln!("{}", format_diagnostic(&src, &diag));
            std::process::exit(1);
        }
    };

    if let Err(diags) = validate(&program) {
        for diag in diags {
            eprintln!("{}", format_diagnostic(&src, &diag));
        }
        std::process::exit(1);
    }

    print!("{}", converge_lang::emit::cvir_json(&program));
}

fn cmd_sim(mut args: impl Iterator<Item = String>) {
    let mut file = None;
    let mut out_path = None;

    while let Some(arg) = args.next() {
        if arg == "--out" {
            out_path = args.next();
        } else if file.is_none() {
            file = Some(arg);
        } else {
            eprintln!("error: unexpected argument `{arg}`\n");
            print_usage();
            std::process::exit(2);
        }
    }

    let path = match file {
        Some(p) => p,
        None => {
            eprintln!("error: expected a file path\n");
            print_usage();
            std::process::exit(2);
        }
    };

    let src = read_file(&path);
    let program = match parse_program(&src) {
        Ok(p) => p,
        Err(diag) => {
            eprintln!("{}", format_diagnostic(&src, &diag));
            std::process::exit(1);
        }
    };

    if let Err(diags) = validate(&program) {
        for diag in diags {
            eprintln!("{}", format_diagnostic(&src, &diag));
        }
        std::process::exit(1);
    }

    let summary = match simulate(&program) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };
    let json = converge_sim::summary_json(&summary);

    if let Some(out) = out_path {
        std::fs::write(&out, json).unwrap_or_else(|e| {
            eprintln!("error: failed to write `{out}`: {e}");
            std::process::exit(2);
        });
    } else {
        print!("{json}");
    }
}

fn cmd_perturb(mut args: impl Iterator<Item = String>) {
    let mut file = None;
    let mut out_path = None;
    let mut config = SweepConfig::default();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out_path = args.next(),
            "--steps" => config.grid_steps = parse_usize(args.next(), "--steps"),
            "--bisect" => config.bisect_steps = parse_usize(args.next(), "--bisect"),
            "--operators" => config.operators = parse_operators(args.next()),
            _ if file.is_none() => file = Some(arg),
            _ => {
                eprintln!("error: unexpected argument `{arg}`\n");
                print_usage();
                std::process::exit(2);
            }
        }
    }

    let path = match file {
        Some(p) => p,
        None => {
            eprintln!("error: expected a file path\n");
            print_usage();
            std::process::exit(2);
        }
    };

    let src = read_file(&path);
    let program = match parse_program(&src) {
        Ok(p) => p,
        Err(diag) => {
            eprintln!("{}", format_diagnostic(&src, &diag));
            std::process::exit(1);
        }
    };

    if let Err(diags) = validate(&program) {
        for diag in diags {
            eprintln!("{}", format_diagnostic(&src, &diag));
        }
        std::process::exit(1);
    }

    let net = match converge_sim::elaborate(&program) {
        Ok(n) => n,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };

    let envelope = match converge_perturb::sweep(&net, &config) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };
    let json = converge_perturb::envelope_json(&envelope);

    if let Some(out) = out_path {
        std::fs::write(&out, json).unwrap_or_else(|e| {
            eprintln!("error: failed to write `{out}`: {e}");
            std::process::exit(2);
        });
    } else {
        print!("{json}");
    }
}

fn parse_usize(value: Option<String>, flag: &str) -> usize {
    let Some(raw) = value else {
        eprintln!("error: `{flag}` needs a value\n");
        print_usage();
        std::process::exit(2);
    };
    raw.parse().unwrap_or_else(|_| {
        eprintln!("error: `{flag}` expects a whole number, got `{raw}`\n");
        print_usage();
        std::process::exit(2);
    })
}

fn parse_operators(value: Option<String>) -> Vec<Operator> {
    let Some(raw) = value else {
        eprintln!("error: `--operators` needs a comma separated list\n");
        print_usage();
        std::process::exit(2);
    };
    let mut chosen = Vec::new();
    for name in raw.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match Operator::from_name(name) {
            Some(op) => chosen.push(op),
            None => {
                let known: Vec<&str> = converge_perturb::ops::ALL
                    .iter()
                    .map(|o| o.name())
                    .collect();
                eprintln!(
                    "error: unknown operator `{name}`, expected one of {}",
                    known.join(", ")
                );
                std::process::exit(2);
            }
        }
    }
    if chosen.is_empty() {
        eprintln!("error: `--operators` selected nothing\n");
        print_usage();
        std::process::exit(2);
    }
    chosen
}

fn read_file(path: &str) -> String {
    std::fs::read_to_string(Path::new(path)).unwrap_or_else(|e| {
        eprintln!("error: failed to read `{path}`: {e}");
        std::process::exit(2);
    })
}

fn print_usage() {
    eprintln!(
        "\
converge: neuromorphic language toolchain (pre-α)

USAGE:
  converge <command> <file>

COMMANDS:
  check   Parse + validate a Converge file
  ast     Print parsed AST (debug)
  cvir    Emit canonical JSON IR (debug)
  sim     Run deterministic simulator
  perturb Sweep fault operators and report the failure envelope
  help    Show this help

PERTURB OPTIONS:
  --out <path>        Write JSON to a file instead of stdout
  --steps <n>         Grid points across the severity range (default 20)
  --bisect <n>        Bisection rounds to tighten each bound (default 6)
  --operators <list>  Comma separated subset, in the order given

EXAMPLES:
  cargo run -p converge-cli -- check   examples/hello.cv
  cargo run -p converge-cli -- ast     examples/hello.cv
  cargo run -p converge-cli -- cvir    examples/hello.cv
  cargo run -p converge-cli -- sim     examples/poisson.cv
  cargo run -p converge-cli -- perturb examples/envelope.cv
  cargo run -p converge-cli -- perturb examples/envelope.cv --operators prune,gain.attenuate
"
    );
}
