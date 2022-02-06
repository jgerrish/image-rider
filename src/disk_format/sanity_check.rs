/// Define a trait that performs sanity checks on an image structure
/// Maybe create a trait that stores the custom parser for a structure, calls it
/// and then calls this
pub trait SanityCheck {
    /// Perform a set of data-dependent sanity checks on a structure
    /// Returns true if the object passes the checks
    /// Return false if the object fails the tests
    fn check(&self) -> bool;
}
