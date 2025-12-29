use print from std;

// Assignment operators: += and -=

let x = 10;

// Increment by value
x += 5;
print(x)!; // Should be 15

// Decrement by value
x -= 3;
print(x)!; // Should be 12

// Using in loops
let sum = 0;
let i = 1;

while i <= 10 {
    sum += i;
    i = i + 1;
}

print(sum)!; // Sum of 1 to 10

// Accumulating with +=
fn accumulate_multiplier(start: num, count: num, multiplier: num) -> num {
    let result = start;
    let i = 0;

    while i < count {
        result += multiplier;
        i = i + 1;
    }

    return result;
}

print(accumulate_multiplier(10, 5, 3)!)!; // 10 + (5 * 3) = 25

// Using -= for countdown
let counter = 20;

while counter > 0 {
    print(counter)!;
    counter -= 2;
}

