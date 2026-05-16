//! Desktop GUI support for LocalSend Improved.

/// Stable desktop application name.
pub fn app_name() -> &'static str {
    "LocalSend Improved"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_stable() {
        assert_eq!(app_name(), "LocalSend Improved");
    }
}
