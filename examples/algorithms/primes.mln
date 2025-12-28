// Prime number checking and generation

// Helper function to check divisibility (n is divisible by d)
fn is_divisible(n: num, d: num) -> bool {
    // Check if n is divisible by d by repeatedly subtracting d
    loop current = n {
        if current < d {
            break current == 0;
        }
        if current == d {
            break true;
        }
        current = current - d;
    }
}

fn is_prime(n: num) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    
    // Check if divisible by 2
    if is_divisible(n, 2)! {
        return false;
    }

    // Check odd divisors from 3 up to sqrt(n)
    let i = 3;
    while i * i <= n {
        if is_divisible(n, i)! {
            return false;
        }
        i = i + 2;
    }

    return true;
}

// Test prime checking
print(is_prime(17)!)!; // Should be true
print(is_prime(20)!)!; // Should be false
print(is_prime(97)!)!; // Should be true

// Find first N primes
fn find_primes(count: num) -> num {
    let found = 0;
    let candidate = 2;
    let last_prime = 0;

    while found < count {
        if is_prime(candidate)! {
            last_prime = candidate;
            found = found + 1;
        }
        candidate = candidate + 1;
    }

    return last_prime;
}

print(find_primes(10)!)!; // 10th prime

