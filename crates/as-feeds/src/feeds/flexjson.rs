//! Flexible JSON feeds support named fields, explicit columns and unambiguous inference.

use super::{http_get_bounded, set_str, set_u64, FLEX_JSON_BYTES};
use crate::vars;
use crate::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::{DateTime, Datelike, NaiveDateTime, Utc};

/// `JavaScriptSerializer.MaxJsonLength` default (UTF-16 code units).
const MAX_JSON_LENGTH: usize = 2_097_152;
/// `JavaScriptSerializer.RecursionLimit` default.
const RECURSION_LIMIT: usize = 100;

// ------------------------------------------------------------------ JSON (JavaScriptSerializer)

#[derive(Debug)]
enum J {
    Obj(Vec<(String, J)>),
    Arr(Vec<J>),
    Text(String),
    Number(String),
    Bool,
    Null,
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

fn jerr(msg: &str, at: usize) -> FeedError {
    FeedError::Parse(format!("{msg} at offset {at}"))
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn eat(&mut self, lit: &str) -> Result<(), FeedError> {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            Ok(())
        } else {
            Err(jerr("invalid JSON literal", self.i))
        }
    }

    fn value(&mut self, depth: usize) -> Result<J, FeedError> {
        if depth > RECURSION_LIMIT {
            return Err(jerr("RecursionLimit exceeded", self.i));
        }
        self.ws();
        match self.peek() {
            Some('{') => self.object(depth),
            Some('[') => self.array(depth),
            Some('"') => Ok(J::Text(self.string()?)),
            Some('t') => self.eat("true").map(|()| J::Bool),
            Some('f') => self.eat("false").map(|()| J::Bool),
            Some('n') => self.eat("null").map(|()| J::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            _ => Err(jerr("invalid JSON value", self.i)),
        }
    }

    fn object(&mut self, depth: usize) -> Result<J, FeedError> {
        self.bump();
        let mut m: Vec<(String, J)> = Vec::new();
        self.ws();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(J::Obj(m));
        }
        loop {
            self.ws();
            if self.peek() != Some('"') {
                return Err(jerr("expected a member name", self.i));
            }
            let k = self.string()?;
            self.ws();
            if self.bump() != Some(':') {
                return Err(jerr("expected ':'", self.i));
            }
            let v = self.value(depth + 1)?;
            // `dictionary[name] = value`: a repeated name keeps its place, the last value wins
            match m.iter_mut().find(|(key, _)| *key == k) {
                Some(e) => e.1 = v,
                None => m.push((k, v)),
            }
            self.ws();
            match self.bump() {
                Some(',') => {}
                Some('}') => return Ok(J::Obj(m)),
                _ => return Err(jerr("expected ',' or '}'", self.i)),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<J, FeedError> {
        self.bump();
        let mut a = Vec::new();
        self.ws();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(J::Arr(a));
        }
        loop {
            a.push(self.value(depth + 1)?);
            self.ws();
            match self.bump() {
                Some(',') => {}
                Some(']') => return Ok(J::Arr(a)),
                _ => return Err(jerr("expected ',' or ']'", self.i)),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, FeedError> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .filter(|h| h.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| jerr("bad \\u escape", self.i))?;
        self.i += 4;
        u32::from_str_radix(h, 16).map_err(|_| jerr("bad \\u escape", self.i))
    }

    fn string(&mut self) -> Result<String, FeedError> {
        self.bump();
        let mut out = String::new();
        loop {
            match self
                .bump()
                .ok_or_else(|| jerr("unterminated string", self.i))?
            {
                '"' => return Ok(out),
                '\\' => {
                    let c = match self.bump() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('/') => '/',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('u') => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi)
                                && self.s[self.i..].starts_with("\\u")
                            {
                                let save = self.i;
                                self.i += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    self.i = save;
                                    hi
                                }
                            } else {
                                hi
                            };
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => return Err(jerr("bad escape", self.i)),
                    };
                    out.push(c);
                }
                c => out.push(c),
            }
        }
    }

    /// Preserve the lexical number so time fields can distinguish integers from fractions.
    fn number(&mut self) -> Result<J, FeedError> {
        let start = self.i;
        let digits = |p: &mut Self| {
            let s = p.i;
            while p.peek().is_some_and(|c| c.is_ascii_digit()) {
                p.bump();
            }
            p.i > s
        };
        if self.peek() == Some('-') {
            self.bump();
        }
        if !digits(self) {
            return Err(jerr("invalid number", start));
        }
        if self.peek() == Some('.') {
            self.bump();
            if !digits(self) {
                return Err(jerr("invalid number", start));
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !digits(self) {
                return Err(jerr("invalid number", start));
            }
        }
        Ok(J::Number(self.s[start..self.i].to_owned()))
    }
}

fn parse_json(s: &str) -> Result<J, FeedError> {
    if s.encode_utf16().count() > MAX_JSON_LENGTH {
        return Err(FeedError::Parse(
            "the JSON is longer than MaxJsonLength (2097152)".into(),
        ));
    }
    let mut p = Parser {
        s: s.trim_start_matches('\u{feff}'),
        i: 0,
    };
    p.ws();
    if p.peek().is_none() {
        // Deserialize("") yields null, which AddElement cannot handle
        return Err(FeedError::Parse("empty response".into()));
    }
    let v = p.value(1)?;
    p.ws();
    if p.peek().is_some() {
        return Err(jerr("data after the JSON value", p.i));
    }
    Ok(v)
}

// ------------------------------------------------------------------ flattening

const HEX: usize = 0;
const UTC: usize = 1;
const CALL: usize = 2;
const TYPE: usize = 3;
const LAT: usize = 4;
const LON: usize = 5;
const ALT: usize = 6;
const TRACK: usize = 7;
const SPEED: usize = 8;
const REG: usize = 9;
const REQUIRED: [usize; 6] = [HEX, LAT, LON, ALT, TRACK, SPEED];
const OPTIONAL: [usize; 4] = [UTC, CALL, TYPE, REG];
const FIELD_NAMES: [&[&str]; 10] = [
    &["hex", "icao"],
    &["utc", "time", "timestamp"],
    &["call", "callsign", "flight"],
    &["type", "typecode"],
    &["lat", "latitude"],
    &["lon", "longitude"],
    &["alt", "altitude"],
    &["track", "groundtrack"],
    &["speed", "groundspeed"],
    &["reg", "registration"],
];

#[derive(Clone, Copy)]
enum Cell<'a> {
    Missing,
    Text(&'a str),
    Number(&'a str),
    Other,
}

impl<'a> Cell<'a> {
    fn from_value(value: &'a J) -> Self {
        match value {
            J::Null => Self::Missing,
            J::Text(text) => Self::Text(text),
            J::Number(number) => Self::Number(number),
            _ => Self::Other,
        }
    }

    fn text(self) -> Option<&'a str> {
        match self {
            Self::Text(text) => Some(text.trim()),
            _ => None,
        }
    }

    fn number(self) -> Option<f64> {
        match self {
            Self::Number(text) | Self::Text(text) => text
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite()),
            _ => None,
        }
    }

    fn timestamp(self) -> Option<DateTime<Utc>> {
        let raw = match self {
            Self::Number(number) | Self::Text(number) => number.trim(),
            _ => return None,
        };
        let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
        let time = if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
            let epoch: i64 = raw.parse().ok()?;
            if epoch.unsigned_abs() < 10_000_000_000 {
                DateTime::from_timestamp(epoch, 0)
            } else {
                DateTime::from_timestamp_millis(epoch)
            }
        } else if matches!(self, Self::Text(_)) {
            DateTime::parse_from_rfc3339(raw)
                .ok()
                .map(|at| at.with_timezone(&Utc))
                .or_else(|| {
                    ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"]
                        .iter()
                        .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
                        .map(|at| at.and_utc())
                })
        } else {
            None
        }?;
        (1..=9999).contains(&time.year()).then_some(time)
    }

