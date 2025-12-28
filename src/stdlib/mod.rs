/// Standard library math module.
pub mod math;
/// Standard library I/O module.
pub mod std;

use crate::core::engine::Engine;

/// Load all standard library modules into the engine.
/// 
/// This function loads all available standard library modules,
/// making them available for import and use in CantaLoop programs.
pub fn load_all_stdlib(engine: &mut Engine) {
    engine.load_stdlib(&*math::MATH_MODULE, "");
    engine.load_stdlib(&*std::STD_MODULE, "");
}

