use crate::core::engine::StdModule;
use crate::core::vm::Value;

/// Standard comparison module.
/// 
/// This is pure declarative metadata describing the comparison module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref COMPARISON_MODULE: StdModule = crate::melon_module! {
    module comparison {
        fn eq(a: any, b: any) -> bool {
            |args, heap| {
                let a = &args[0];
                let b = &args[1];
                let result = if let (Some(a_num), Some(b_num)) = (a.as_number(), b.as_number()) {
                    a_num == b_num
                } else if let (Some(a_str), Some(b_str)) = (a.as_string(heap), b.as_string(heap)) {
                    a_str == b_str
                } else if let (Some(a_bool), Some(b_bool)) = (a.as_boolean(), b.as_boolean()) {
                    a_bool == b_bool
                } else {
                    panic!("Comparison eq on incompatible types")
                };
                Value::boolean(result)
            }
        }
        fn neq(a: any, b: any) -> bool {
            |args, heap| {
                let a = &args[0];
                let b = &args[1];
                let result = if let (Some(a_num), Some(b_num)) = (a.as_number(), b.as_number()) {
                    a_num != b_num
                } else if let (Some(a_str), Some(b_str)) = (a.as_string(heap), b.as_string(heap)) {
                    a_str != b_str
                } else if let (Some(a_bool), Some(b_bool)) = (a.as_boolean(), b.as_boolean()) {
                    a_bool != b_bool
                } else {
                    panic!("Comparison neq on incompatible types")
                };
                Value::boolean(result)
            }
        }
        fn lt(a: num, b: num) -> bool {
            |args, _heap| {
                let a = args[0].as_number().expect("lt expects number arguments");
                let b = args[1].as_number().expect("lt expects number arguments");
                Value::boolean(a < b)
            }
        }
        fn lte(a: num, b: num) -> bool {
            |args, _heap| {
                let a = args[0].as_number().expect("lte expects number arguments");
                let b = args[1].as_number().expect("lte expects number arguments");
                Value::boolean(a <= b)
            }
        }
        fn gt(a: num, b: num) -> bool {
            |args, _heap| {
                let a = args[0].as_number().expect("gt expects number arguments");
                let b = args[1].as_number().expect("gt expects number arguments");
                Value::boolean(a > b)
            }
        }
        fn gte(a: num, b: num) -> bool {
            |args, _heap| {
                let a = args[0].as_number().expect("gte expects number arguments");
                let b = args[1].as_number().expect("gte expects number arguments");
                Value::boolean(a >= b)
            }
        }
    }
    };
}
