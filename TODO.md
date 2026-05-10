- [x] Record
  - [x] `insert`
  - [x] `get`
  - [x] `has`
  - [x] `remove`
  - [x] `keys`
  - [x] `values`
  - [x] `entries`
- [ ] Type
  - [ ] `cast`
  - [x] `typeof`
  - [x] `lazy`
  - [x] `force`
  - [x] `fn`
  - [x] `list`
  - [x] `map`
- [ ] Control Flow
  - [x] `if`
  - [ ] `exit`
- [ ] Special
  - [x] `call`
  - [x] `eval`
  - [x] `recur`
  - [ ] `import`

- [ ] Recursive form eval

## Lazy Arguments

### Problem

Functions like `defn`, `def`, and `if` do this internally.

When calling `(if true (print "hey"))`, the `if` will evaluate condition first (`true`) before evaluating the body `(print "hey")` if the condition is true. In this case, `if` is intentionally breaking the "eval all arguments" rule.

To allow user-defined functions to also break this rule, we need to rethink how laziness is handled in Quip.

For example, we could have a user-defined `if` statement, assuming the second argument was marked as lazy (like `(defn if [cond 'body])`). If the user called it and wanted return the value of a symbol, such as `(if true a)`, this wouldn't work as expected. The body, `a`, would be evaluated as the symbol `a` instead of its value, since the evaluator wouldn't evaluate the body of the `if`. But, when the if evaluates the condition to true and goes to evaluate the body, there's a problem.

Because the body is just the symbol `a`, when evaluating `(call body)`, it just evaluates to `a`.

There is a similar problem when we pass a function to `if`, like `(if true (fn [] (print "yay")))`. Because `fn` constructs a Function type, rather than existing as the type itself, the `if` would evaluate `body`, which is a form that calls to `fn`. However, this call is now evaluated within the `if` for one, but is also never called since `(call body)` just evaluates `body` and calls the `fn` form, which returns a Function but doesn't call it because the evaluation stopped once after evaluating the `(fn ...)` form.

### Solution

Add a `Lazy { scope: DefaultKey, inner: Box<Expr>> }` kind to the `ExprKind` enum. This would wrap around arguments that are marked as lazy in a function definition, such as the body of `if`, where `(defn if [cond 'body] ...)`. When a Lazy is evaluated, such as `(call body)`, the evaluator temporarily moves into the scope of the Lazy to ensure that symbols and functions that need to be run within their originating scopes are evaluated as expected, instead of the scope of the called function.

This would mean that `(defn run ['thing] (call thing))` would call `thing` in the scope that calls `run`. For example, `(run (def a 1))` would define `a` as `1` in the scope that calls `run`, instead of within the scope of `run` where it would be dropped after `run` finishes.
