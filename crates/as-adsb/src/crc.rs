//! Mode-S parity (`ModeSReply.calcParity`).

/// CRC generator polynomial (Annex 10), 24 bit.
pub const POLYNOMIAL: u32 = 0xFFF409;

/// Bit-exact port of `calcParity`: the 24-bit remainder over `msg` (DF/CA byte plus payload,
/// without the parity field). `msg` shorter than 3 bytes yields its bytes left-aligned, where
/// C# throws.
pub fn parity(msg: &[u8]) -> u32 {
    let mut reg: u32 = 0;
    for k in 0..3 {
        reg = (reg << 8) | u32::from(msg.get(k).copied().unwrap_or(0));
    }
    for i in 0..msg.len() * 8 {
        let invert = reg & 0x80_0000 != 0;
        reg = (reg << 1) & 0xFF_FFFF;
        let byteidx = (24 + i) / 8;
        if byteidx < msg.len() {
            reg |= u32::from((msg[byteidx] >> (7 - (i % 8))) & 1);
        }
        if invert {
            reg ^= POLYNOMIAL;
        }
    }
    reg
}
