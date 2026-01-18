use crate::core::engine::StdModule;
use crate::core::vm::Value;

lazy_static::lazy_static! {
    pub static ref STD_MODULE: StdModule = crate::melon_module! {
    module std {
        // `print` is effectful and accepts any value (it stringifies at runtime).
        fn print(v: any) ~> str {
            |args, heap| {
                let s = args[0].value_to_string(heap);
                println!("{}", s);
                Value::string_with_heap(String::new(), heap)
            }
        }
        fn format_number(n: num, decimals: num) -> str {
            |args, heap| {
                let n = args[0].as_number().expect("expected number");
                let decimals = args[1].as_number().expect("expected number") as i32;
                let formatted = format!("{:.1$}", n, decimals as usize);
                Value::string_with_heap(formatted, heap)
            }
        }
    }
    };
}
