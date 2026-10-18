//! Frames on the socket: a 4-byte little-endian length, then that many bytes of JSON.

use std::io::{self, Read, Write};

/// No frame is longer (a large embedded drawing is a few MiB).
pub const MAX_FRAME: usize = 256 << 20;

pub fn write_frame(out: &mut impl Write, json: &[u8]) -> io::Result<()> {
    if json.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
    }
    out.write_all(&(json.len() as u32).to_le_bytes())?;
    out.write_all(json)?;
    out.flush()
}

/// The next frame. A peer that has gone away is `UnexpectedEof`.
pub fn read_frame(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    input.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut json = vec![0u8; len];
    input.read_exact(&mut json)?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_reject_oversize() {
        let mut buf = Vec::new();
        write_frame(&mut buf, br#"{"type":"Ping"}"#).unwrap();
        assert_eq!(&buf[..4], &15u32.to_le_bytes());
        assert_eq!(read_frame(&mut &buf[..]).unwrap(), br#"{"type":"Ping"}"#);
        let huge = ((MAX_FRAME + 1) as u32).to_le_bytes();
        assert!(read_frame(&mut &huge[..]).is_err());
        assert_eq!(read_frame(&mut &buf[..3]).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
