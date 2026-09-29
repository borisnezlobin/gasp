// Page template for PDF export. It ports the "professional" print style of
// the PDF Export Plus plugin: every value below corresponds to a rule in the
// plugin's page CSS, with CSS line boxes emulated by fixing the text edges to
// a 1em box and putting the rest of the line height into the leading.

#import "/mitex/compat.typ": mitex-scope

#let ink = rgb("#111111")
#let rule-grey = rgb("#777777")
#let settings = state("editor-settings", (print-background: false, line-height: 2))

// LaTeX math converted by mitex, evaluated with its scope.
#let m(source, scope: mitex-scope) = eval(source, scope: scope)

// An equation that could not be converted is shown as its LaTeX source.
#let math-error(source) = text(fill: rgb("#b00020"), raw(source))

#let with-background(fill) = context {
  if settings.get().print-background { fill } else { none }
}

// A highlight is part of what the note says, not page decoration, so it
// prints even when backgrounds are off. The yellow is pale enough to read
// as a light grey on a monochrome printer.
#let mark(body) = highlight(fill: rgb("#fff1a8"), extent: 0.05em, body)

#let kbd(body) = box(
  stroke: 0.5pt + rule-grey,
  inset: (x: 0.25em),
  outset: (y: 0.2em),
  radius: 0pt,
  text(font: "DejaVu Sans Mono", size: 0.85em, body),
)

#let hrule() = block(above: 2em, below: 2em, line(length: 100%, stroke: 0.5pt + rule-grey))

#let page-break() = pagebreak(weak: true)

#let quote-block(body) = block(
  width: 100%,
  inset: (left: 1em),
  stroke: (left: 2pt + rule-grey),
  body,
)

#let callout(title: none, body) = context {
  let background = if settings.get().print-background { rgb("#eeeeee") } else { none }
  block(width: 100%, stroke: 0.75pt + rgb("#888888"), breakable: true, {
    if title != none {
      block(
        width: 100%,
        fill: background,
        inset: (x: 1.2em, y: 0.55em),
        above: 0pt,
        below: 0pt,
        strong(title),
      )
    }
    if body != [] {
      block(width: 100%, inset: (x: 1.2em, y: 0.75em), above: 0pt, below: 0pt, body)
    }
  })
}

#let code-block(lang: none, title: none, source) = context {
  let background = if settings.get().print-background { rgb("#f7f7f7") } else { none }
  let frame = 0.75pt + rgb("#bbbbbb")
  block(width: 100%, breakable: true, {
    if title != none {
      block(
        width: 100%,
        stroke: (top: frame, left: frame, right: frame),
        inset: (x: 0.75em, y: 0.4em),
        above: 0pt,
        below: 0pt,
        text(size: 0.9em, emph(title)),
      )
    }
    block(
      width: 100%,
      stroke: frame,
      fill: background,
      inset: 0.75em,
      above: 0pt,
      below: 0pt,
      raw(source, lang: lang, block: true),
    )
  })
}

#let bullet-markers = ([•], [◦], [▪])

#let task-box(done) = box(
  width: 0.8em,
  height: 0.8em,
  baseline: 0.1em,
  stroke: 0.6pt + ink,
  align(center + horizon, if done { text(size: 0.8em, sym.checkmark) }),
)

// A list whose items are `(marker, body)` pairs. A marker is `"bullet"`,
// `"open"` or `"done"` (task items) or an item number.
#let md-list(depth: 0, ..items) = context {
  let leading = par.leading.to-absolute()
  block(above: leading, below: leading + 1em, grid(
    columns: (2em, 1fr),
    row-gutter: leading,
    ..items.pos().map(((marker, body)) => {
      let shown = if marker == "bullet" {
        bullet-markers.at(calc.rem(depth, bullet-markers.len()))
      } else if marker == "open" {
        task-box(false)
      } else if marker == "done" {
        task-box(true)
      } else {
        [#marker.]
      }
      (align(right, shown + h(0.5em)), body)
    }).flatten()
  ))
}

// Tables up to this many body rows are kept on one page: a short table
// split after its first row reads as two tables.
#let table-keep-rows = 10

#let md-table(aligns: (), header: (), ..cells) = context {
  let background = if settings.get().print-background { rgb("#eeeeee") } else { none }
  let rows = calc.div-euclid(cells.pos().len(), calc.max(aligns.len(), 1))
  block(breakable: rows > table-keep-rows, table(
    columns: aligns.len(),
    align: aligns,
    stroke: 0.5pt + rule-grey,
    inset: (x: 0.5em, y: 0.35em),
    fill: (_, row) => if row == 0 and header.len() > 0 { background },
    table.header(..header.map(cell => strong(cell))),
    ..cells.pos(),
  ))
}

// An image scaled like the plugin's print CSS: at most the text width and
// 120 mm tall, centred on its own line.
#let note-image(path, width: auto) = layout(size => {
  let natural = measure(image(path))
  let target = if width == auto { natural.width } else { width }
  target = calc.min(target, size.width)
  if natural.width > 0pt {
    let height = natural.height * (target / natural.width)
    if height > 120mm { target = target * (120mm / height) }
  }
  block(width: 100%, breakable: false, align(center, image(path, width: target)))
})

