use cantaloop::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test to reproduce the parsing issue scenario from students.mln.
///
/// Note: the original file path used here no longer exists in this repo layout,
/// so we embed a representative snippet instead of `include_str!`.
#[test]
fn test_exact_students_mln_reproduction() {
    // Representative content (updated to current `use <name> from <module>;` syntax)
    let students_mln = r#"
mod students;

use calculate_average from grades;
use calculate_final_grade from grades;
use grade_statistics from grades;
use highest_grade from grades;
use lowest_grade from grades;
use array_length from std;

pub fn letterGrade(grade: num) -> string {
    if grade >= 90 {
        return "A";
    }
    return "F";
}
"#;
    
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
use calculate_average from grades;
"#;
    
    let use2 = r#"mod test;
use calculate_average from grades;
use calculate_final_grade from grades;
"#;
    
    let use3 = r#"mod test;
use calculate_average from grades;
use calculate_final_grade from grades;
use grade_statistics from grades;
"#;
    
    let use4 = r#"mod test;
use calculate_average from grades;
use calculate_final_grade from grades;
use grade_statistics from grades;
use highest_grade from grades;
"#;
    
    let use5 = r#"mod test;
use calculate_average from grades;
use calculate_final_grade from grades;
use grade_statistics from grades;
use highest_grade from grades;
use lowest_grade from grades;
"#;
    
    let use6 = r#"mod test;
use calculate_average from grades;
use calculate_final_grade from grades;
use grade_statistics from grades;
use highest_grade from grades;
use lowest_grade from grades;
use array_length from std;
"#;
    
    let use6_pub_fn = r#"mod test;
use calculate_average from grades;
use calculate_final_grade from grades;
use grade_statistics from grades;
use highest_grade from grades;
use lowest_grade from grades;
use array_length from std;

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

