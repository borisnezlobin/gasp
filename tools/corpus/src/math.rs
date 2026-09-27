//! LaTeX templates covering what an undergraduate maths or physics student writes.
//!
//! Slots start with `#` and a letter, optionally followed by a digit; a slot
//! repeated inside one template gets the same value each time.

use crate::rng::Rng;

const VARIABLES: &[&str] = &["x", "y", "z", "t", "s", "u", "r"];
const INDICES: &[&str] = &["i", "j", "k", "n", "m"];
const GREEK: &[&str] = &[
    "\\alpha",
    "\\beta",
    "\\gamma",
    "\\delta",
    "\\epsilon",
    "\\varepsilon",
    "\\theta",
    "\\lambda",
    "\\mu",
    "\\sigma",
    "\\omega",
    "\\phi",
    "\\psi",
    "\\rho",
    "\\tau",
    "\\xi",
    "\\eta",
    "\\kappa",
];
const CAPITAL_GREEK: &[&str] = &[
    "\\Gamma", "\\Delta", "\\Theta", "\\Lambda", "\\Sigma", "\\Phi", "\\Psi", "\\Omega",
];
const FUNCTIONS: &[&str] = &["f", "g", "h", "F", "G", "\\phi"];
const CONSTANTS: &[&str] = &["a", "b", "c", "k", "m", "q"];
const SETS: &[&str] = &[
    "\\mathbb{R}",
    "\\mathbb{C}",
    "\\mathbb{N}",
    "\\mathbb{Z}",
    "\\mathbb{Q}",
    "\\mathbb{R}^n",
    "\\mathbb{R}^3",
];
const ELEMENTARY: &[&str] = &[
    "\\sin", "\\cos", "\\tan", "\\exp", "\\log", "\\ln", "\\sinh", "\\cosh",
];
const NAMED_OPERATORS: &[&str] = &[
    "\\operatorname{tr}",
    "\\operatorname{rank}",
    "\\operatorname{Var}",
    "\\operatorname{Cov}",
    "\\operatorname{span}",
    "\\operatorname{sgn}",
    "\\operatorname{Re}",
    "\\operatorname{Im}",
];
const MATRICES: &[&str] = &["A", "B", "M", "P", "Q", "U"];

