//! Lowercase hex, for hashes, host identifiers and credential payloads.

pub fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_every_byte_as_two_lowercase_digits() {
        assert_eq!(encode(&[]), "");
        assert_eq!(encode(&[0x00, 0x0f, 0xf0, 0xff]), "000ff0ff");
        assert_eq!(encode(b"{\"a\":1}"), "7b2261223a317d");
        assert_eq!(encode(&[0xab; 4]).len(), 8);
    }
}