    fn bounded(self, min: f64, max: f64) -> Option<f64> {
        self.number().filter(|value| (min..=max).contains(value))
    }

    fn hex(self) -> Option<&'a str> {
        self.text()
            .filter(|text| text.len() == 6 && text.bytes().all(|c| c.is_ascii_hexdigit()))
    }
}

#[derive(Clone, Copy)]
struct Row<'a> {
    parent: &'a J,
    key: Option<&'a str>,
    node: &'a J,
    width: usize,
}

impl<'a> Row<'a> {
    fn column(self, index: usize) -> Cell<'a> {
        if index == 0 {
            return self.key.map_or(Cell::Missing, Cell::Text);
        }
        let value = match self.node {
            J::Obj(members) => members.get(index - 1).map(|(_, member)| member),
            J::Arr(members) => members.get(index - 1),
            _ => None,
        };
        value.map_or(Cell::Missing, Cell::from_value)
    }

    fn named(self, field: usize) -> Option<(usize, Cell<'a>)> {
        let J::Obj(members) = self.node else {
            return None;
        };
        FIELD_NAMES[field].iter().find_map(|alias| {
            members
                .iter()
                .enumerate()
                .find_map(|(index, (name, value))| {
                    name.eq_ignore_ascii_case(alias)
                        .then(|| (index + 1, Cell::from_value(value)))
                })
        })
    }
}

