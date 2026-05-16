//! Desktop GUI support for LocalSend Improved.

pub mod settings;

pub use settings::{gui_load_settings, gui_save_settings, GuiMode, GuiSettings};

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
