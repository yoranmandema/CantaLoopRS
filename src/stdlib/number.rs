use crate::core::engine::StdModule;
use crate::core::vm::Value;

lazy_static::lazy_static! {
    pub static ref NUMBER_MODULE: StdModule = crate::melon_module! {
    module number {
        fn add(a: num, b: num) -> num {
            |args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number(a + b)
            }
        }
        fn mul(a: num, b: num) -> num {
            |args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number(a * b)
            }
        }
        fn clamp(val: num, min: num, max: num) -> num {
            |args, _heap| {
                let val = args[0].as_number().expect("expected number as input");
                let min = args[1].as_number().expect("expected number as min boundary");
                let max = args[2].as_number().expect("expected number as max boundary");

                if min > max {
                    panic!("clamp: min must be <= max (got min={}, max={})", min, max);
                }

                let clamped = val.clamp(min, max);
                Value::number(clamped)
            }
        }
    }
    };
}
