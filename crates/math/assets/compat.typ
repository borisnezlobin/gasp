// Compatibility scope for the mitex converter.
//
// The mitex Rust converter (0.2.4, and mitex HEAD as of 985d8e7) always emits
// the Typst names from the command spec bundled with the crate, which was
// generated in 2024. Typst 0.14 and 0.15 renamed or removed some of those
// symbols (`sect`, `diff`, `plus.circle`, `angle.l`, ...), and the vendored
// mitex Typst package now expects different arguments for a few commands.
// This file maps the old names onto the current ones. The test
// `every_spec_symbol_resolves` checks it stays complete.

#import "mod.typ": mitex-scope as upstream-scope

// Symbols whose old modifiers no longer exist, rebuilt with every variant the
// bundled spec can emit.
#let angle = symbol(
  str(sym.angle),
  ("arc", str(sym.angle.arc)),
  ("l", str(sym.chevron.l)),
  ("r", str(sym.chevron.r)),
  ("spheric", str(sym.angle.spheric)),
)
#let ast = symbol(
  str(sym.ast),
  ("circle", str(sym.convolve.o)),
)
#let bracket = symbol(
  str(sym.bracket),
  ("l", str(sym.bracket.l)),
  ("l.double", str(sym.bracket.stroked.l)),
  ("r", str(sym.bracket.r)),
  ("r.double", str(sym.bracket.stroked.r)),
)
#let circle = symbol(
  str(sym.circle),
  ("nested", str(sym.compose.o)),
  ("stroked.big", str(sym.circle.stroked.big)),
)
#let dash = symbol(
  str(sym.dash),
  ("circle", str(sym.dash.o)),
  ("colon", str(sym.dash.colon)),
)
#let diff = symbol(
  str(sym.partial),
)
#let dot = symbol(
  str(sym.dot),
  ("c", str(sym.dot.c)),
  ("circle", str(sym.dot.o)),
  ("circle.big", str(sym.dot.o.big)),
  ("double", str(sym.dot.double)),
  ("op", str(sym.dot.op)),
  ("quad", str(sym.dot.quad)),
  ("square", str(sym.dot.square)),
  ("triple", str(sym.dot.triple)),
)
#let gt = symbol(
  str(sym.gt),
  ("dot", str(sym.gt.dot)),
  ("double", str(sym.gt.double)),
  ("eq.lt", str(sym.gt.eq.lt)),
  ("eq.not", str(sym.gt.eq.not)),
  ("eq.slant", str(sym.gt.eq.slant)),
  ("equiv", str(sym.gt.equiv)),
  ("lt", str(sym.gt.lt)),
  ("nequiv", str(sym.gt.nequiv)),
  ("not", str(sym.gt.not)),
  ("ntilde", str(sym.gt.ntilde)),
  ("tilde", str(sym.gt.tilde)),
  ("tri", str(sym.gt.closed)),
  ("tri.eq", str(sym.gt.closed.eq)),
  ("tri.eq.not", str(sym.gt.closed.eq.not)),
  ("tri.not", str(sym.gt.closed.not)),
  ("triple", str(sym.gt.triple)),
)
#let lt = symbol(
  str(sym.lt),
  ("dot", str(sym.lt.dot)),
  ("double", str(sym.lt.double)),
  ("eq.gt", str(sym.lt.eq.gt)),
  ("eq.not", str(sym.lt.eq.not)),
  ("eq.slant", str(sym.lt.eq.slant)),
  ("equiv", str(sym.lt.equiv)),
  ("gt", str(sym.lt.gt)),
  ("nequiv", str(sym.lt.nequiv)),
  ("not", str(sym.lt.not)),
  ("ntilde", str(sym.lt.ntilde)),
  ("tilde", str(sym.lt.tilde)),
  ("tri", str(sym.lt.closed)),
  ("tri.eq", str(sym.lt.closed.eq)),
  ("tri.eq.not", str(sym.lt.closed.eq.not)),
  ("tri.not", str(sym.lt.closed.not)),
  ("triple", str(sym.lt.triple)),
)
#let minus = symbol(
  str(sym.minus),
  ("circle", str(sym.minus.o)),
  ("plus", str(sym.minus.plus)),
  ("square", str(sym.minus.square)),
  ("tilde", str(sym.minus.tilde)),
)
#let ohm = symbol(
  str(sym.Omega),
  ("inv", str(sym.Omega.inv)),
)
#let planck = symbol(
  str(sym.planck),
  ("reduce", str(sym.planck)),
)
#let plus = symbol(
  str(sym.plus),
  ("circle", str(sym.plus.o)),
  ("circle.big", str(sym.plus.o.big)),
  ("dot", str(sym.plus.dot)),
  ("minus", str(sym.plus.minus)),
  ("square", str(sym.plus.square)),
)
#let sect = symbol(
  str(sym.inter),
  ("big", str(sym.inter.big)),
  ("double", str(sym.inter.double)),
  ("sq", str(sym.inter.sq)),
)
#let times = symbol(
  str(sym.times),
  ("circle", str(sym.times.o)),
  ("circle.big", str(sym.times.o.big)),
  ("div", str(sym.times.div)),
  ("l", str(sym.times.l)),
  ("r", str(sym.times.r)),
  ("square", str(sym.times.square)),
  ("three.l", str(sym.times.three.l)),
  ("three.r", str(sym.times.three.r)),
)

// Handlers the old converter calls with the old argument lists.
#let (mitexcolor, colortext, mitexcolorbox) = (
  upstream-scope.mitexcolor,
  upstream-scope.colortext,
  upstream-scope.mitexcolorbox,
)
#let compat-color(texcolor, ..body) = mitexcolor(none, texcolor, ..body)
#let compat-textcolor(texcolor, body) = colortext(none, texcolor, body)
#let compat-colorbox(texcolor, body) = mitexcolorbox(none, texcolor, body)

#let mitex-scope = upstream-scope + (
  angle: angle,
  ast: ast,
  bracket: bracket,
  circle: circle,
  dash: dash,
  diff: diff,
  dot: dot,
  gt: gt,
  lt: lt,
  minus: minus,
  ohm: ohm,
  planck: planck,
  plus: plus,
  sect: sect,
  times: times,
  mitexcolor: compat-color,
  colortext: compat-textcolor,
  colorbox: compat-colorbox,
  "set": upstream-scope.mitexset,
  textwidth: none,
  // The vendored package passes "||", which current Typst rejects.
  Vmatrix: math.mat.with(delim: "‖"),
  argmax: math.op("arg max", limits: true),
  argmin: math.op("arg min", limits: true),
)
