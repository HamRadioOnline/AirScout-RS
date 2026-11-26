//! .NET Framework 4.7.2 invariant number formatting (`AirScout.csproj:13`).
//!
//! .NET Framework formats doubles by first rounding to 15 significant digits
//! and then rounding that decimal string half away from zero.

/// Sign and decimal digits of `x` after the 15-significant-digit step,
/// rounded half away from zero to `decimals` places.
/// Returns `(negative, integer digits, fraction digits)`; `x` must be finite.
fn round_decimal(x: f64, decimals: usize) -> (bool, String, String) {
    let neg = x.is_sign_negative();
    let sci = format!("{:.14e}", x.abs());
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: Vec<u8> = mant
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    // Value = 0.d1d2d3... * 10^point.
    let point = exp + 1;
    let (mut int, mut frac): (Vec<u8>, Vec<u8>) = if point <= 0 {
        let mut f = vec![0u8; (-point) as usize];
        f.extend(&digits);
        (vec![0], f)
    } else if point as usize >= digits.len() {
        let mut i = digits.clone();
        i.resize(point as usize, 0);
        (i, Vec::new())
    } else {
        (
            digits[..point as usize].to_vec(),
            digits[point as usize..].to_vec(),
        )
    };
    let round_up = frac.get(decimals).is_some_and(|&d| d >= 5);
    frac.truncate(decimals);
    frac.resize(decimals, 0);
    if round_up {
        let mut carry = true;
        for d in frac.iter_mut().rev().chain(int.iter_mut().rev()) {
            if *d == 9 {
                *d = 0;
            } else {
                *d += 1;
                carry = false;
                break;
            }
        }
        if carry {
            int.insert(0, 1);
        }
    }
    let s = |v: &[u8]| v.iter().map(|d| char::from(b'0' + d)).collect::<String>();
    (neg, s(&int), s(&frac))
}

/// `ToString("F<decimals>")` with the invariant culture.
pub fn fixed(x: f64, decimals: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.into();
    }
    let (neg, int, frac) = round_decimal(x, decimals);
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&int);
    if decimals > 0 {
        out.push('.');
        out.push_str(&frac);
    }
    out
}

/// Custom format `"000"`: integer, at least 3 digits, `-` for negative
/// values (also when they round to zero).
pub fn zero3(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.into();
    }
    let (neg, int, _) = round_decimal(x, 0);
    format!("{}{:0>3}", if neg { "-" } else { "" }, int)
}
