# Student Grade Management System

A real-world example project demonstrating CantaLoop's capabilities for data processing, array manipulation, and functional programming patterns.

## Overview

This project implements a complete grade management system for tracking student performance across assignments and exams. It showcases:

- **Array Processing**: Iterating over arrays, calculating statistics
- **Modular Design**: Separate modules for grade calculations and student management
- **Function Composition**: Using reducer patterns (`|> sum`) for aggregation
- **Data Aggregation**: Computing averages, weighted grades, and statistics
- **Real-World Logic**: Letter grade conversion, weighted averages, and report generation

## Project Structure

```
grade_manager/
├── melon.json          # Project configuration
├── README.md           # This file
└── src/
    ├── main.mln        # Main entry point - demonstrates the system
    ├── grades.mln      # Grade calculation utilities
    └── students.mln    # Student record management
```

## Features

### Grade Calculations (`grades.mln`)

- `calculate_average()` - Compute simple average of grades
- `calculate_weighted_average()` - Weighted average with custom weights
- `letter_grade()` - Convert numeric grades to letter grades (A-F)
- `calculate_final_grade()` - Combine assignment and exam grades with weights
- `highest_grade()` / `lowest_grade()` - Find min/max in grade lists
- `grade_statistics()` - Comprehensive statistics (avg, high, low)

### Student Management (`students.mln`)

- `student_performance()` - Calculate overall student performance (60% assignments, 40% exams)
- `generate_student_report()` - Create formatted reports with all statistics

### Main Program (`main.mln`)

Demonstrates:
- Creating student records with assignment and exam grades
- Generating individual student reports
- Computing class-wide statistics
- Weighted grade calculations with custom weights

## Example Output

```
========================================
  Student Grade Management System      
========================================

=== Student Report: Alice ===
Assignment Average: 88.4
Assignment Grades: [85, 92, 88, 90, 87]
Exam Average: 91.0
Exam Grades: [91, 89, 93]
Final Grade: 89.44 (B)

Assignment Statistics:
  Average: 88.4, Highest: 92, Lowest: 85

Exam Statistics:
  Average: 91.0, Highest: 93, Lowest: 89

...
```

## Usage

Run with the melon compiler:

```bash
melon examples/grade_manager
```

## Extending

This project demonstrates how to:

1. **Add new calculation methods**: Extend `grades.mln` with additional statistical functions
2. **Create new report formats**: Modify `generate_student_report()` to change output format
3. **Implement new weighting schemes**: Adjust weights in `student_performance()`
4. **Add data validation**: Implement bounds checking for grades (0-100)

## Dependencies

Uses standard library functions:
- `std.print` - Output
- `std.format_number` - Number formatting with decimal places
- `std.array_length` - Get array length
- `math.sum` - Sum reducer for arrays

