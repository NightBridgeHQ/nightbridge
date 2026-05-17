//! Generated local API bindings for NightBridge.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Common API messages.
pub mod common {
    /// Version 1 common API messages.
    pub mod v1 {
        #![allow(missing_docs)]
        tonic::include_proto!("lsi.common.v1");
    }
}

/// Daemon status API.
pub mod daemon {
    /// Version 1 daemon status API.
    pub mod v1 {
        #![allow(missing_docs)]
        tonic::include_proto!("lsi.daemon.v1");
    }
}

/// Peer listing API.
pub mod peers {
    /// Version 1 peer listing API.
    pub mod v1 {
        #![allow(missing_docs)]
        tonic::include_proto!("lsi.peers.v1");
    }
}

/// Transfer API.
pub mod transfers {
    /// Version 1 transfer API.
    pub mod v1 {
        #![allow(missing_docs)]
        tonic::include_proto!("lsi.transfers.v1");
    }
}

/// Inbox API.
pub mod inbox {
    /// Version 1 inbox API.
    pub mod v1 {
        #![allow(missing_docs)]
        tonic::include_proto!("lsi.inbox.v1");
    }
}

/// Event stream API.
pub mod events {
    /// Version 1 event stream API.
    pub mod v1 {
        #![allow(missing_docs)]
        tonic::include_proto!("lsi.events.v1");
    }
}

#[cfg(test)]
mod tests {
    use crate::daemon::v1::DaemonStatus;

    #[test]
    fn daemon_status_generated_type_has_expected_fields() {
        let status = DaemonStatus {
            alias: "demo".to_string(),
            fingerprint: "abcd-1234".to_string(),
            version: "0.1.0".to_string(),
            inbox_dir: "/tmp/inbox".to_string(),
            localsend_port: 53317,
            native_port: 53400,
        };

        assert_eq!(status.alias, "demo");
        assert_eq!(status.localsend_port, 53317);
        assert_eq!(status.native_port, 53400);
    }
}
