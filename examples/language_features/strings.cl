use print from std;

// String manipulation examples

// String concatenation
let greeting = "Hello";
let name = "World";
let message = greeting + ", " + name + "!";
print(message)!;

// Building strings with loops
fn build_countdown_message(n: num) -> string {
    let message = "Countdown: ";
    let i = n;

    while i > 0 {
        i = i - 1;
    }
    // Add the final message
    return message + "Blastoff!";
}

print(build_countdown_message(5)!)!;

// Concatenating multiple strings
let part1 = "The";
let part2 = "quick";
let part3 = "brown";
let sentence = part1 + " " + part2 + " " + part3 + " fox";
print(sentence)!;

