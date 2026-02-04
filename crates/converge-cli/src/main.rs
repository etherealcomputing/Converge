#![forbid(unsafe_code)]

use std::path::Path;

use converge_lang::parser::{format_diagnostic, parse_program};
use converge_lang::validate::validate;
use converge_sim::simulate;

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".to_string());

    match cmd.as_str() {
        "check" => cmd_check(args),
        "ast" => cmd_ast(args),
        "cvir" => cmd_cvir(args),
        "sim" => cmd_sim(args),
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
  help    Show this help

EXAMPLES:
  cargo run -p converge-cli -- check examples/hello.cv
  cargo run -p converge-cli -- ast   examples/hello.cv
  cargo run -p converge-cli -- cvir  examples/hello.cv
  cargo run -p converge-cli -- sim   examples/poisson.cv
"
    );
}
