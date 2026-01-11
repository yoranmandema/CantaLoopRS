use print from std;

// Higher-order functions: functions that operate on other functions/thunks

fn add(a: num, b: num) -> num {
    return a + b;
}

fn mul(a: num, b: num) -> num {
    return a * b;
}

fn double(n: num) -> num {
    return n * 2;
}

fn square(n: num) -> num {
    return n * n;
}

// Higher-order function with thunks
// A function that takes a thunk and applies it
fn apply_thunk(f: num ~> num, x: num) -> num {
    return f(x)!;
}

// Create thunks (partially applied functions)
let add5 = add(5);
let add10 = add(10);
let mul3 = mul(3);

// Apply thunks using higher-order function
print("apply_thunk(add5, 10): " + apply_thunk(add5, 10)!)!;
print("apply_thunk(mul3, 7): " + apply_thunk(mul3, 7)!)!;

// More practical: map-like operation using recursion
// Applies a thunk n times to a value
fn map_recursive(f: num ~> num, value: num, n: num) -> num {
    if n <= 0 {
        return value;
    } else {
        let transformed = f(value)!;
        return map_recursive(f, transformed, n - 1)!;
    }
}

let add_one = add(1);
print("map_recursive(add_one, 0, 5): " + map_recursive(add_one, 0, 5)!)!; // Applies add_one 5 times to 0: 5

// Transform a value multiple times
let double_thunk = double();
print("map_recursive(double, 2, 3): " + map_recursive(double_thunk, 2, 3)!)!; // 2 -> 4 -> 8 -> 16

