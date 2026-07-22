use clap::Args;

/// API-version inference and strictness options.
#[derive(Args, Debug, Clone)]
pub struct InferenceArgs {
    /// Enable Feature D apiVersion guessing for kinds whose
    /// apiVersion the IR couldn't pin.
    #[arg(long = "api-version-guess", conflicts_with = "strict_api_versions")]
    pub api_version_guess: bool,

    /// Disable Feature D inference entirely, regardless of
    /// `--api-version-guess`.
    #[arg(long = "strict-api-versions")]
    pub strict_api_versions: bool,
}

impl InferenceArgs {
    /// Reports whether API-version inference is enabled after strict-mode policy.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.api_version_guess && !self.strict_api_versions
    }
}
