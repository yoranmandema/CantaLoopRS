mod utils; // Defines the module name
use array_length from std;

pub fn add(a: num, b: num) -> num { // This is a public function
    return a + b;
}

let magicNumber = 5; // This is a private variable

pub const PI = 3.14; // This is a public constant
pub const PI2 = PI * 2;
pub const dayInSeconds = 60 * 60 * 24;

pub fn addMagicNumber(a: num) -> num {
    return add(a, magicNumber); // This is a public function
}

pub fn calculate_something(grades: [num], weights: [num]) -> num {
    let total_weight = 1;
    let weighted_sum = 0;
    let len = array_length(grades)!;

    loop i = 0 {
        if i >= len {
            break;
        }
        weighted_sum = weighted_sum + ((grades[i]) * (weights[i]));
        total_weight = total_weight + weights[i];
        i = i + 1;
    }

    
    return weighted_sum / total_weight;
}
