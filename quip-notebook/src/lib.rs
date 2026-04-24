use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
  Init,
  Eval { id: usize, source: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
  Eval {
    id: usize,
    result: Result<String, String>,
  },
}

/// Big-endian u64 length + UTF-8 JSON. Same layout for requests and responses.
pub fn read_framed_json<T: DeserializeOwned>(
  r: &mut impl Read,
) -> io::Result<T> {
  let b = read_framed_bytes(r)?;
  serde_json::from_slice(&b)
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write_framed_json<T: Serialize>(
  w: &mut impl Write,
  v: &T,
) -> io::Result<()> {
  let json = serde_json::to_vec(v)
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
  write_framed_bytes(w, &json)
}

fn read_framed_bytes(r: &mut impl Read) -> io::Result<Vec<u8>> {
  let mut len_buf = [0u8; 8];
  r.read_exact(&mut len_buf)?;
  let n = u64::from_be_bytes(len_buf);
  if n > u32::MAX as u64 {
    return Err(io::Error::new(
      io::ErrorKind::InvalidData,
      "frame length exceeds cap",
    ));
  }
  let n = n as usize;
  let mut buf = vec![0u8; n];
  r.read_exact(&mut buf)?;
  Ok(buf)
}

fn write_framed_bytes(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
  let n = u64::try_from(payload.len()).map_err(|_| {
    io::Error::new(io::ErrorKind::InvalidInput, "payload does not fit in u64")
  })?;
  w.write_all(&n.to_be_bytes())?;
  w.write_all(payload)?;
  w.flush()?;
  Ok(())
}
