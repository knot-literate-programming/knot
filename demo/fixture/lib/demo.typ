// Page and syntax highlighting of the recorded demo document.
#import "@preview/codly:1.3.0": *

#let demo(body) = {
  show: codly-init
  codly(
    languages: (
      r: (name: "R", icon: "", color: rgb("#CE412B")),
      python: (name: "Python", icon: "", color: rgb("#3572A5")),
    ),
    lang-radius: 100pt,
  )
  set page(width: 15cm, height: auto, margin: 1cm)
  set text(size: 10pt)
  body
}
