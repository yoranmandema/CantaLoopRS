// Complex function composition chains

fn add(a: num, b: num) -> num {
    return a + b;
}

fn mul(a: num, b: num) -> num {
    return a * b;
}

fn square(n: num) -> num {
    return n * n;
}

fn double(n: num) -> num {
    return n * 2;
}

// Create thunks for partial application
let add5 = add(5);
let add10 = add(10);
let mul3 = mul(3);
let mul4 = mul(4);

// Long composition chain: add5 |> add10 |> mul3 |> square |> double
// When applied to 2: add5(2)=7, add10(7)=17, mul3(17)=51, square(51)=2601, double(2601)=5202
let complex_chain = add5 |> add10 |> mul3 |> square |> double;
print(complex_chain(2)!)!;

// Reverse chain: double <| square <| mul3 <| add10 <| add5
// Same result, different direction
let reverse_chain = double <| square <| mul3 <| add10 <| add5;
print(reverse_chain(2)!)!;

// Mixing forward and reverse (though typically you'd stick with one direction)
let mixed_chain = add5 |> mul3 <| square;
// This reads as: square(mul3(add5(x)))
print(mixed_chain(3)!)!; // add5(3)=8, mul3(8)=24, square(24)=576

// Building up compositions incrementally
let step1 = add10;
let step2 = step1 |> mul2;
let step3 = step2 |> square;

print(step3(5)!)!; // add10(5)=15, mul2(15)=30, square(30)=900

// Using composition for data transformation pipelines
let transform = add5 |> double |> square;
print("Transform 10: " + transform(10)!)!; // add5(10)=15, double(15)=30, square(30)=900

