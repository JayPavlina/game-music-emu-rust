/// Read a little-endian 16-bit value from a byte slice.
#[inline]
pub fn get_le16(p: &[u8]) -> u16 {
    u16::from_le_bytes([p[0], p[1]])
}

/// Read a big-endian 16-bit value from a byte slice.
#[inline]
pub fn get_be16(p: &[u8]) -> u16 {
    u16::from_be_bytes([p[0], p[1]])
}

/// Read a little-endian 32-bit value from a byte slice.
#[inline]
pub fn get_le32(p: &[u8]) -> u32 {
    u32::from_le_bytes([p[0], p[1], p[2], p[3]])
}

/// Read a big-endian 32-bit value from a byte slice.
#[inline]
pub fn get_be32(p: &[u8]) -> u32 {
    u32::from_be_bytes([p[0], p[1], p[2], p[3]])
}

/// Write a little-endian 16-bit value to a byte slice.
#[inline]
pub fn set_le16(p: &mut [u8], n: u16) {
    p[0] = n as u8;
    p[1] = (n >> 8) as u8;
}

/// Write a big-endian 16-bit value to a byte slice.
#[inline]
pub fn set_be16(p: &mut [u8], n: u16) {
    p[0] = (n >> 8) as u8;
    p[1] = n as u8;
}

/// Write a little-endian 32-bit value to a byte slice.
#[inline]
pub fn set_le32(p: &mut [u8], n: u32) {
    p[0] = n as u8;
    p[1] = (n >> 8) as u8;
    p[2] = (n >> 16) as u8;
    p[3] = (n >> 24) as u8;
}

/// Write a big-endian 32-bit value to a byte slice.
#[inline]
pub fn set_be32(p: &mut [u8], n: u32) {
    p[0] = (n >> 24) as u8;
    p[1] = (n >> 16) as u8;
    p[2] = (n >> 8) as u8;
    p[3] = n as u8;
}
