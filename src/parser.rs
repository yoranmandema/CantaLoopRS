use pest::Parser as PestParser;
use pest::pratt_parser::Assoc;
use pest::pratt_parser::PrattParser;
use pest_derive::Parser;

use crate::ast::builder::build_program;
use crate::ast::enums::Program;

#[derive(Parser)]
#[grammar = "src/grammar/grammar.pest"]
pub struct CantaLoopParser;

lazy_static! {
    pub static ref PRATT_PARSER: PrattParser<Rule> = {
        use pest::pratt_parser::{Op};

        PrattParser::new()
            .op(Op::infix(Rule::or, Assoc::Left))
            .op(Op::infix(Rule::and, Assoc::Left))
            .op(Op::infix(Rule::eq, Assoc::Left) | Op::infix(Rule::ne, Assoc::Left))
            .op(Op::infix(Rule::gt, Assoc::Left) | Op::infix(Rule::lt, Assoc::Left) | Op::infix(Rule::ge, Assoc::Left) | Op::infix(Rule::le, Assoc::Left))
            .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::sub, Assoc::Left))
            .op(Op::infix(Rule::mul, Assoc::Left) | Op::infix(Rule::div, Assoc::Left))
            .op(Op::infix(Rule::pow, Assoc::Right))
            .op(Op::prefix(Rule::not) | Op::prefix(Rule::neg) | Op::prefix(Rule::increment) | Op::prefix(Rule::decrement))
    };
}

pub fn parse_program(src: &str) -> Result<Program, pest::error::Error<Rule>> {
    // #region agent log
    let log_path = ".cursor/debug.log";
    let _ = std::fs::write(log_path, "");
    let log_line = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H1\",\"location\":\"parser.rs:29\",\"message\":\"parse_program entry\",\"data\":{{\"src_length\":{},\"src_preview\":{:?}}},\"timestamp\":{}}}\n", src.len(), src.chars().take(100).collect::<String>(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line.as_bytes()));
    // #endregion
    // #region agent log
    let test_input = "fn factorial(n) {";
    let log_line2 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H2\",\"location\":\"parser.rs:35\",\"message\":\"testing function_statement parse\",\"data\":{{\"test_input\":{:?}}},\"timestamp\":{}}}\n", test_input, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line2.as_bytes()));
    let test_func_result = CantaLoopParser::parse(Rule::function_statement, test_input);
    let log_line3 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H2\",\"location\":\"parser.rs:37\",\"message\":\"function_statement parse result\",\"data\":{{\"is_ok\":{},\"error\":{:?}}},\"timestamp\":{}}}\n", test_func_result.is_ok(), test_func_result.as_ref().err().map(|e| format!("{:?}", e)), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line3.as_bytes()));
    // #endregion
    // #region agent log
    let log_line4 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H3\",\"location\":\"parser.rs:40\",\"message\":\"testing block parse\",\"data\":{{\"src_preview\":{:?}}},\"timestamp\":{}}}\n", src.chars().take(50).collect::<String>(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line4.as_bytes()));
    let test_block_result = CantaLoopParser::parse(Rule::block, src);
    let log_line5 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H3\",\"location\":\"parser.rs:42\",\"message\":\"block parse result\",\"data\":{{\"is_ok\":{},\"error\":{:?}}},\"timestamp\":{}}}\n", test_block_result.is_ok(), test_block_result.as_ref().err().map(|e| format!("{:?}", e)), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line5.as_bytes()));
    // #endregion
    // #region agent log
    let log_line6 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H4\",\"location\":\"parser.rs:45\",\"message\":\"testing statement parse\",\"data\":{{\"test_input\":{:?}}},\"timestamp\":{}}}\n", test_input, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line6.as_bytes()));
    let test_stmt_result = CantaLoopParser::parse(Rule::statement, test_input);
    let log_line7 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H4\",\"location\":\"parser.rs:47\",\"message\":\"statement parse result\",\"data\":{{\"is_ok\":{},\"error\":{:?}}},\"timestamp\":{}}}\n", test_stmt_result.is_ok(), test_stmt_result.as_ref().err().map(|e| format!("{:?}", e)), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line7.as_bytes()));
    // #endregion
    // #region agent log
    let log_line8 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H5\",\"location\":\"parser.rs:50\",\"message\":\"about to parse program\",\"data\":{{}},\"timestamp\":{}}}\n", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line8.as_bytes()));
    // #endregion
    let parse_result = CantaLoopParser::parse(Rule::program, src);
    // #region agent log
    let log_line9 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H5\",\"location\":\"parser.rs:54\",\"message\":\"program parse result\",\"data\":{{\"is_ok\":{},\"error\":{:?}}},\"timestamp\":{}}}\n", parse_result.is_ok(), parse_result.as_ref().err().map(|e| format!("{:?}", e)), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line9.as_bytes()));
    // #endregion
    let mut pairs = parse_result?;
    let program = pairs.next().unwrap();
    build_program(program)
}