#let missing-image(name) = block(
  width: 100%,
  inset: 1em,
  stroke: (paint: rule-grey, thickness: 0.5pt, dash: "dashed"),
  align(center, text(fill: rule-grey, size: 0.85em)[Missing image: #name]),
)

// An embed of something other than an image, such as another note.
#let note-embed(name) = block(
  width: 100%,
  inset: 1em,
  stroke: 0.5pt + rule-grey,
  emph(name),
)

// A drop cap spanning `lines` lines. `chunks` is the first paragraph split
// at word boundaries; the words that fit beside the letter are laid out next
// to it and the rest continues as a normal paragraph below.
#let drop-cap(lines: 3, gap: 0.15em, letter, chunks) = layout(size => {
  let body-size = text.size
  let leading = par.leading.to-absolute()
  let cap = measure(text(top-edge: "cap-height", bottom-edge: "baseline", [X])).height
  let target = (lines - 1) * (leading + 1em.to-absolute()) + cap
  let letter-size = body-size * (target / cap)
  let shown = text(size: letter-size, top-edge: "cap-height", bottom-edge: "baseline", letter)
  let drop = box(inset: (right: gap), move(dy: 0.8em.to-absolute() - cap, shown))
  let side = size.width - measure(drop).width
  let limit = lines * 1em.to-absolute() + (lines - 1) * leading + 0.01pt
  let fits(count) = {
    let part = chunks.slice(0, count).join()
    measure(block(width: side, par(first-line-indent: 0pt, linebreaks: "simple", part))).height <= limit
  }
  let low = 0
  let high = chunks.len()
  while low < high {
    let mid = calc.div-euclid(low + high + 1, 2)
    if fits(mid) { low = mid } else { high = mid - 1 }
  }
  let beside = if low > 0 { chunks.slice(0, low).join() } else { [] }
  grid(
    columns: (auto, 1fr),
    drop,
    par(first-line-indent: 0pt, linebreaks: "simple", beside),
  )
  if low < chunks.len() {
    block(above: leading, par(first-line-indent: 0pt, chunks.slice(low).join()))
  }
})

#let note(
  paper: "a4",
  margin: (top: 18mm, bottom: 16mm, left: 12mm, right: 12mm),
  font: ("Iowan Old Style", "Libertinus Serif"),
  mono-font: ("Courier New", "DejaVu Sans Mono"),
  size: 12pt,
  line-height: 2,
  footnote-size: 8.5pt,
  paragraph-indent: 0.5in,
  page-numbers: true,
  print-background: false,
  body,
) = {
  let half-leading = size * (line-height - 1) / 2
  settings.update((print-background: print-background, line-height: line-height))

  set document(title: none)
  set page(
    paper: paper,
    margin: margin,
    footer: if page-numbers {
      context align(center, text(size: 7.5pt, fill: rgb("#555555"), top-edge: "cap-height")[
        #counter(page).display() / #counter(page).final().first()
      ])
    },
  )
  set text(
    font: font,
    size: size,
    fill: ink,
    top-edge: 0.8em,
    bottom-edge: -0.2em,
    hyphenate: false,
  )
  set block(spacing: 1em + half-leading)
  set par(
    leading: (line-height - 1) * 1em,
    spacing: (line-height - 1) * 1em,
    first-line-indent: paragraph-indent,
    justify: false,
  )
  show raw: set text(font: mono-font, size: 0.9em)
  show raw.where(block: true): set par(leading: 0.4em)
  show raw.where(block: true): set text(top-edge: "ascender", bottom-edge: "descender")
  show link: underline
  show math.equation.where(block: true): set block(spacing: 1em + half-leading)

  show heading: it => {
    // Absolute sizes: Typst's own heading sizes must not compound with these.
    let scale = (2, 1.5, 1.25, 1.1, 1, 0.9).at(calc.min(it.level, 6) - 1)
    set text(size: size * scale, weight: 700, style: "normal")
    set par(leading: 0.2em, first-line-indent: 0pt)
    block(above: 1.25em + half-leading, below: 0.5em + half-leading, sticky: true, it.body)
  }

  set footnote.entry(
    separator: block(below: 10pt, line(length: 100%, stroke: 0.5pt + rgb("#999999"))),
    clearance: 12pt,
    gap: footnote-size * 0.4,
    indent: 0pt,
  )
  show footnote.entry: it => {
    set text(size: footnote-size)
    set par(leading: 0.4em, first-line-indent: 0pt, spacing: 0.4em)
    let number = counter(footnote).display(at: it.note.location(), "1")
    [#text(weight: 600)[#number.] #it.note.body]
  }
  show footnote: it => {
    set super(size: 0.65em)
    text(weight: 600, it)
  }

  body
}

// The note title the plugin adds above the body.
#let note-title(body) = {
  set text(size: 2em, weight: 700)
  set par(leading: 0.2em, first-line-indent: 0pt)
  block(above: 0pt, below: 0.8em, sticky: true, body)
}
