//! RPC record marking for TCP transport
//!
//! RPC over TCP uses a record marking scheme where each record is preceded
//! by a 4-byte header indicating the fragment length and whether it's the
//! last fragment.

use crate::fs::nfs::legit_nfs::error::nfsstat3;

/// Record marker
pub struct RecordMarker;

impl RecordMarker {
    /// Encode a record marker
    /// 
    /// The format is:
    /// - Bits 0-30: fragment length (31 bits)
    /// - Bit 31: last fragment flag (1 = last, 0 = more)
    pub fn encode(length: u32, last: bool) -> [u8; 4] {
        let mut value = length & 0x7FFFFFFF;
        if last {
            value |= 0x80000000;
        }
        value.to_be_bytes()
    }

    /// Decode a record marker
    pub fn decode(bytes: [u8; 4]) -> (u32, bool) {
        let value = u32::from_be_bytes(bytes);
        let length = value & 0x7FFFFFFF;
        let last = (value & 0x80000000) != 0;
        (length, last)
    }
}

/// Read a complete RPC message from a stream
pub async fn read_rpc_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, nfsstat3> {
    let mut message = Vec::new();
    loop {
        // Read 4-byte record marker
        let mut marker = [0u8; 4];
        match tokio::io::AsyncReadExt::read_exact(reader, &mut marker).await {
            Ok(_) => {}
            Err(_) => return Err(nfsstat3::ERR_IO),
        }

        let (length, last) = RecordMarker::decode(marker);
        
        // Read fragment
        let mut fragment = vec![0u8; length as usize];
        match tokio::io::AsyncReadExt::read_exact(reader, &mut fragment).await {
            Ok(_) => {}
            Err(_) => return Err(nfsstat3::ERR_IO),
        }

        message.extend_from_slice(&fragment);

        if last {
            break;
        }
    }
    Ok(message)
}

/// Write an RPC message with record marking
pub async fn write_rpc_message<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    data: &[u8],
) -> Result<(), nfsstat3> {
    // For simplicity, write as a single fragment
    let marker = RecordMarker::encode(data.len() as u32, true);
    
    match tokio::io::AsyncWriteExt::write_all(writer, &marker).await {
        Ok(()) => {}
        Err(_) => return Err(nfsstat3::ERR_IO),
    }
    
    match tokio::io::AsyncWriteExt::write_all(writer, data).await {
        Ok(()) => {}
        Err(_) => return Err(nfsstat3::ERR_IO),
    }
    
    match tokio::io::AsyncWriteExt::flush(writer).await {
        Ok(()) => {}
        Err(_) => return Err(nfsstat3::ERR_IO),
    }
    
    Ok(())
}
