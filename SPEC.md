# Quip - Spec

## Rules

1. Function calls: `(<symbol> <args...>)`
2. Symbols are evaluated greedily once, on evaluation at runtime: `a -> 2, [a] -> [2], (* a a) -> 4` (except as the symbol of a function call)

## Evaluation (and raw with `'`)

By default, symbols will automatically evaluate to their underlying values at runtime.

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

## Raw Calls (with `@`)

`@` on a call makes all args raw. `@` on an arg within a raw call evaluates it. `'` within a raw call adds an extra layer of laziness.

```clojure
(def 'a 2)
(typeof a)  ;; -> number
(@typeof a) ;; -> symbol

(@def a 2)          ;; equivalent to (def 'a 2)
(@def a @(foo b))   ;; `a` is raw, `(foo b)` is evaluated
(@def 'a 2)         ;; def receives the raw symbol 'a
```

## Functions

Functions always follow `(<symbol> <args...>)`. For example, `(+ 1 2)` adds `2` to `1`.

### Defining Functions

```clojure
(@def add (fn [a b] (+ a b)))

(print add)       ;; -> (Function [a b] (+ a b))
(print (add 2 2)) ;; -> 4
```

### Scopes

```clojure
(@fn! [..] ..) ;; Scopeless functions
```
