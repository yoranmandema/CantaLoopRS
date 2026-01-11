use print from std;

// While loop example
// While loops continue as long as the condition is true

let count = 0;

while count < 5 {
    print(count)!;
    count = count + 1;
}

// While loop with early break
let i = 0;

while i < 10 {
    if i == 5 {
        break;
    }
    print(i)!;
    i = i + 1;
}

// Factorial using while loop
fn factorial_while(n: num) -> num {
    let result = 1;
    let i = 1;

    while i <= n {
        result = result * i;
        i = i + 1;
    }

    return result;
}

print(factorial_while(5)!)!;

