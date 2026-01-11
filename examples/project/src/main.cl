use mul, fold from math;
use print from std;

let xs = [1, 2, 3, 4, 5];
let a = xs |> fold(1, add);

print(a)!;