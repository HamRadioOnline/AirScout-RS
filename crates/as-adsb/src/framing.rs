//! Stream framing of `AirScout.PlaneFeeds.Plugin.RTL1090/RTL1090.cs`: Mode-S Beast binary
//! (`ReceiveBinaryMsg`) and AVR text lines (`ReceiveAVRMsg`).

/// Incremental Beast reader. Feed it whatever the socket delivers; it returns the complete
/// frames as AVR strings (`*HEX;`).
///
/// Like C#, it scans for `0x1A`, then reads a type byte:
/// - `0x32`: 6 timestamp bytes, 1 signal byte, 7 frame bytes;
/// - `0x33`: 6 timestamp bytes, 1 signal byte, 14 frame bytes;
/// - anything else (including `0x31` Mode A/C): skip the two bytes and keep scanning.
///
/// Escaped `0x1A 0x1A` pairs inside a frame are unescaped before counting frame bytes
/// (logic-fixes #40). The Beast MLAT timestamp is ignored; C# discards it in nearly all cases
/// (logic-findings #41).
#[derive(Debug, Default, Clone)]
pub struct BeastReader {
    buf: Vec<u8>,
}

impl BeastReader {
    /// Append `bytes` and return every frame completed so far. Partial frames stay buffered.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut i = 0;
        'scan: loop {
            // scan for the escape byte
            while i < self.buf.len() && self.buf[i] != 0x1A {
                i += 1;
            }
            if i + 1 >= self.buf.len() {
                break;
            }
            let len = match self.buf[i + 1] {
                0x32 => 7,
                0x33 => 14,
                _ => {
                    i += 2;
                    continue;
                }
            };
            let needed = 6 + 1 + len;
            let mut body = Vec::with_capacity(needed);
            let mut j = i + 2;
            while body.len() < needed {
                if j >= self.buf.len() {
                    break 'scan;
                }
                if self.buf[j] != 0x1A {
                    body.push(self.buf[j]);
                    j += 1;
                    continue;
                }
                if j + 1 >= self.buf.len() {
                    break 'scan;
                }
                if self.buf[j + 1] == 0x1A {
                    body.push(0x1A);
                    j += 2;
                    continue;
                }
                i = j;
                continue 'scan;
            }
            let hex: String = body[7..].iter().map(|b| format!("{b:02X}")).collect();
            out.push(format!("*{hex};"));
            i = j;
        }
        self.buf.drain(..i);
        out
    }
}

/// One AVR text line to the `*HEX;` form the decoder takes: `*…;` stays, `@` plus a
/// 12-character MLAT timestamp becomes `*…`. Returns `None` for lines the C# receive loop
/// would not decode (not `*…;` afterwards, or an `@` line shorter than 13 characters, where
/// C# throws and reconnects).
pub fn avr_line(line: &str) -> Option<String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let s = if line.starts_with('@') {
        format!("*{}", line.get(13..)?)
    } else {
        line.to_string()
    };
    (s.starts_with('*') && s.ends_with(';')).then_some(s)
}
