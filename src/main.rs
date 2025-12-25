use crate::parser::parse_program;

mod parser;


fn main() {
    let input = "test = 23;";

    let res = parse_program(input).unwrap();

    println!("{:?}", res);
}