const INLINE: &[&str] = &[
    "#v^#n",
    "#v_#i",
    "#v^{#n}_{#i}",
    "\\frac{#n}{#m}",
    "\\frac{#c}{#v}",
    "\\frac{d#f}{d#v}",
    "\\frac{\\partial #f}{\\partial #v}",
    "\\frac{d^2 #v}{dt^2}",
    "#f(#v) = #v^#n + #c#v",
    "#f'(#v)",
    "#f^{-1}(#v)",
    "\\sum_{#i=1}^{n} #v_#i",
    "\\sum_{#i=0}^{\\infty} \\frac{#v^#i}{#i!}",
    "\\prod_{#i=1}^{#n} (1 + #v_#i)",
    "\\int_0^{#n} #f(#v)\\,d#v",
    "\\int_{-\\infty}^{\\infty} e^{-#v^2}\\,d#v = \\sqrt{\\pi}",
    "\\oint_C \\mathbf{F} \\cdot d\\mathbf{r}",
    "\\lim_{#v \\to 0} \\frac{\\sin #v}{#v} = 1",
    "\\lim_{n \\to \\infty} \\left(1 + \\frac{1}{n}\\right)^n = e",
    "#v \\in #S",
    "#f: #S \\to #S2",
    "#g",
    "#g = #d",
    "#g_#i",
    "\\hat{#v}",
    "\\vec{#v}",
    "\\mathbf{#v}",
    "\\bar{#v}",
    "\\overline{#v}",
    "\\dot{#v}",
    "\\ddot{#v}",
    "\\nabla #f",
    "\\nabla \\cdot \\mathbf{E} = \\frac{\\rho}{\\varepsilon_0}",
    "\\nabla \\times \\mathbf{B} = \\mu_0 \\mathbf{J}",
    "E = mc^2",
    "F = ma",
    "p = mv",
    "\\hbar #g",
    "\\Delta #v",
    "#G",
    "\\det(#M) \\neq 0",
    "#M^{-1}",
    "#M^T #M = I",
    "\\operatorname{tr}(#M)",
    "#O(#v)",
    "\\sqrt{#v^2 + #v2^2}",
    "\\sqrt[#n]{#v}",
    "|#v| < \\epsilon",
    "\\lvert #v - #c \\rvert < \\delta",
    "\\left\\| #v \\right\\|",
    "\\sin^2 #v + \\cos^2 #v = 1",
    "e^{i\\pi} + 1 = 0",
    "e^{#c#v}",
    "\\log_#n #v",
    "\\binom{n}{#i}",
    "#n!",
    "\\mathbb{E}[#v]",
    "\\mathbb{P}(#v > #c)",
    "\\operatorname{Var}(#v) = \\mathbb{E}[#v^2] - \\mathbb{E}[#v]^2",
    "#v \\sim \\mathcal{N}(\\mu, \\sigma^2)",
    "\\mathcal{O}(n \\log n)",
    "\\forall #g > 0\\ \\exists \\delta > 0",
    "#v \\leq #v2",
    "#v \\geq 0",
    "#v \\neq #c",
    "#v \\approx #d",
    "a \\equiv b \\pmod{#n}",
    "\\gcd(#n, #m) = 1",
    "#v \\mapsto #v^#n",
    "#S^#n",
    "\\{#v \\in #S : #v > 0\\}",
    "A \\subseteq B",
    "A \\cap B = \\emptyset",
    "\\infty",
    "\\partial_#v #f",
    "#v_{n+1} = #v_n - \\frac{#f(#v_n)}{#f'(#v_n)}",
    "\\langle #v, #v2 \\rangle",
    "\\langle \\psi | \\phi \\rangle",
    "\\hat{H}\\psi = E\\psi",
    "[\\hat{x}, \\hat{p}] = i\\hbar",
    "\\omega = 2\\pi f",
    "T = \\frac{2\\pi}{\\omega}",
    "v = #d\\,\\text{m/s}",
    "g \\approx 9.81\\,\\text{m/s}^2",
    "\\operatorname{rank}(#M) = #n",
    "#v = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
    "\\sigma_#v",
    "\\lambda_#i",
    "k_B T",
    "\\theta \\in [0, 2\\pi)",
    "\\mathbf{#v} \\cdot \\mathbf{#v2}",
    "\\|\\mathbf{#v}\\|",
    "(#v - #c)^#n",
    "#c_0 + #c_1 #v + #c_2 #v^2",
    "z = re^{i\\theta}",
    "\\arg z",
    "#o #v",
    "#o(#g #v)",
    "#f(#v) = \\text{const.}",
    "\\left( #v + \\frac{1}{#v} \\right)^2",
];

