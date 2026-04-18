# Quip - Spec

## Evaluation (and raw with `'`)

By default, symbols will automatically evaluate to their underlying values at runtine:

```clojure
(let 'a 1) ;; "'a" is marked as a raw symbol
(let 'b 2) ;; "'b" is also marked as a raw symbol

(print a)  ;; -> 1
(print 'a) ;; -> a

(print (add a b)) ;; -> 3

(print [a b])   ;; -> [1 2]
(print ['a b]) ;; -> [a 2]
(print ['a 'b]) ;; -> [a b]
(print '[a b])  ;; -> [a b]
```

## Functions
