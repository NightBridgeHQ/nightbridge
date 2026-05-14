//! Length-delimited control message framing.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::dto::{
    AcceptTransfer, CancelTransfer, ChunkAck, DoneTransfer, Hello, HelloAck, RequestTransfer,
    ResumeRequest,
};
use crate::{NativeError, Result};

/// Maximum encoded control frame payload, excluding the 4-byte length prefix.
pub const MAX_CONTROL_FRAME_BYTES: usize = 1024 * 1024;

const TYPE_HELLO: u8 = 1;
const TYPE_HELLO_ACK: u8 = 2;
const TYPE_REQUEST_TRANSFER: u8 = 3;
const TYPE_ACCEPT: u8 = 4;
const TYPE_CHUNK_ACK: u8 = 5;
const TYPE_CANCEL: u8 = 6;
const TYPE_DONE: u8 = 7;
const TYPE_RESUME_REQUEST: u8 = 8;

/// Control stream messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlMessage {
    /// Initial hello.
    Hello(Hello),
    /// Hello acknowledgement.
    HelloAck(HelloAck),
    /// Request to transfer files.
    RequestTransfer(RequestTransfer),
    /// Transfer acceptance.
    Accept(AcceptTransfer),
    /// Chunk acknowledgement.
    ChunkAck(ChunkAck),
    /// Transfer cancellation.
    Cancel(CancelTransfer),
    /// Transfer completion.
    Done(DoneTransfer),
    /// Resume request.
    ResumeRequest(ResumeRequest),
}

/// Writes one control message as a length-delimited frame.
pub async fn write_control<W>(writer: &mut W, message: &ControlMessage) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut frame = Vec::new();
    frame.push(message_type(message));
    frame.extend_from_slice(&message_payload(message)?);

    if frame.len() > MAX_CONTROL_FRAME_BYTES {
        return Err(protocol_error("control frame too large"));
    }

    writer.write_all(&(frame.len() as u32).to_be_bytes()).await?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads one control message from a length-delimited frame.
pub async fn read_control<R>(reader: &mut R) -> Result<ControlMessage>
where
    R: AsyncRead + Unpin,
{
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;

    if length == 0 {
        return Err(protocol_error("empty control frame"));
    }

    if length > MAX_CONTROL_FRAME_BYTES {
        return Err(protocol_error("control frame too large"));
    }

    let mut frame = vec![0_u8; length];
    reader.read_exact(&mut frame).await?;
    decode_frame(&frame)
}

fn decode_frame(frame: &[u8]) -> Result<ControlMessage> {
    let Some((&message_type, payload)) = frame.split_first() else {
        return Err(protocol_error("empty control frame"));
    };

    match message_type {
        TYPE_HELLO => Ok(ControlMessage::Hello(serde_json::from_slice(payload)?)),
        TYPE_HELLO_ACK => Ok(ControlMessage::HelloAck(serde_json::from_slice(payload)?)),
        TYPE_REQUEST_TRANSFER => {
            Ok(ControlMessage::RequestTransfer(serde_json::from_slice(payload)?))
        }
        TYPE_ACCEPT => Ok(ControlMessage::Accept(serde_json::from_slice(payload)?)),
        TYPE_CHUNK_ACK => Ok(ControlMessage::ChunkAck(serde_json::from_slice(payload)?)),
        TYPE_CANCEL => Ok(ControlMessage::Cancel(serde_json::from_slice(payload)?)),
        TYPE_DONE => Ok(ControlMessage::Done(serde_json::from_slice(payload)?)),
        TYPE_RESUME_REQUEST => Ok(ControlMessage::ResumeRequest(serde_json::from_slice(payload)?)),
        other => Err(protocol_error(format!("unknown control message type {other}"))),
    }
}

fn message_type(message: &ControlMessage) -> u8 {
    match message {
        ControlMessage::Hello(_) => TYPE_HELLO,
        ControlMessage::HelloAck(_) => TYPE_HELLO_ACK,
        ControlMessage::RequestTransfer(_) => TYPE_REQUEST_TRANSFER,
        ControlMessage::Accept(_) => TYPE_ACCEPT,
        ControlMessage::ChunkAck(_) => TYPE_CHUNK_ACK,
        ControlMessage::Cancel(_) => TYPE_CANCEL,
        ControlMessage::Done(_) => TYPE_DONE,
        ControlMessage::ResumeRequest(_) => TYPE_RESUME_REQUEST,
    }
}

fn message_payload(message: &ControlMessage) -> Result<Vec<u8>> {
    match message {
        ControlMessage::Hello(message) => Ok(serde_json::to_vec(message)?),
        ControlMessage::HelloAck(message) => Ok(serde_json::to_vec(message)?),
        ControlMessage::RequestTransfer(message) => Ok(serde_json::to_vec(message)?),
        ControlMessage::Accept(message) => Ok(serde_json::to_vec(message)?),
        ControlMessage::ChunkAck(message) => Ok(serde_json::to_vec(message)?),
        ControlMessage::Cancel(message) => Ok(serde_json::to_vec(message)?),
        ControlMessage::Done(message) => Ok(serde_json::to_vec(message)?),
        ControlMessage::ResumeRequest(message) => Ok(serde_json::to_vec(message)?),
    }
}

fn protocol_error(message: impl Into<String>) -> NativeError {
    NativeError::Protocol(message.into())
}

#[cfg(test)]
mod tests {
    use crate::dto::{default_extensions, Hello, PROTOCOL_VERSION};
    use crate::framing::{read_control, write_control, ControlMessage, MAX_CONTROL_FRAME_BYTES};

    fn hello() -> ControlMessage {
        ControlMessage::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            alias: "node-a".to_string(),
            pubkey: [7; 32],
            nonce: b"nonce".to_vec(),
            extensions: default_extensions(),
        })
    }

    #[tokio::test]
    async fn frame_roundtrip_for_hello() {
        let message = hello();
        let mut bytes = Vec::new();

        write_control(&mut bytes, &message).await.unwrap();
        let decoded = read_control(&mut bytes.as_slice()).await.unwrap();

        assert_eq!(decoded, message);
    }

    #[tokio::test]
    async fn frame_reader_rejects_unknown_message_type() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.push(255);

        let error = read_control(&mut bytes.as_slice()).await.unwrap_err();

        assert!(error.to_string().contains("unknown control message type"));
    }

    #[tokio::test]
    async fn frame_reader_rejects_oversized_frame() {
        let length = (MAX_CONTROL_FRAME_BYTES as u32) + 1;
        let bytes = length.to_be_bytes().to_vec();

        let error = read_control(&mut bytes.as_slice()).await.unwrap_err();

        assert!(error.to_string().contains("control frame too large"));
    }

    #[tokio::test]
    async fn frame_reader_handles_multiple_messages() {
        let first = hello();
        let second = hello();
        let mut bytes = Vec::new();

        write_control(&mut bytes, &first).await.unwrap();
        write_control(&mut bytes, &second).await.unwrap();

        let mut reader = bytes.as_slice();
        assert_eq!(read_control(&mut reader).await.unwrap(), first);
        assert_eq!(read_control(&mut reader).await.unwrap(), second);
    }
}
