/// This is a comprehensive example demonstrating CantaLoop's documentation system.
///
/// Documentation can be attached to functions, constants, structs, modules, and let bindings.

/// Calculates the square of a number.
/// @param x The number to square
/// @returns The square of x (x * x)
fn square(x: num) -> num {
    return x * x;
}

/// Calculates the factorial of a number using recursion.
///
/// This function uses the standard recursive definition:
/// - factorial(0) = 1
/// - factorial(n) = n * factorial(n - 1)
///
/// @param n The number to compute the factorial for (must be >= 0)
/// @returns The factorial of n
fn factorial(n: num) -> num {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

/// A constant representing the value of pi.
/// This is an approximation with 5 decimal places.
const PI = 3.14159;

/// A variable with documentation.
/// Note: let bindings can have documentation, but it's less common than for functions/constants.
let greeting = "Hello";

/// Represents a point in 2D space.
/// @param x The x-coordinate
/// @param y The y-coordinate
// struct Point {
//     x: num,
//     y: num
// }

// /// Calculates the distance between two points using the Euclidean distance formula.
// /// @param p1 First point
// /// @param p2 Second point
// /// @returns The distance between p1 and p2
// fn distance(p1: Point, p2: Point) -> num {
//     let dx = p2.x - p1.x;
//     let dy = p2.y - p1.y;
//     sqrt(dx * dx + dy * dy)
// }

/// Example of an effectful function with effect documentation.
///
/// This function demonstrates how to document functions that have execution effects.
/// The @effects tag is important for effectful functions (those with ~> return type).
///
/// @param message The message to print
/// @effects Executes I/O operation to write to stdout
/// @returns void (nothing)
fn print_message(message: str) -> void {
    use print from std;
    print(message);
}