fn collect_rows<'a>(node: &'a J, parent: &'a J, key: Option<&'a str>, rows: &mut Vec<Row<'a>>) {
    let scalar = |member: &J| matches!(member, J::Text(_) | J::Number(_));
    let (width, has_cells) = match node {
        J::Obj(members) => (
            members.len(),
            members.iter().any(|(_, value)| scalar(value)),
        ),
        J::Arr(members) => (members.len(), members.iter().any(scalar)),
        _ => return,
    };
    if has_cells {
        rows.push(Row {
            parent,
            key,
            node,
            width: width + 1,
        });
    }
    match node {
        J::Obj(members) => {
            for (name, member) in members {
                collect_rows(member, node, Some(name), rows);
            }
        }
        J::Arr(members) => {
            for member in members {
                collect_rows(member, node, None, rows);
            }
        }
        _ => {}
    }
}

fn plane_rows(root: &J) -> Result<(Vec<Row<'_>>, usize), FeedError> {
    let mut rows = Vec::new();
    collect_rows(root, root, None, &mut rows);
    let mut counts = std::collections::HashMap::<usize, usize>::new();
    for row in &rows {
        *counts.entry(row.width).or_default() += 1;
    }
    let best = rows
        .iter()
        .copied()
        .fold(None::<Row<'_>>, |best, row| {
            if best.is_none_or(|best| counts[&row.width] > counts[&best.width]) {
                Some(row)
            } else {
                best
            }
        })
        .ok_or_else(|| FeedError::Parse("FlexJSON has no usable row structure".into()))?;

    if rows
        .iter()
        .any(|row| row.named(LAT).is_some() && row.named(LON).is_some())
    {
        rows.retain(|row| row.named(LAT).is_some() || row.named(LON).is_some());
    } else {
        rows.retain(|row| row.width == best.width || std::ptr::eq(row.parent, best.parent));
    }
    Ok((rows, best.width))
}

#[derive(Clone, Copy)]
struct Units {
    altitude_metric: bool,
    speed_metric: bool,
}

impl Units {
    fn altitude(self, cell: Cell<'_>) -> Option<f64> {
        if self.altitude_metric {
            cell.bounded(0.0, 30_000.0).map(|meters| meters * 3.28084)
        } else {
            cell.bounded(0.0, 100_000.0)
        }
    }

    fn speed(self, cell: Cell<'_>) -> Option<f64> {
        if self.speed_metric {
            cell.bounded(0.0, 1000.0).map(|kmh| kmh / 1.852)
        } else {
            cell.bounded(0.0, 550.0)
        }
    }
}

#[derive(Default)]
struct NumericStats {
    samples: usize,
    min: f64,
    max: f64,
    mean: f64,
}

