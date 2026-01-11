use print from std;

// Test closure with expression body
let add_one = fn(x: num) => x + 1;
print(add_one(5)!)!;

// Test closure with block body
let multiply = fn(a: num, b: num) => {
    return a * b;
};
print(multiply(3, 4)!)!;

// Test closure in map
let numbers = [1, 2, 3, 4, 5];
let doubled = numbers |> map(fn(x) => x * 2);
print(doubled)!;

