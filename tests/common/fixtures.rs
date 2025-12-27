/// Test fixtures - sample code snippets for testing

pub const SIMPLE_ASSIGNMENT: &str = "let x = 42";

pub const SIMPLE_EXPRESSION: &str = "1 + 2";

pub const SIMPLE_FUNCTION: &str = r#"
fn test() {
    let x = 1
}
"#;

pub const FUNCTION_WITH_ARGS: &str = r#"
fn add(a, b) {
    return a + b
}
"#;

pub const IF_STATEMENT: &str = r#"
if (true) {
    x = 1
}
"#;

pub const IF_ELSE_STATEMENT: &str = r#"
if (x == 1) {
    let y = 2
} else {
    let y = 3
}
"#;

pub const ARITHMETIC_EXPRESSIONS: &str = r#"
let x = 1 + 2 * 3
let y = (1 + 2) * 3
let z = 2 ^ 3
"#;

pub const BOOLEAN_EXPRESSIONS: &str = r#"
let x = true and false
let y = true or false
let z = not true
"#;

pub const COMPARISON_EXPRESSIONS: &str = r#"
let x = 1 == 1
let y = 1 != 2
let z = 1 > 0
let w = 1 < 2
"#;

pub const VARIABLE_ASSIGNMENT: &str = r#"
let x = 10
let y = x + 5
"#;

pub const FUNCTION_CALL: &str = r#"
fn greet(name) {
    print(name)
}

greet("world")
"#;

pub const RECURSIVE_FUNCTION: &str = r#"
fn factorial(n) {
    if (n == 0) {
        return 1
    } else {
        return n * factorial(n - 1)
    }
}
"#;