impl NumericStats {
    fn add(&mut self, value: f64) {
        if self.samples == 0 {
            self.min = value;
            self.max = value;
        }
        self.samples += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        let n = self.samples as f64;
        self.mean = self.mean * ((n - 1.0) / n) + value / n;
    }
}

fn column_hits(rows: &[Row<'_>], column: usize, units: Units, now: DateTime<Utc>) -> [usize; 10] {
    let mut hits = [0; 10];
    let mut numbers = NumericStats::default();
    for row in rows {
        let cell = row.column(column);
        let number = cell.number();
        if let Some(value) = number {
            numbers.add(value);
        }
        let stamp = cell.timestamp();
        let text = cell.text().unwrap_or("");
        let has_letters = text.bytes().any(|byte| byte.is_ascii_alphabetic());
        let has_digits = text.bytes().any(|byte| byte.is_ascii_digit());
        let mixed = number.is_none()
            && has_letters
            && has_digits
            && text.bytes().all(|byte| byte.is_ascii_alphanumeric());
        let type_code = mixed && text.len() == 4;
        let matching = [
            cell.hex().is_some(),
            stamp.is_some_and(|time| (time - now).abs() <= chrono::Duration::seconds(1000)),
            mixed && (3..=8).contains(&text.len()) && !type_code && cell.hex().is_none(),
            type_code && cell.hex().is_none(),
            cell.bounded(-90.0, 90.0).is_some(),
            cell.bounded(-180.0, 180.0).is_some(),
            units.altitude(cell).is_some(),
            cell.bounded(0.0, 360.0).is_some(),
            units.speed(cell).is_some(),
            number.is_none()
                && stamp.is_none()
                && has_letters
                && (text.contains('-') || (text.starts_with('N') && has_digits && text.len() <= 6)),
        ];
        for (count, matches) in hits.iter_mut().zip(matching) {
            *count += usize::from(matches);
        }
    }
    let mean_ft = if units.altitude_metric {
        numbers.mean * 3.28084
    } else {
        numbers.mean
    };
    if mean_ft <= 5000.0 {
        hits[ALT] = 0;
    }
    hits
}

#[derive(Clone, PartialEq, Eq)]
struct InferenceLayout {
    width: usize,
    configured: [i32; 10],
    named: [bool; 10],
    candidates: [Vec<usize>; 10],
}

impl InferenceLayout {
    fn needs_index(&self, field: usize) -> bool {
        self.configured[field] < 0 && !self.named[field]
    }

    fn augment(
        &self,
        field: usize,
        used: &mut [Option<usize>],
        visited: &mut [bool],
        excluded: Option<(usize, usize)>,
    ) -> bool {
        for &column in &self.candidates[field] {
            if visited[column] || excluded == Some((field, column)) {
                continue;
            }
            visited[column] = true;
            if used[column].is_none_or(|other| self.augment(other, used, visited, excluded)) {
                used[column] = Some(field);
                return true;
            }
        }
        false
    }

    fn assign_required(&self, excluded: Option<(usize, usize)>) -> Option<Vec<Option<usize>>> {
        let mut used = vec![None; self.width];
        for field in REQUIRED
            .into_iter()
            .filter(|&field| self.needs_index(field))
        {
            if !self.augment(field, &mut used, &mut vec![false; self.width], excluded) {
                return None;
            }
        }
        Some(used)
    }

    fn resolve(&self) -> Result<[Option<usize>; 10], FeedError> {
        let configure = || {
            FeedError::Config(
                "FlexJSON requires explicit indices for missing or ambiguous aircraft fields"
                    .into(),
            )
        };
        let used = self.assign_required(None).ok_or_else(configure)?;
        let mut inferred = [None; 10];
        for (column, field) in used
            .iter()
            .enumerate()
            .filter_map(|(column, field)| field.map(|field| (column, field)))
        {
            if self.assign_required(Some((field, column))).is_some() {
                return Err(configure());
            }
            inferred[field] = Some(column);
        }

        for field in OPTIONAL
            .into_iter()
            .filter(|&field| self.needs_index(field))
        {
            let mut matches = self.candidates[field]
                .iter()
                .copied()
                .filter(|&column| used[column].is_none());
            if let Some(column) = matches.next() {
                let overlapping = OPTIONAL.iter().any(|&other| {
                    other != field
                        && self.needs_index(other)
                        && self.candidates[other].contains(&column)
                });
                if matches.next().is_none() && !overlapping {
                    inferred[field] = Some(column);
                }
            }
        }
        Ok(inferred)
    }
}

#[derive(Clone)]
struct Inference {
    layout: InferenceLayout,
    indices: [Option<usize>; 10],
}

fn infer_layout(
    rows: &[Row<'_>],
    width: usize,
    configured: [i32; 10],
    units: Units,
    now: DateTime<Utc>,
) -> InferenceLayout {
    let mut named = [false; 10];
    let mut occupied = vec![false; width];
    for column in configured
        .iter()
        .filter_map(|&index| usize::try_from(index).ok())
    {
        if let Some(used) = occupied.get_mut(column) {
            *used = true;
        }
    }
    for row in rows {
        for field in 0..10 {
            if configured[field] < 0 {
                if let Some((column, _)) = row.named(field) {
                    named[field] = true;
                    if let Some(used) = occupied.get_mut(column) {
                        *used = true;
                    }
                }
            }
        }
    }
    let mut layout = InferenceLayout {
        width,
        configured,
        named,
        candidates: std::array::from_fn(|_| Vec::new()),
    };
    if (0..10).any(|field| layout.needs_index(field)) {
        for (column, _) in occupied.iter().enumerate().filter(|(_, used)| !**used) {
            let hits = column_hits(rows, column, units, now);
            for (field, count) in hits.into_iter().enumerate() {
                if layout.needs_index(field) && count * 10 > rows.len() * 9 {
                    layout.candidates[field].push(column);
                }
            }
        }
    }
    layout
}

fn map_row(
    row: Row<'_>,
    layout: &InferenceLayout,
    inferred: &[Option<usize>; 10],
    units: Units,
    now: DateTime<Utc>,
) -> Option<FeedPlane> {
    let cell = |field: usize| {
        if let Ok(column) = usize::try_from(layout.configured[field]) {
            return row.column(column);
        }
        if layout.named[field] {
            return row.named(field).map_or(Cell::Missing, |(_, cell)| cell);
        }
        inferred[field].map_or(Cell::Missing, |column| row.column(column))
    };
    let optional_text = |field| cell(field).text().unwrap_or("").to_ascii_uppercase();
    let time = match cell(UTC) {
        Cell::Missing => now,
        other => other.timestamp()?,
    };
    Some(FeedPlane {
        hex: cell(HEX).hex()?.to_ascii_uppercase(),
        time: Some(time),
        lat: cell(LAT).bounded(-90.0, 90.0)?,
        lon: cell(LON).bounded(-180.0, 180.0)?,
        alt_ft: units.altitude(cell(ALT))?,
        track: cell(TRACK).bounded(0.0, 360.0)?,
        speed_kts: units.speed(cell(SPEED))?,
        call: optional_text(CALL),
        type_code: optional_text(TYPE),
        reg: optional_text(REG),
        ..FeedPlane::default()
    })
}

pub const DISCLAIMER: &str = "This feed reads JSON from a web server you configure and \
guesses its layout. Some servers do not allow automated use of their data: use only sources \
you are permitted to use. You use this feed at your own risk.";

/// The FlexJSON feed. Indices are column numbers in a row `[key, v1, .., vn]`; -1 means
/// "detect on the next request".
#[derive(Clone)]
pub struct FlexJson {
    pub url: String,
    pub timeout_s: u64,
    pub units_alt: String,
    pub units_speed: String,
    pub min_elements: i32,
    pub min_planes: i32,
    pub index_hex: i32,
    pub index_utc: i32,
    pub index_call: i32,
    pub index_type: i32,
    pub index_lat: i32,
    pub index_lon: i32,
    pub index_alt: i32,
    pub index_track: i32,
    pub index_speed: i32,
    pub index_reg: i32,
    inference: Option<Inference>,
}

impl std::fmt::Debug for FlexJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlexJson")
            .field("timeout_s", &self.timeout_s)
            .field("units_alt", &self.units_alt)
            .field("units_speed", &self.units_speed)
            .field("min_elements", &self.min_elements)
            .field("min_planes", &self.min_planes)
            .field("index_hex", &self.index_hex)
            .field("index_utc", &self.index_utc)
            .field("index_call", &self.index_call)
            .field("index_type", &self.index_type)
            .field("index_lat", &self.index_lat)
            .field("index_lon", &self.index_lon)
            .field("index_alt", &self.index_alt)
            .field("index_track", &self.index_track)
            .field("index_speed", &self.index_speed)
            .field("index_reg", &self.index_reg)
            .finish_non_exhaustive()
    }
}

