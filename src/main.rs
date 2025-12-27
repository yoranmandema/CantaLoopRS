use crate::{engine::Engine, semantic_analyser::{FunctionSignature, ValueKind}};

mod ast;
mod parser;
mod bytecode;
mod engine;
mod vm;
mod semantic_analyser;

#[macro_use]
extern crate lazy_static;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let filename = args.get(1).map(|s| s.as_str()).unwrap_or("examples/thunk.mln");
    let input = std::fs::read_to_string(filename)
        .expect(&format!("Failed to read {}", filename));

    let mut engine = Engine::new();

    let print_sig = FunctionSignature {
        params: vec![ValueKind::String],
        return_type: Box::new(ValueKind::String),
    };
    engine.add_function("print", print_sig, |args| {
        println!("{}", args[0]);
        "".to_string()
    });

    engine.run(&input);
}