const DISPLAY: &[&str] = &[
    "\\int_{#n}^{\\infty} \\frac{1}{#v^2}\\,d#v = \\frac{1}{#n}",
    "#f(#v) = \\sum_{n=0}^{\\infty} \\frac{#f^{(n)}(a)}{n!}(#v - a)^n",
    "\\begin{pmatrix} #c & #n \\\\ #m & #c2 \\end{pmatrix} \\begin{pmatrix} #v \\\\ #v2 \\end{pmatrix} = \\begin{pmatrix} 0 \\\\ 0 \\end{pmatrix}",
    "#M = \\begin{pmatrix}\n#n & 0 & #m \\\\\n0 & 1 & 0 \\\\\n#m & 0 & #n\n\\end{pmatrix}",
    "\\det(#M - \\lambda I) = \\begin{vmatrix} #n - \\lambda & 1 \\\\ 1 & #m - \\lambda \\end{vmatrix} = 0",
    "|#f(#v)| = \\begin{cases}\n#f(#v) & \\text{if } #f(#v) \\geq 0 \\\\\n-#f(#v) & \\text{otherwise}\n\\end{cases}",
    "\\begin{align}\n#f(#v) &= (#v + #n)^2 \\\\\n&= #v^2 + 2 \\cdot #n #v + #n^2\n\\end{align}",
    "\\begin{aligned}\n\\nabla \\cdot \\mathbf{E} &= \\frac{\\rho}{\\varepsilon_0} \\\\\n\\nabla \\cdot \\mathbf{B} &= 0 \\\\\n\\nabla \\times \\mathbf{E} &= -\\frac{\\partial \\mathbf{B}}{\\partial t} \\\\\n\\nabla \\times \\mathbf{B} &= \\mu_0 \\mathbf{J} + \\mu_0 \\varepsilon_0 \\frac{\\partial \\mathbf{E}}{\\partial t}\n\\end{aligned}",
    "i\\hbar \\frac{\\partial}{\\partial t} \\Psi(\\mathbf{r}, t) = \\left( -\\frac{\\hbar^2}{2m} \\nabla^2 + V(\\mathbf{r}) \\right) \\Psi(\\mathbf{r}, t)",
    "\\frac{d}{dt} \\frac{\\partial L}{\\partial \\dot{q}_#i} - \\frac{\\partial L}{\\partial q_#i} = 0",
    "\\hat{f}(\\xi) = \\int_{-\\infty}^{\\infty} f(#v) e^{-2\\pi i #v \\xi}\\,d#v",
    "\\left( \\sum_{#i=1}^{n} a_#i b_#i \\right)^2 \\leq \\left( \\sum_{#i=1}^{n} a_#i^2 \\right) \\left( \\sum_{#i=1}^{n} b_#i^2 \\right)",
    "\\lim_{h \\to 0} \\frac{#f(#v + h) - #f(#v)}{h} = #f'(#v)",
    "\\forall \\varepsilon > 0\\ \\exists \\delta > 0 : |#v - a| < \\delta \\implies |#f(#v) - #f(a)| < \\varepsilon",
    "\\mathbb{P}(A \\mid B) = \\frac{\\mathbb{P}(B \\mid A)\\,\\mathbb{P}(A)}{\\mathbb{P}(B)}",
    "\\operatorname{Var}(X) = \\mathbb{E}\\left[(X - \\mu)^2\\right] = \\sigma^2",
    "\\oint_{\\partial \\Omega} \\omega = \\int_{\\Omega} d\\omega",
    "\\int_a^b #f'(#v)\\,d#v = #f(b) - #f(a)",
    "\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6}",
    "x_{1,2} = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
    "\\mathbf{#v}(t) = \\mathbf{#v}_0 + \\mathbf{v}_0 t + \\frac{1}{2} \\mathbf{a} t^2",
    "\\begin{align}\n\\dot{#v} &= #g #v - #c #v #v2 \\\\\n\\dot{#v2} &= -#G #v2 + #c2 #v #v2\n\\end{align}",
    "E_n = -\\frac{13.6\\,\\text{eV}}{n^2}",
    "\\frac{\\partial^2 u}{\\partial t^2} = c^2 \\frac{\\partial^2 u}{\\partial #v^2}",
    "#f(#v) = \\frac{1}{\\sigma \\sqrt{2\\pi}} \\exp\\left( -\\frac{(#v - \\mu)^2}{2\\sigma^2} \\right)",
    "\\operatorname{rank}(#M) + \\dim \\ker(#M) = n",
    "\\binom{n}{k} = \\frac{n!}{k!\\,(n-k)!}",
    "\\begin{cases}\n#c#v + #n#v2 = #m \\\\\n#v - #v2 = #n\n\\end{cases}",
    "\\underbrace{1 + 1 + \\cdots + 1}_{n \\text{ times}} = n",
    "Z = \\sum_{#i} e^{-\\beta E_#i}, \\quad \\beta = \\frac{1}{k_B T}",
    "\\langle \\hat{A} \\rangle = \\langle \\psi | \\hat{A} | \\psi \\rangle",
    "#M\\mathbf{v} = \\lambda \\mathbf{v}",
    "e^{#M} = \\sum_{k=0}^{\\infty} \\frac{#M^k}{k!}",
    "\\text{Area} = \\iint_D dA = \\int_0^{2\\pi}\\!\\int_0^R r\\,dr\\,d\\theta = \\pi R^2",
    "\\begin{align}\n\\mathbb{E}[X + Y] &= \\mathbb{E}[X] + \\mathbb{E}[Y] \\\\\n\\operatorname{Var}(aX + b) &= a^2 \\operatorname{Var}(X)\n\\end{align}",
    "#f(#v) = \\left\\{ \\begin{array}{ll} #v^2 & #v < 0 \\\\ \\sqrt{#v} & #v \\geq 0 \\end{array} \\right.",
    "\\begin{bmatrix} \\cos\\theta & -\\sin\\theta \\\\ \\sin\\theta & \\cos\\theta \\end{bmatrix}",
    "#v^#n - 1 = (#v - 1)(#v^{#n-1} + \\cdots + 1) \\tag{#m}",
    "#O(#M) = \\sum_{#i} \\lambda_#i",
    "\\oint_C \\frac{#f(z)}{z - a}\\,dz = 2\\pi i\\,#f(a)",
];

