//! Taint barrier. Values from the model are `Tainted`. Only the kernel can turn an approved value into `Clean`.

/// A value that came from the model and may be shaped by untrusted content.
pub struct Tainted<T>(T);

impl<T> Tainted<T> {
    pub(crate) fn new(v: T) -> Self {
        Tainted(v)
    }

    pub fn peek(&self) -> &T {
        &self.0
    }
}

/// A value a human approved, bound to a digest that still matches. Only the kernel can make one.
///
/// Apps can read a `Clean` value:
/// ```
/// fn effect(a: wraithkit::Clean<serde_json::Value>) -> String { a.get().to_string() }
/// ```
/// Apps cannot construct one. The passing twin of this block takes the public accessor instead:
/// ```compile_fail
/// let _c = wraithkit::Clean(serde_json::json!({}));
/// ```
/// ```
/// let _f = wraithkit::Clean::<serde_json::Value>::get;
/// ```
/// Public items of the taint module are reachable:
/// ```
/// let _f: fn(&wraithkit::taint::Tainted<i32>) -> &i32 = wraithkit::taint::Tainted::peek;
/// ```
/// The gate itself is not:
/// ```compile_fail
/// let _f = wraithkit::taint::release::<i32>;
/// ```
/// ```
/// let _f = wraithkit::taint::Tainted::<i32>::peek;
/// ```
pub struct Clean<T>(T);

impl<T> Clean<T> {
    pub fn get(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Releases the value only when the digest the human approved still equals the digest recomputed now.
pub(crate) fn release<T>(
    t: Tainted<T>,
    approved_digest: &str,
    digest_now: &str,
) -> Result<Clean<T>, String> {
    if approved_digest != digest_now {
        return Err("stale: inputs changed since approval".into());
    }
    Ok(Clean(t.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_releases_matching() {
        let c = release(Tainted::new(5), "abc", "abc").expect("matching digests release");
        assert_eq!(*c.get(), 5);
        assert_eq!(c.into_inner(), 5);
    }

    #[test]
    fn gate_refuses_stale() {
        let err = release(Tainted::new(5), "abc", "def")
            .err()
            .expect("stale must refuse");
        assert!(err.starts_with("stale:"));
    }
}
