//! Callouts folded or unfolded by clicking their header. The source's `-`
//! and `+` set the initial state; a click overrides it for this view
//! without editing the note. The folds live in the core, which the phone
//! shares.

pub use gasp_core::render::folds::Folds;