/// An inline equation, without the `$` delimiters.
pub fn inline(rng: &mut Rng) -> String {
    let template = *rng.pick(INLINE);
    fill(rng, template)
}

/// An inline equation that is safe inside a table cell (no `|`).
pub fn inline_for_table(rng: &mut Rng) -> String {
    loop {
        let equation = inline(rng);
        if !equation.contains('|') {
            return equation;
        }
    }
}

/// A display equation, possibly several lines long, without the `$$` delimiters.
pub fn display(rng: &mut Rng) -> String {
    let template = *rng.pick(DISPLAY);
    fill(rng, template)
}

/// Fills every `#x` / `#x2` slot of a template.
pub fn fill(rng: &mut Rng, template: &str) -> String {
    let mut chosen: Vec<(String, &'static str)> = Vec::new();
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        let slot_len = slot_length(&rest[at..]);
        let slot = &rest[at..at + slot_len];
        out.push_str(slot_value(rng, &mut chosen, slot));
        rest = &rest[at + slot_len..];
    }
    out.push_str(rest);
    out
}

fn slot_length(text: &str) -> usize {
    let bytes = text.as_bytes();
    let has_digit = bytes.len() > 2 && bytes[2].is_ascii_digit();
    if has_digit { 3 } else { 2.min(bytes.len()) }
}

fn slot_value(rng: &mut Rng, chosen: &mut Vec<(String, &'static str)>, slot: &str) -> &'static str {
    if let Some((_, value)) = chosen.iter().find(|(name, _)| name == slot) {
        return value;
    }
    let pool = pool_for(slot.as_bytes().get(1).copied().unwrap_or(b'v'));
    let mut value = *rng.pick(pool);
    for _ in 0..4 {
        if !chosen.iter().any(|(_, v)| *v == value) {
            break;
        }
        value = *rng.pick(pool);
    }
    chosen.push((slot.to_string(), value));
    value
}

const SMALL_NUMBERS: &[&str] = &["2", "3", "4", "5", "6", "7", "8", "9"];
const DECIMALS: &[&str] = &["0.5", "1.5", "2.25", "3.14", "9.81", "0.01", "6.02", "1.6"];

fn pool_for(kind: u8) -> &'static [&'static str] {
    match kind {
        b'i' => INDICES,
        b'g' => GREEK,
        b'G' => CAPITAL_GREEK,
        b'f' => FUNCTIONS,
        b'c' => CONSTANTS,
        b'S' => SETS,
        b'o' => ELEMENTARY,
        b'O' => NAMED_OPERATORS,
        b'M' => MATRICES,
        b'n' | b'm' => SMALL_NUMBERS,
        b'd' => DECIMALS,
        _ => VARIABLES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balanced(text: &str) -> bool {
        let mut depth = 0i32;
        let mut escaped = false;
        for c in text.chars() {
            if std::mem::take(&mut escaped) {
                continue;
            }
            escaped = c == '\\';
            depth += match c {
                '{' => 1,
                '}' => -1,
                _ => 0,
            };
            if depth < 0 {
                return false;
            }
        }
        depth == 0
    }

    #[test]
    fn every_template_fills_completely() {
        let mut rng = Rng::new(8);
        for template in INLINE.iter().chain(DISPLAY) {
            let filled = fill(&mut rng, template);
            assert!(!filled.contains('#'), "{filled}");
            assert!(!filled.contains('$'), "{filled}");
            assert!(balanced(&filled), "{filled}");
        }
    }

    #[test]
    fn inline_is_one_line() {
        assert!(INLINE.iter().all(|t| !t.contains('\n')));
    }

    #[test]
    fn repeated_slot_gets_same_value() {
        let mut rng = Rng::new(1);
        for _ in 0..20 {
            let filled = fill(&mut rng, "#v+#v");
            let (a, b) = filled.split_once('+').unwrap();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn table_math_has_no_pipes() {
        let mut rng = Rng::new(2);
        for _ in 0..300 {
            assert!(!inline_for_table(&mut rng).contains('|'));
        }
    }

    #[test]
    fn covers_the_usual_constructs() {
        let all = INLINE
            .iter()
            .chain(DISPLAY)
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        for needle in [
            "\\frac",
            "\\sum",
            "\\int",
            "\\mathbb",
            "pmatrix",
            "cases",
            "align",
            "\\left(",
            "\\operatorname",
            "\\text{",
            "_{",
            "^{",
        ] {
            assert!(all.contains(needle), "missing {needle}");
        }
    }
}
