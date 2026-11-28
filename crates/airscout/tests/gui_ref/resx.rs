//! Minimal `.resx` string lookup for the Designer parser.

/// Look up a named string in a `.resx` XML document.
///
/// Scans for `<data name="KEY" …>` and returns the decoded `<value>…</value>`
/// text, decoding the XML entities the Designer emits (`&amp; &lt; &gt; &quot;
/// &apos;` and the numeric `&#NN;` / `&#xNN;` character references).
pub fn lookup(resx_xml: &str, key: &str) -> Option<String> {
    let needle = format!("<data name=\"{}\"", key);
    let pos = resx_xml.find(&needle)?;
    // Bound the value search to this `<data>` element only, so a key without a
    // `<value>` never leaks the next key's value.
    let data_end = pos + resx_xml[pos..].find("</data>")?;
    let data = &resx_xml[pos..data_end];
    let value = data.find("<value")?;
    let after_value = data[value + "<value".len()..].trim_start();
    // Self-closing `<value />` / `<value/>` means the empty string.
    if let Some(rest) = after_value.strip_prefix("/>") {
        debug_assert!(rest.trim().is_empty());
        return Some(String::new());
    }
    let inner = after_value.strip_prefix('>')?;
    let value_end = inner.find("</value>")?;
    Some(decode_entities(&inner[..value_end]))
}

/// Decode the standard XML entities plus decimal and hex character references.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after_amp = &rest[amp + 1..];
        match after_amp.find(';') {
            Some(semi) => match decode_entity(&after_amp[..semi]) {
                Some(ch) => {
                    out.push(ch);
                    rest = &after_amp[semi + 1..];
                }
                None => {
                    // Not an entity we know: keep the '&' and move past it.
                    out.push('&');
                    rest = after_amp;
                }
            },
            None => {
                out.push('&');
                rest = after_amp;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Decode one entity body (without the surrounding `&` and `;`) into a `char`.
fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let digits = entity.strip_prefix('#')?;
            let code = if digits.starts_with('x') || digits.starts_with('X') {
                u32::from_str_radix(&digits[1..], 16).ok()?
            } else {
                digits.parse::<u32>().ok()?
            };
            char::from_u32(code)
        }
    }
}
