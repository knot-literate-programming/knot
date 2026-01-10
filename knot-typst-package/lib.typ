#import "@preview/codly:1.3.0": *
// #show: codly-init

#import "@preview/codly-languages:0.1.10": *
#codly(languages: codly-languages)

// Manual 'r' language definition to work around the codly-languages bug
#codly(
  languages: (
    r: (name: "R", icon: "", color: rgb("#CE412B")),
  ),
)

// Activate figure numbering globally
#set figure(numbering: "1.")

#let code-chunk(
  input: none,
  output: none,
  .. // Catch all other arguments
) = {
  // Just generate the grid layout
  grid(
    columns: (1fr, 1fr),
    gutter: 1em,
    input,
    if output != none {
      block(fill: luma(244), radius: 4pt, inset: 8pt, width: 100%)[#output]
    } else {
      []
    },
  )
}
