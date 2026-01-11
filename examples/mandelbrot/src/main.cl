use print from std;
use map, filter, reduce, fold from functional;

let min_x = -2;
let max_x = 1;
let min_y =  -1;
let max_y =  1;

let width = 64;
let height = 64;
let max_iter = 32;

struct State {
  zx: num,
  zy: num,
  iter: num,
  escaped: bool
}

let mandel_iter = fn (cx: num, cy: num) -> num => {
    let final =
        array.range(0, max_iter)
        |> fold(
            State { zx: 0, zy: 0, iter: 0, escaped: false },
            fn (state) -> State => {

                if state.escaped {
                    state
                } else {
                    let zx2 = state.zx * state.zx - state.zy * state.zy + cx;
                    let zy2 = 2 * state.zx * state.zy + cy;
                    let escaped = zx2 * zx2 + zy2 * zy2 > 4;

                    let newState = State {
                        zx: zx2,
                        zy: zy2,
                        iter:  state.iter + 1,
                        escaped: escaped
                    };

                    newState
                }
            }
        );

    print(final)!;

    final.iter
};

let scale = fn (v: num, dimension: num, min: num, max: num) -> num => v / dimension * (max - min) + min;

let scale_x = scale(?, width, min_x, max_x);
let scale_y = scale(?, height, min_y, max_y);

let mandel = 
    array.range(0, height)
    |> map(fn (y) =>
        array.range(0, width)
        |> map(fn (x) => {
            let sx = scale_x(x);
            let sy = scale_y(y);

            let res = mandel_iter(sx,sy);

            return res;
        })
    );

let gradient = " .'`^,:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

let to_char = fn (v: num) => {
    if v == max_iter {
        return "@";
    } else {
        return gradient[
            math.floor(v * (string.len(gradient) - 1) / max_iter)
        ];
    }
};

mandel
|> map(fn (row) =>
    string.join(
        row |> map(fn(x) => to_char(x)),
        ""
    )!
)
|> map(print);