//! Desktop GUI support for LocalSend Improved.

pub mod daemon;
pub mod settings;

pub use daemon::{
    gui_embedded_daemon_status, gui_start_embedded_daemon, gui_stop_embedded_daemon,
    EmbeddedDaemonManager,
};
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
