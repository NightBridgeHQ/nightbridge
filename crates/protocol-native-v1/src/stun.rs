//! Minimal STUN binding client for WAN candidate discovery.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use rand::RngCore;
use tokio::net::UdpSocket;

use crate::{NativeError, Result};

const BINDING_REQUEST: u16 = 0x0001;
const BINDING_SUCCESS_RESPONSE: u16 = 0x0101;
const XOR_MAPPED_ADDRESS: u16 = 0x0020;
const MAGIC_COOKIE: u32 = 0x2112_a442;
const HEADER_LEN: usize = 20;
const TRANSACTION_ID_LEN: usize = 12;

/// Builds a STUN binding request for a caller-provided transaction ID.
pub fn build_binding_request(transaction_id: [u8; TRANSACTION_ID_LEN]) -> Vec<u8> {
    let mut request = Vec::with_capacity(HEADER_LEN);
    request.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    request.extend_from_slice(&0_u16.to_be_bytes());
    request.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    request.extend_from_slice(&transaction_id);
    request
}

/// Parses a STUN binding success response and returns the XOR-MAPPED-ADDRESS endpoint.
pub fn parse_binding_response(
    response: &[u8],
    transaction_id: [u8; TRANSACTION_ID_LEN],
) -> Result<SocketAddr> {
    if response.len() < HEADER_LEN {
        return stun_error("STUN response is shorter than header");
    }
    let message_type = u16::from_be_bytes([response[0], response[1]]);
    if message_type != BINDING_SUCCESS_RESPONSE {
        return stun_error("STUN response is not a binding success");
    }
    let message_len = u16::from_be_bytes([response[2], response[3]]) as usize;
    if response.len() < HEADER_LEN + message_len {
        return stun_error("STUN response body is truncated");
    }
    if u32::from_be_bytes([response[4], response[5], response[6], response[7]]) != MAGIC_COOKIE {
        return stun_error("STUN magic cookie mismatch");
    }
    if response[8..20] != transaction_id {
        return stun_error("STUN transaction id mismatch");
    }

    let mut offset = HEADER_LEN;
    let end = HEADER_LEN + message_len;
    while offset + 4 <= end {
        let attr_type = u16::from_be_bytes([response[offset], response[offset + 1]]);
        let attr_len = u16::from_be_bytes([response[offset + 2], response[offset + 3]]) as usize;
        offset += 4;
        if offset + attr_len > end {
            return stun_error("STUN attribute is truncated");
        }
        if attr_type == XOR_MAPPED_ADDRESS {
            return parse_xor_mapped_address(&response[offset..offset + attr_len], transaction_id);
        }
        offset += padded_attr_len(attr_len);
    }

    stun_error("STUN response missing XOR-MAPPED-ADDRESS")
}

/// Discovers a server-reflexive endpoint using one STUN server.
pub async fn discover_server_reflexive_candidate(
    stun_server: SocketAddr,
    local_port: u16,
    timeout: Duration,
) -> Result<SocketAddr> {
    if local_port == 0 {
        return stun_error("STUN local port must be greater than zero");
    }

    let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, local_port))).await?;
    let mut transaction_id = [0_u8; TRANSACTION_ID_LEN];
    rand::thread_rng().fill_bytes(&mut transaction_id);
    let request = build_binding_request(transaction_id);

    socket.send_to(&request, stun_server).await?;
    let mut response = vec![0_u8; 1500];
    let (len, _) = tokio::time::timeout(timeout, socket.recv_from(&mut response))
        .await
        .map_err(|_| NativeError::Transport("STUN binding request timed out".to_string()))??;
    response.truncate(len);

    parse_binding_response(&response, transaction_id)
}

fn parse_xor_mapped_address(
    attribute: &[u8],
    _transaction_id: [u8; TRANSACTION_ID_LEN],
) -> Result<SocketAddr> {
    if attribute.len() < 8 {
        return stun_error("STUN XOR-MAPPED-ADDRESS is truncated");
    }
    if attribute[1] != 0x01 {
        return stun_error("STUN XOR-MAPPED-ADDRESS must be IPv4");
    }

    let x_port = u16::from_be_bytes([attribute[2], attribute[3]]);
    let port = x_port ^ ((MAGIC_COOKIE >> 16) as u16);
    let cookie = MAGIC_COOKIE.to_be_bytes();
    let address = Ipv4Addr::new(
        attribute[4] ^ cookie[0],
        attribute[5] ^ cookie[1],
        attribute[6] ^ cookie[2],
        attribute[7] ^ cookie[3],
    );

    Ok(SocketAddr::new(IpAddr::V4(address), port))
}

fn padded_attr_len(len: usize) -> usize {
    (len + 3) & !3
}

fn stun_error<T>(message: &str) -> Result<T> {
    Err(NativeError::Transport(message.to_string()))
}

#[cfg(test)]
fn fixture_binding_success(
    endpoint: SocketAddr,
    transaction_id: [u8; TRANSACTION_ID_LEN],
) -> Vec<u8> {
    let SocketAddr::V4(endpoint) = endpoint else {
        panic!("fixture only supports IPv4");
    };
    let cookie = MAGIC_COOKIE.to_be_bytes();
    let x_port = endpoint.port() ^ ((MAGIC_COOKIE >> 16) as u16);
    let ip = endpoint.ip().octets();
    let value = [
        0,
        0x01,
        x_port.to_be_bytes()[0],
        x_port.to_be_bytes()[1],
        ip[0] ^ cookie[0],
        ip[1] ^ cookie[1],
        ip[2] ^ cookie[2],
        ip[3] ^ cookie[3],
    ];
    let mut response = Vec::new();
    response.extend_from_slice(&BINDING_SUCCESS_RESPONSE.to_be_bytes());
    response.extend_from_slice(&12_u16.to_be_bytes());
    response.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    response.extend_from_slice(&transaction_id);
    response.extend_from_slice(&XOR_MAPPED_ADDRESS.to_be_bytes());
    response.extend_from_slice(&(value.len() as u16).to_be_bytes());
    response.extend_from_slice(&value);
    response
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::*;

    const TEST_TRANSACTION_ID: [u8; 12] =
        [0x21, 0x12, 0xa4, 0x42, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

    #[test]
    fn parses_xor_mapped_ipv4_address() {
        let response = fixture_xor_mapped_response();
        let endpoint = parse_binding_response(&response, TEST_TRANSACTION_ID).unwrap();

        assert_eq!(endpoint.to_string(), "203.0.113.10:53400");
    }

    #[test]
    fn binding_request_has_expected_header() {
        let request = build_binding_request(TEST_TRANSACTION_ID);

        assert_eq!(&request[0..2], &[0x00, 0x01]);
        assert_eq!(&request[2..4], &[0x00, 0x00]);
        assert_eq!(&request[4..8], &[0x21, 0x12, 0xa4, 0x42]);
        assert_eq!(&request[8..20], &TEST_TRANSACTION_ID);
    }

    #[test]
    fn parse_rejects_wrong_transaction_id() {
        let response = fixture_xor_mapped_response();
        let error = parse_binding_response(&response, [9; 12]).unwrap_err();

        assert!(error.to_string().contains("STUN transaction id mismatch"), "{error}");
    }

    fn fixture_xor_mapped_response() -> Vec<u8> {
        let endpoint = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)), 53400);
        fixture_binding_success(endpoint, TEST_TRANSACTION_ID)
    }
}
