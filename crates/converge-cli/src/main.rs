#![forbid(unsafe_code)]

use std::path::Path;

use converge_lang::parser::{format_diagnostic, parse_program};
use converge_lang::validate::validate;

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".to_string());

    match cmd.as_str() {
        "check" => cmd_check(args),
        "ast" => cmd_ast(args),
        "cvir" => cmd_cvir(args),
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

fn read_file(path: &str) -> String {
    std::fs::read_to_string(Path::new(path)).unwrap_or_else(|e| {
        eprintln!("error: failed to read `{path}`: {e}");
        std::process::exit(2);
    })
}

fn print_usage() {
    eprintln!(
        "\
converge — neuromorphic language toolchain (pre-α)

USAGE:
  converge <command> <file>

COMMANDS:
  check   Parse + validate a Converge file
  ast     Print parsed AST (debug)
  cvir    Emit canonical JSON IR (debug)
  help    Show this help

EXAMPLES:
  cargo run -p converge-cli -- check examples/hello.cv
  cargo run -p converge-cli -- ast   examples/hello.cv
  cargo run -p converge-cli -- cvir  examples/hello.cv
"
    );
}
