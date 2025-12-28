use CantaLoopRS::{Engine, stdlib};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let filename = args.get(1).map(|s| s.as_str()).unwrap_or("examples/thunk.mln");

    let mut engine = Engine::new();
    
    // Load all standard library modules
    stdlib::load_all_stdlib(&mut engine);

    engine.run(filename);
}