impl Default for FlexJson {
    fn default() -> Self {
        Self {
            url: String::new(),
            timeout_s: 30,
            units_alt: "ft".into(),
            units_speed: "kts".into(),
            min_elements: 10,
            min_planes: 50,
            index_hex: -1,
            index_utc: -1,
            index_call: -1,
            index_type: -1,
            index_lat: -1,
            index_lon: -1,
            index_alt: -1,
            index_track: -1,
            index_speed: -1,
            index_reg: -1,
            inference: None,
        }
    }
}

fn set_i32(t: &toml::Table, key: &str, dst: &mut i32) -> Result<(), FeedError> {
    match t.get(key) {
        None => Ok(()),
        Some(toml::Value::Integer(i)) => {
            *dst = i32::try_from(*i)
                .map_err(|_| FeedError::Config(format!("{key} is out of range")))?;
            Ok(())
        }
        Some(_) => Err(FeedError::Config(format!("{key} must be a number"))),
    }
}

impl FlexJson {
    fn indices(&self) -> [i32; 10] {
        [
            self.index_hex,
            self.index_utc,
            self.index_call,
            self.index_type,
            self.index_lat,
            self.index_lon,
            self.index_alt,
            self.index_track,
            self.index_speed,
            self.index_reg,
        ]
    }

