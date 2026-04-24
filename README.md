# Quip

In development, see below for details.

## Purpose

**Goals:**

1. To have strict and consistent rules that make Quip easy to implement and comprehend.
2. To include a simple core library while supporting the bootstrapping of language features from within userspace,
3. To make it easy to embed and run with custom intrinsics.

**Non-Goals:**

1. To be a useful language outside of embeddable scripting.

## Syntax

Quip is heavily inspired by the simplistic syntax of lispy languages such as Clojure and Common Lisp. Here is an example of what Quip looks like:

```clojure
(defn each [list cb]
  (call (fn [i len]
    (call (if (< i len)
      (do
        (call cb (nth i list))
        '(recur (+ i 1) len)))))
  0 (len list)))

(def list [])
(each "hello" (fn [el] (set list (push list el))))

(= list ["h" "e" "l" "l" "o"])
```

## Evaluation

Arguments passed to a form or a user-defined function will be evaluated once before binding.

```clojure
(print (+ 1 2)) ;; -> 3

;; `def` only evaluates the second argument.
(def a 1)
(def b 2)

(def c a) ;; (= c a) -> true

(print a)       ;; -> 1
(print b)       ;; -> 2
(print (+ a b)) ;; -> 3
```

To bypass evaluation and pass the raw expression, use `(lazy <expr>)` or `'<expr>`.

```clojure
(def a 1)

(print a)        ;; -> 1
(print (lazy a)) ;; -> a
(print 'a)       ;; -> a

(print (+ 1 2))        ;; -> 3
(print (lazy (+ 1 2))) ;; -> (+ 1 2)
(print '(+ 1 2))       ;; -> (+ 1 2)
```

### Special Cases

There are some special cases to

## Functions

Functions can be constructed by using `defn` for a named function and by `fn` for an anonymous function.

```clojure
(defn add [a b] (+ 2 2))
;; or
(def add (fn [a b] (+ 2 2)))
```

When a form's symbol is a user-defined function, the evaluator will execute it with the arguments from the form.

```clojure
(print (add 2 2)) ;; -> 4
```

**Note:** `fn` is not a special form. It is a **constructor** that returns the underlying function type. So, to create an IIFE (Immediately Invoked Function Expression), one would need to use the `call` form to evaluate the `fn` constructor and then to call the underlying function.

```clojure
(call (fn [] (print "hello!")))

;; or with args
(call (fn [msg] (print msg)) "hello!")
```

## Currying

Currying can be done in a few ways.

```clojure
(print (() + 1 2)) ;; -> 3
(print ((+) 1 2))  ;; -> 3
(print ((+ 1) 2))  ;; -> 3
(print ((+ 1 2)))  ;; -> 3
```

However, as mentioned in the section on functions, applying this structure to immediately-created functions is not possible.

```clojure
((fn [msg] (print msg)) "hello!") ;; -> Fn([msg] (print msg) "hello!")
```

This is because `fn` is a normal form and the arguments from the outher form are merged into the inner. This means that the above is the same as:

```clojure
(fn [msg] (print msg) "hello!") ;; -> Fn([msg] (print msg) "hello!")
```

This is why `call` is required to call functions defined within the arguments of other forms. `call` will first evaluate its first argument, then, if the result is a function, runs the function with the rest of the arguments. See the functions section for how to `call`.

## Scopes

Functions create a new scope. The scope of a function is detemined when it is constructed. For example:

```clojure
(def var 0)
(defn inner [func] (def var 1) (func))

;; The `fn` form is constructed during call arg evaluation, then provided to `inner` as a function.
;; This creates a new scope that references the outer (global) scope.
(inner (fn [] (print "outer" var))) ;; -> 0
```

Closures work as you'd expect from other languages such as JavaScript or Rust.

```clojure
(defn create-counter [init]
  (def count init)
  (fn [incr-by]
    (set count (+ count incr-by))
    count))

(def counter (create-counter 0))

(print (counter 1)) ;; -> 1
(print (counter 1)) ;; -> 2
(print (counter 10)) ;; -> 12
```

### Garbage Collection

As long as a function exists that references a scope, that scope will be preserved. In the counter example, `create-counter` creates a scope which `counter` then uses to increment the count. This scope will live as long as `counter` exists.

However, garbage collection runs once the scope graph grows beyond the set threshold (e.g. 100 scopes), where orphaned (unused) scopes are removed from the graph. If the garbage collection cannot remove enough scopes below the threshold, there is no truncation, so the scopes still in use are preserved.

It is up to the implementor to orcehstrate a check and to trigger garbage collection if the check returns true. In the Quip CLI, garbage collection happens when evaluating each top-level form of a file, or of each form in the REPL.
