# Quip - Spec

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
(def 'a 1) ;; "'a" is marked as a raw symbol
(def 'b 2) ;; "'b" is also marked as a raw symbol

(print a)  ;; -> 1
(print 'a) ;; -> a

(print [a b])   ;; -> [1 2]
(print ['a b])  ;; -> [a 2]
(print ['a 'b]) ;; -> [a b]
(print '[a b])  ;; -> [a b]
```

### Macro Calls

`#` on a call makes all args unevaluated.

```clojure
(def 'a 2)
(typeof a)  ;; -> number
(#typeof a) ;; -> symbol
(#typeof 'a) ;; -> 'symbol

;; where
(#def a 2)
;; is the same as
(def 'a 2)

;; so then
(#def a (+ 2 2))
(print a) ;; -> (+ 2 2)

;; and
(def 'a (+ 2 2))
(print a) ;; -> 4
```

## Functions

Functions can be defined.

```clojure
;; macro call
(#defn add [a b] (print a) (print b) (+ 2 2))
;; or
(defn 'add '[a b] '(print a) '(print b) '(+ 2 2))

;; which both evaluate to
(def 'add (fn '[a b] '(print a) '(print b) '(+ 2 2)))

(print add)       ;; -> (Function [a b] (+ a b))
(print (add 2 2)) ;; -> 4
```

<!--### Scopes

```clojure
(fn! [..] ..) ;; Scopeless functions
```-->