    fn planes(&mut self, json: &str, now: DateTime<Utc>) -> Result<Vec<FeedPlane>, FeedError> {
        let root = parse_json(json)?;
        let (rows, width) = plane_rows(&root)?;
        let units = Units {
            altitude_metric: self.units_alt.trim().eq_ignore_ascii_case("m"),
            speed_metric: self.units_speed.trim().eq_ignore_ascii_case("km/h"),
        };
        let layout = infer_layout(&rows, width, self.indices(), units, now);
        if self
            .inference
            .as_ref()
            .is_some_and(|inference| inference.layout != layout)
        {
            self.inference = None;
        }
        let inferred = match &self.inference {
            Some(inference) => inference.indices,
            None => layout.resolve()?,
        };
        let planes: Vec<_> = rows
            .into_iter()
            .filter_map(|row| map_row(row, &layout, &inferred, units, now))
            .collect();
        if planes.is_empty() {
            return Err(FeedError::Parse(
                "FlexJSON has no valid aircraft rows".into(),
            ));
        }
        self.inference = Some(Inference {
            layout,
            indices: inferred,
        });
        Ok(planes)
    }
}

impl PlaneFeed for FlexJson {
    fn name(&self) -> &'static str {
        "FlexJSON"
    }

    fn info(&self) -> &'static str {
        "Web feed with flexible JSON column assignment, for web feeds with unknown content. \
         The feed reads the JSON into rows of plane information and guesses which field is \
         in which column. Set explicit indices for ambiguous fields. The legacy min_elements \
         and min_planes settings do not filter the response."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        Some(DISCLAIMER)
    }

    fn enabled_by_default(&self) -> bool {
        true
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("url".into(), self.url.clone().into());
        t.insert("timeout_s".into(), (self.timeout_s as i64).into());
        t.insert("units_alt".into(), self.units_alt.clone().into());
        t.insert("units_speed".into(), self.units_speed.clone().into());
        t.insert("min_elements".into(), i64::from(self.min_elements).into());
        t.insert("min_planes".into(), i64::from(self.min_planes).into());
        let names = [
            "index_hex",
            "index_utc",
            "index_call",
            "index_type",
            "index_lat",
            "index_lon",
            "index_alt",
            "index_track",
            "index_speed",
            "index_reg",
        ];
        for (k, v) in names.into_iter().zip(self.indices()) {
            t.insert(k.into(), i64::from(v).into());
        }
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        self.inference = None;
        set_str(t, "url", &mut self.url)?;
        set_u64(t, "timeout_s", &mut self.timeout_s)?;
        set_str(t, "units_alt", &mut self.units_alt)?;
        set_str(t, "units_speed", &mut self.units_speed)?;
        set_i32(t, "min_elements", &mut self.min_elements)?;
        set_i32(t, "min_planes", &mut self.min_planes)?;
        set_i32(t, "index_hex", &mut self.index_hex)?;
        set_i32(t, "index_utc", &mut self.index_utc)?;
        set_i32(t, "index_call", &mut self.index_call)?;
        set_i32(t, "index_type", &mut self.index_type)?;
        set_i32(t, "index_lat", &mut self.index_lat)?;
        set_i32(t, "index_lon", &mut self.index_lon)?;
        set_i32(t, "index_alt", &mut self.index_alt)?;
        set_i32(t, "index_track", &mut self.index_track)?;
        set_i32(t, "index_speed", &mut self.index_speed)?;
        set_i32(t, "index_reg", &mut self.index_reg)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let now = Utc::now();
        let url = vars::substitute(&self.url, args, now, "")?;
        if url.is_empty() {
            return Err(FeedError::Config("The URL is empty.".into()));
        }
        let json = http_get_bounded(&url, self.timeout_s, None, FLEX_JSON_BYTES)?;
        self.planes(&json, now)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_77_column_statistics_ignore_missing_values() {
        let mut stats = NumericStats::default();
        for cell in [Cell::Number("-9"), Cell::Number("-3"), Cell::Missing] {
            if let Some(value) = cell.number() {
                stats.add(value);
            }
        }
        assert_eq!(
            (stats.min, stats.max, stats.mean, stats.samples),
            (-9.0, -3.0, -6.0, 2)
        );
    }

    #[test]
    fn finding_77_inference_commits_only_after_a_usable_layout() {
        let mut feed = FlexJson::default();
        let settings: toml::Table = "index_lat=2\nindex_lon=3".parse().unwrap();
        feed.load_settings(&settings).unwrap();
        let now = DateTime::from_timestamp(2_147_483_647, 0).unwrap();
        let valid = r#"[["3C0001",50.25,10.25,12000,80,430,2147484647]]"#;
        let planes = feed.planes(valid, now).unwrap();
        assert_eq!(planes[0].time, DateTime::from_timestamp(2_147_484_647, 0));
        assert!(feed.inference.is_some());
        assert_eq!(feed.settings()["index_hex"].as_integer(), Some(-1));

        let ambiguous = r#"[["3C0001",50.25,10.25,12000,80,90]]"#;
        assert!(matches!(
            feed.planes(ambiguous, now),
            Err(FeedError::Config(_))
        ));
        assert!(feed.inference.is_none());
        feed.planes(valid, now).unwrap();
        assert!(feed.inference.is_some());
        feed.load_settings(&settings).unwrap();
        assert!(feed.inference.is_none());
    }

    #[test]
    fn finding_375_flexjson_parser_caps_utf16_units_independently_of_transport() {
        let exact = format!("\"{}\"", "🙂".repeat((MAX_JSON_LENGTH - 2) / 2));
        assert!(exact.len() > MAX_JSON_LENGTH);
        assert!(parse_json(&exact).is_ok());
        assert!(parse_json(&(exact + " ")).is_err());
    }
}
