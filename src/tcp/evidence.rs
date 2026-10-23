//! Optional synchronous wire-frame observation and offline decompression.
//! Observers must enforce their own total storage budget and deadline. Callback
//! failure fails the request; successful data is never returned without evidence.
use std::{fmt::Debug, io, net::SocketAddr};

/// One callback before transmission, followed by a complete wire frame or failure.
/// Header length is always 16 and body contains compressed bytes, before decoding.
#[derive(Debug)]
pub enum FrameEvent<'a> {
    Request {
        endpoint: SocketAddr,
        request: &'a [u8],
    },
    Response {
        endpoint: SocketAddr,
        request: &'a [u8],
        header: &'a [u8; 16],
        body: &'a [u8],
    },
    Failed {
        endpoint: SocketAddr,
        request: &'a [u8],
        kind: io::ErrorKind,
    },
}

/// Shared observers survive endpoint changes, handshake retries and reconnects.
/// Implementations must synchronize concurrent connections and avoid unbounded queues.
pub trait FrameObserver: Debug + Send + Sync {
    fn observe(&self, event: FrameEvent<'_>) -> io::Result<()>;
}

/// Decode the same bounded bytes archived by a FrameObserver, without networking.
/// Call `Tdx::try_parse` with the returned body to replay a particular protocol.
pub fn decode_frame(header: &[u8; 16], body: &[u8], decompress: bool) -> io::Result<Vec<u8>> {
    let deflate = u16::from_le_bytes([header[12], header[13]]) as usize;
    let inflate = u16::from_le_bytes([header[14], header[15]]) as usize;
    if body.len() != deflate {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "wire frame length mismatch",
        ));
    }
    if decompress && deflate != inflate {
        let decoded = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(body, inflate)
            .map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("响应解压失败: {e:?}"))
            })?;
        if decoded.len() != inflate {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "响应解压长度不匹配",
            ));
        }
        Ok(decoded)
    } else {
        Ok(body.to_vec())
    }
}
