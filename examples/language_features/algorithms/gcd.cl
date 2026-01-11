// Greatest Common Divisor (GCD) using subtraction-based Euclidean algorithm
// (since modulo operator is not available, we use subtraction)

fn gcd(a: num, b: num) -> num {
    if a == b {
        return a;
    }
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    if a > b {
        return gcd(a - b, b)!;
    } else {
        return gcd(a, b - a)!;
    }
}

// Test cases
print(gcd(48, 18)!)!; // Should be 6
print(gcd(100, 25)!)!; // Should be 25
print(gcd(17, 5)!)!; // Should be 1

// Iterative version using loop
fn gcd_loop(a: num, b: num) -> num {
    let num_a = a;
    let num_b = b;

    while !(num_a == num_b) {
        if num_a > num_b {
            num_a = num_a - num_b;
        } else {
            num_b = num_b - num_a;
        }
    }

    return num_a;
}

print(gcd_loop(48, 18)!)!; // Should be 6

