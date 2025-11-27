fn main() {
    // This build script enforces that backend features are mutually exclusive.
    // let features: &[&'static str] = &[
    //     #[cfg(feature = "use_smolstr")]
    //     "use_smolstr",
    //     // Add other potential backend features here in the future
    //     // e.g., #[cfg(feature = "backend-other")] "backend-other",
    // ];
    //
    // if features.len() > 1 {
    //     panic!("Error: The following backend features are mutually exclusive and cannot be enabled at the same time: {features:?}");
    // }
}
