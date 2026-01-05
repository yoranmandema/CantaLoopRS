use crate::core::engine::StdModule;
use crate::core::vm::Value;

/// Standard logic module.
///
/// This is pure declarative metadata describing the logic module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref LOGIC_MODULE: StdModule = crate::melon_module! {
    module logic {
        fn and(a: bool, b: bool) -> bool {
            |args, _heap| {
                let a = args[0].as_boolean().expect("and expects boolean arguments");
                let b = args[1].as_boolean().expect("and expects boolean arguments");
                Value::boolean(a && b)
            }
        }
        fn or(a: bool, b: bool) -> bool {
            |args, _heap| {
                let a = args[0].as_boolean().expect("or expects boolean arguments");
                let b = args[1].as_boolean().expect("or expects boolean arguments");
                Value::boolean(a || b)
            }
        }
        fn not(a: bool) -> bool {
            |args, _heap| {
                let a = args[0].as_boolean().expect("not expects boolean argument");
                Value::boolean(!a)
            }
        }
        fn xor(a: bool, b: bool) -> bool {
            |args, _heap| {
                let a = args[0].as_boolean().expect("xor expects boolean arguments");
                let b = args[1].as_boolean().expect("xor expects boolean arguments");
                Value::boolean(a ^ b)
            }
        }
    }
    };
}
