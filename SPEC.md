# Quip - Spec

## Rules

1. Function calls: `(<symbol> <args...>)`
2. Symbols are evaluated greedily once, on evaluation at runtime: `a -> 2, [a] -> [2], (* a a) -> 4` (except as the symbol of a function call)

## Evaluation (and raw with `'`)

By default, symbols will automatically evaluate to their underlying values at runtine.

```clojure
(let 'a 1) ;; "'a" is marked as a raw symbol
(let 'b 2) ;; "'b" is also marked as a raw symbol

(print a)  ;; -> 1
(print 'a) ;; -> a

(print (add a b)) ;; -> 3

(print [a b])   ;; -> [1 2]
(print ['a b])  ;; -> [a 2]
(print ['a 'b]) ;; -> [a b]
(print '[a b])  ;; -> [a b]
```

## Functions

Functions are the core of Quip. They always follow this call pattern `(<symbol> <args...>)` where `<symbol>` is the name of the function to call and `<args...>` is a set of args separated by space. For example, `(+ 1 2)` will add `2` to `1`.

### Defining Functions

In Quip, `fn` is a function that takes in an array of symbols as arguments and one or more functions/values to execute, then constructs the underlying Function type from that.

<!--
Problems: Because arguments are an array of symbols, they will need to be lazy. And because the function body contains calls, we also don't want them to be evaluated, so they're lazied as well.

We should add an exception to the core s-expression logic where (fn [] ..) can be wrapped within parens to call it, `((fn [a b] (+ a b)) 2 2) -> 4`. Then, one other exception, when evaluating symbols that exist at runtime in an s-expression, to evaluate and call the inner fn. For example, `(add 2 2)` might unwrap to `((fn [a b] (+ a b)) 2 2)` before being executed. This allows the entire function to be lazied instead of the args and each body expression. Plus, it provides a standard method of evaluating anonymous functions along.

Problem: Functions don't contain names nor position in source as they're not constructed until they're called. Though I think this was Stack's problem as well.
-->

```clojure
(def add '(fn [a b] (+ a b))) ;; define a function that adds two numbers

(print add)       ;; -> (Function [a b] (+ a b))
(print (add 2 2)) ;; -> 4
```

### Scopes

```clojure
(fn! '[..] ..) ;; Scopeless functions
```
