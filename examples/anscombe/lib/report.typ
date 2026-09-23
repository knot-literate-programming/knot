#import "@preview/codly:1.3.0": *

#let report(body) = {
  set page(paper: "a4", margin: (x: 22mm, y: 19mm), numbering: "1")
  set text(font: "New Computer Modern", size: 10pt, lang: "en")
  set par(justify: true)
  set heading(numbering: "1.1")
  show raw: set text(size: 8pt)
  show: codly-init
  codly(
    languages: (
      r: (name: "R", icon: "", color: rgb("#CE412B")),
      python: (name: "Python", icon: "", color: rgb("#3572A5")),
      output: (name: "#>", icon: "", color: rgb("#9999")),
    ),
    lang-radius: 100pt,
  )
  body
}

#let results-table(rows) = {
  set text(size: 9pt)
  table(
    columns: (auto, 1fr, 1fr, 1fr, 1fr, 1fr),
    align: (left, right, right, right, right, right),
    stroke: none,
    inset: 5pt,
    table.header([*Series*], [*Mean x*], [*Mean y*], [*Slope*], [*Intercept*], [*$R^2$*]),
    table.hline(),
    ..rows.map(r => (r.series, ..("mean_x", "mean_y", "slope", "intercept", "r_squared")
      .map(key => str(calc.round(r.at(key), digits: 3))))).flatten(),
    table.hline(),
  )
}

#let interpretation = [
  *What the summaries conceal.* Series I has an approximately linear pattern
  with scatter. Series II is curved. Series III is nearly linear except for an
  outlying response. In series IV, one observation determines the slope because
  the other ten share the same x coordinate. Similar fitted lines therefore
  do not establish that the same model is appropriate for all four datasets.
]
