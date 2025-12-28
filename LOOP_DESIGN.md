# Scoped Loop Design: `loop var1 = val1, var2 = val2, ... { body }`

## Overview

The scoped loop syntax `loop a = 0, b = 1, i = 0 { body }` creates a **loop frame** that:
- Initializes local variables **inside** the loop scope
- Prevents state from leaking outside the loop
- Allows `break value` to return a value from the loop
- Provides a stack-safe, non-recursive alternative to tail recursion

## Why This Design is Better Than Traditional While Loops

### 1. **Encapsulation and State Isolation**

Traditional while loops require variables to be declared **outside** the loop:

```rust
// Traditional approach - variables leak outside
let mut a = 0;
let mut b = 1;
let mut i = 0;
while i < n {
    if i >= n { break; }
    let next = a + b;
    a = b;
    b = next;
    i = i + 1;
}
// a, b, i are still accessible here - they leaked!
```

With scoped loops, variables are **contained** within the loop:

```rust
let result = loop a = 0, b = 1, i = 0 {
    if (i >= n) { break a }
    let next = a + b
    a = b
    b = next
    i = i + 1
}
// a, b, i are NOT accessible here - they're scoped to the loop
```

**Benefits:**
- Prevents accidental reuse of loop variables
- Makes loop state explicit and self-contained
- Reduces namespace pollution
- Makes it clear which variables are loop-local vs. outer scope

### 2. **Expression-Valued Loops**

Traditional while loops are **statements** - they don't return values. You must use mutable state:

```rust
let mut result = 0;
while condition {
    // ... compute result ...
}
// result is separate from the loop
```

Scoped loops are **expressions** - they return the break value:

```rust
let result = loop a = 0, b = 1 {
    if (i >= n) { break a }  // Returns 'a' as the loop's value
    // ...
}
// result is the value returned by break
```

**Benefits:**
- More functional style - loops produce values
- No need for separate mutable variables
- Composable - can use loop expressions in larger expressions
- Matches the functional programming paradigm

### 3. **Stack-Safe Alternative to Tail Recursion**

Tail-recursive functions can cause stack overflow in languages without TCO:

```rust
fn fib_tail(n: num, a: num, b: num) -> num {
    if (n == 0) { return a }
    return fib_tail(n - 1, b, a + b)  // Tail call - but might stack overflow
}
```

Scoped loops provide the same iteration pattern **without recursion**:

```rust
fn fib(n: num) -> num {
    loop a = 0, b = 1, i = 0 {
        if (i >= n) { break a }
        let next = a + b
        a = b
        b = next
        i = i + 1
    }
}
```

**Benefits:**
- **Always stack-safe** - no recursion, no stack growth
- Same iteration pattern as tail recursion
- More efficient - no function call overhead
- Works in any language/runtime, even without TCO

### 4. **Clearer Intent and Readability**

The initialization syntax makes the loop's state **explicit**:

```rust
loop a = 0, b = 1, i = 0 {  // Clear: these are the loop's state variables
    // ...
}
```

vs.

```rust
let mut a = 0;  // Is this used in the loop? Or elsewhere?
let mut b = 1;
let mut i = 0;
while condition {  // Hard to see which variables are loop-local
    // ...
}
```

## Bytecode Compilation Strategy

### Loop Frame Structure

The loop frame is implemented as a **local scope** within the VM's call frame:

```
CallFrame {
    code: [...],
    ip: ...,
    locals: [..., loop_var1, loop_var2, loop_var3, break_slot, ...]
    //              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //              Loop variables are in the same locals array
}
```

### Bytecode Sequence

For `loop a = 0, b = 1, i = 0 { body }`:

```rust
// 1. Initialize loop variables (evaluate initializers)
LdNum(0)           // Push initial value for 'a'
StVar(a_slot)      // Store in loop-local slot
LdNum(1)           // Push initial value for 'b'
StVar(b_slot)      // Store in loop-local slot
LdNum(0)           // Push initial value for 'i'
StVar(i_slot)      // Store in loop-local slot

// 2. Loop start marker
LOOP_START:        // IP position recorded

// 3. Loop body
// ... emit body bytecode ...

// 4. Jump back to start
Jmp(LOOP_START)

// 5. Loop end marker
LOOP_END:          // IP position recorded
```

### Break with Value

When `break value` is encountered:

```rust
// Inside loop body:
if (i >= n) {
    break a  // Break with value
}
```

Compiles to:

```rust
// Condition check
LdVar(i_slot)
LdVar(n_slot)
Ge
JmpIfFalse(continue_label)

// Break with value
LdVar(a_slot)           // Load the break value
StVar(break_slot)       // Store in loop's break_slot
Jmp(LOOP_END)           // Jump to end of loop

continue_label:
// ... rest of loop body ...
```

After the loop completes:

