// Vendored from github.com/mitex-rs/mitex packages/mitex/specs at commit 985d8e7 (Apache-2.0, see LICENSE).

#import "prelude.typ": *
#import "latex/standard.typ": package as latex-std

// 1. import all the packages and form a mitex-scope for mitex to use
#let packages = (latex-std,)
#let mitex-scope = packages.map(pkg => pkg.scope).sum()

// 2. export all packages with specs by metadata and <mitex-packages> label,
//    mitex-cli can fetch them by
//    `typst query --root . ./packages/mitex/specs/mod.typ "<mitex-packages>"`
#metadata(packages) <mitex-packages>
