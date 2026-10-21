/// Why a note or the vault password couldn't be used.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CryptError {
    #[error("That password doesn't unlock it.")]
    WrongPassword,
    #[error("The encrypted note is damaged, so it can't be opened.")]
    Corrupt,
    #[error("This note isn't encrypted.")]
    NotEncrypted,
    #[error("A newer version of Gasp encrypted this note. Update Gasp to open it.")]
    NewerVersion,
}