```rust
// If loop is an expression, load the break value
LdVar(break_slot)  // Push break value onto stack
```

### Loop Frame Implementation Details

1. **Variable Slots**: Loop variables use **local variable slots** in the current call frame
   - Same mechanism as function parameters
   - No special "loop frame" - just scoped variables

2. **Break Slot**: For expression-valued loops, allocate a `break_slot` variable
   - Stores the value passed to `break`
   - Loaded after loop completes if loop is an expression

3. **Scope Management**: Loop variables are in the **same scope** as the loop body
   - Body can access loop variables
   - Body can shadow loop variables with `let`
   - Loop variables are **not** accessible after loop

4. **Jump Targets**: 
   - `Jmp(LOOP_START)` - continue to next iteration
   - `Jmp(LOOP_END)` - break out of loop
   - Both use absolute IP positions (patched after loop body is emitted)

## Interaction with Lazy Thunks and Functional Style

### 1. **Lazy Evaluation in Loop Bodies**

Loop bodies can contain **lazy thunks**:

```rust
let result = loop a = 0, b = 1 {
    if (condition) { break a }
    let lazy_sum = add(a, b)  // Creates a thunk (not invoked)
    a = b
    b = lazy_sum!  // Force evaluation with !
}
```

**Behavior:**
- Thunks created in loop body are **evaluated lazily**
- Only forced when `!` is used
- Loop variables can hold thunks
- Break values can be thunks (evaluated when loop returns)

### 2. **Functional Composition**

Loops can be used in **functional pipelines**:

```rust
let doubled = loop i = 0, acc = [] {
    if (i >= 10) { break acc }
    let new_acc = append(acc, i * 2)  // Functional style
    i = i + 1
    acc = new_acc
}
```

**Benefits:**
- Loops produce values (expressions)
- Can be composed with other expressions
- No mutable state outside the loop
- Matches functional programming patterns

### 3. **Thunk Forcing at Loop Boundaries**

When a loop returns a value (via `break`), thunks are **forced**:

```rust
let result = loop a = 0 {
    if (condition) { break lazy_computation() }  // Thunk returned
    // ...
}
// result is forced here (thunk evaluated)
```

This matches the language's **boundary forcing** semantics:
- Function returns force thunks
- Loop returns force thunks
- Expression statements don't force (unless `!` is used)

### 4. **Stack Safety with Thunks**

The combination of scoped loops + thunks provides **stack safety**:

```rust
// Recursive version (might stack overflow)
fn process_list(list: List) -> num {
    if (empty(list)) { return 0 }
    return head(list) + process_list(tail(list))
}

// Loop version (always stack-safe)
fn process_list(list: List) -> num {
    loop acc = 0, remaining = list {
        if (empty(remaining)) { break acc }
        acc = acc + head(remaining)
        remaining = tail(remaining)
    }
}
```

**Key Insight:**
- Loops provide iteration without recursion
- Thunks provide lazy evaluation without eager computation
- Together: stack-safe, lazy, functional iteration

## Implementation Plan

1. **Grammar**: Extend `loop_statement` to support initialization syntax
2. **AST**: Add loop initialization variables to `Statement::Loop`
3. **Semantic Analysis**: Create loop-local scope for initialization variables
4. **Bytecode**: Emit initialization code before loop body
5. **VM**: No changes needed (uses existing local variable mechanism)

## Example: Fibonacci

```rust
fn fib(n: num) -> num {
    loop a = 0, b = 1, i = 0 {
        if (i >= n) { break a }
        let next = a + b
        a = b
        b = next
        i = i + 1
    }
}
```

**Compiled Bytecode:**
```
LdNum(0)      ; Initialize a
StVar(0)      ; a_slot = 0
LdNum(1)      ; Initialize b
StVar(1)      ; b_slot = 1
LdNum(0)      ; Initialize i
StVar(2)      ; i_slot = 0

LOOP_START:
LdVar(2)      ; Load i
LdVar(n)      ; Load n (parameter)
Ge            ; i >= n
JmpIfFalse(CONTINUE)

; Break with value
LdVar(0)      ; Load a
StVar(3)      ; break_slot = a
Jmp(LOOP_END)

CONTINUE:
LdVar(0)      ; Load a
LdVar(1)      ; Load b
Add           ; a + b
StVar(4)      ; next = a + b
LdVar(1)      ; Load b
StVar(0)      ; a = b
LdVar(4)      ; Load next
StVar(1)      ; b = next
LdVar(2)      ; Load i
LdNum(1)      ; Push 1
Add           ; i + 1
StVar(2)      ; i = i + 1

Jmp(LOOP_START)

LOOP_END:
LdVar(3)      ; Load break_slot (loop return value)
Ret           ; Return from function
```

This design provides a **clean, functional, stack-safe** iteration mechanism that integrates seamlessly with the language's lazy evaluation and functional programming model.

