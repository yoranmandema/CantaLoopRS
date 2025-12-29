use CantaLoopRS::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test to reproduce the exact issue from students.mln
#[test]
fn test_exact_students_mln_reproduction() {
    // Read the actual file content
    let students_mln = include_str!("../examples/grade_manager/src/students.mln");
    
    println!("\n=== Parsing actual students.mln file ===");
    let parse_result = CantaLoopParser::parse(Rule::program, students_mln);
    match parse_result {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully (unexpected!)");
            let program = pairs.next().unwrap();
            println!("  Program span length: {}", program.as_span().end());
        }
        Err(e) => {
            println!("✗ Parse failed (expected): {:?}", e);
            println!("  Location: {:?}", e.location);
            println!("  Line/col: {:?}", e.line_col);
            
            // Show the exact line where it fails
            let lines: Vec<&str> = students_mln.lines().collect();
            let pos = match e.location {
                pest::error::InputLocation::Pos(p) => p,
                pest::error::InputLocation::Span((s, _)) => s,
            };
            if true {
                // Find which line this position is on
                let mut current_pos = 0;
                for (line_num, line) in lines.iter().enumerate() {
                    let line_start = current_pos;
                    let line_end = current_pos + line.len() + 1; // +1 for newline
                    if pos >= line_start && pos < line_end {
                        println!("  Failing line {}: {}", line_num + 1, line);
                        println!("  Position in line: {}", pos - line_start);
                        break;
                    }
                    current_pos = line_end;
                }
            }
        }
    }
    
    // Test progressive parsing: add one use statement at a time
    println!("\n=== Progressive test: adding use statements one by one ===");
    
    let base = r#"mod test;
"#;
    
    let use1 = r#"mod test;
use grades.calculate_average;
"#;
    
    let use2 = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
"#;
    
    let use3 = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
"#;
    
    let use4 = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
"#;
    
    let use5 = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
"#;
    
    let use6 = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;
"#;
    
    let use6_pub_fn = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

pub fn test() -> num { return 42; }
"#;
    
    let test_cases = vec![
        ("base", base),
        ("use1", use1),
        ("use2", use2),
        ("use3", use3),
        ("use4", use4),
        ("use5", use5),
        ("use6", use6),
        ("use6 + pub fn", use6_pub_fn),
    ];
    
    for (name, code) in test_cases {
        let result = CantaLoopParser::parse(Rule::program, code);
        match result {
            Ok(_) => println!("  {}: ✓", name),
            Err(e) => {
                let pos = match e.location {
                    pest::error::InputLocation::Pos(p) => p,
                    pest::error::InputLocation::Span((s, _)) => s,
                };
                println!("  {}: ✗ Failed at Pos({})", name, pos);
                if name == "use6 + pub fn" {
                    println!("    This is where it fails!");
                }
            }
        }
    }
}

