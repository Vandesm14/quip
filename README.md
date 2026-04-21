# Quip

In development, see below for details.

## Purpose

**Goals:**

1. To build a language with strict and consistent rules that make it easy to implement and understand.
2. To be deployed as a simple language, yet supporting the bootstrapping of language features in userspace,
3. To create a platform within the language to include macros and code-as-data behaviors without special-casing macro-like behavior.

**Non-Goals:**

1. To be a useful language outside of embeddable scripting.

## Evaluation

By default, symbols will automatically evaluate to their underlying values at runtime.

```clojure
(def a 1)
(def b 2)

(print a)        ;; -> 1
(print (lazy a)) ;; -> a

(print (list a b))               ;; -> (1 2)
(print (list (lazy a) b))        ;; -> (a 2)
(print (list (lazy a) (lazy b))) ;; -> (a b)
(print (lazy (list a b)))        ;; -> (list a b)
```

## Functions

Functions can be defined.

```clojure
(defn add (a b) (+ 2 2))
(def add (fn (a b) (+ 2 2)))

(print (add 2 2))  ;; -> 4
```

<!--### Scopes

```clojure
(fn! [..] ..) ;; Scopeless functions
```-->
