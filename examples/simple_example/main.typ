#import "../../knot-typst-package/lib.typ": *

#set page(width: 210mm, margin: 25mm)
#set text(font: "New Computer Modern", size: 11pt, lang: "en")
#set par(justify: true)

= Simple Example with Two R Chunks

This document demonstrates two R code chunks.

== First Chunk: Define a variable

#figure(
  kind: raw,
  supplement: "Chunk",
  caption: "This is my first R code chunk.",
)[#code-chunk(
  lang: "r",
  name: "chunk-1",
  caption: "This is my first R code chunk.",
  echo: true,
  eval: true,
  input: [```r
  x <- c(2, 3, 5, 7, 11, 13)```],
  output: none,
)] <chunk-1>

The value of `x` is defined in the first chunk.

== Second Chunk: Use and print the variable

#figure(kind: raw, supplement: "Chunk")[#code-chunk(
  lang: "r",
  name: "second-chunk",
  echo: true,
  eval: true,
  input: [```r
  y <- x * 2
  print(y)```],
  output: [```
  [1]  4  6 10 14 22 26```],
)] <second-chunk>

The second chunk calculates `y` based on `x` and prints its value.

We can reference the first chunk like this: @chunk-1.
