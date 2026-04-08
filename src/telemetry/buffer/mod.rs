mod container_output;
mod deployments;
pub mod json;
mod logs;
mod status;
mod traces;

pub use container_output::ContainerOutputBuffer;
pub use deployments::DeploymentsBuffer;
pub use logs::LogsBuffer;
pub use status::StatusBuffer;
pub use traces::SpansBuffer;

fn bytes_to_hex(val: &[u8]) -> String {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    let mut buf = Vec::with_capacity(val.len() * 2);

    for &b in val {
        buf.push(HEX_CHARS[(b >> 4) as usize]);
        buf.push(HEX_CHARS[(b & 0x0f) as usize]);
    }

    unsafe { String::from_utf8_unchecked(buf) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn validate_bytes_to_hex(bytes: Vec<u8>) -> bool {
        let actual = bytes_to_hex(&bytes);
        let expected = bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        actual == expected
    }
}
