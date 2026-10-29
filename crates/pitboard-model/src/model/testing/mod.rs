//! What the model's tests share: records as the core reports them, the state driven by hand,
//! other apps as a test says they are, and, for the threaded tests, a machine of the test's
//! own that the real core runs on.

pub(crate) mod machine;
pub(crate) mod platform;
pub(crate) mod records;
pub(crate) mod world;
