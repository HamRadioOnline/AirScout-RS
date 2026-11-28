//! Helpers shared by the `elevation-tile-gen` tests: synthetic sources built from the golden
//! `source` cases of `tests/golden/elevation_tilegen.json` (no DEM data is committed).
#![allow(dead_code)]

use as_golden::{Case, Golden};
use serde_json::Value;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// The tile generator golden file.
pub fn golden() -> Golden {
    as_golden::load("elevation_tilegen.json")
}

/// The `source` case named `name`.
pub fn source_case<'a>(g: &'a Golden, name: &str) -> &'a Case {
    g.by_fn("source")
        .find(|c| c.args["name"] == name)
        .unwrap_or_else(|| panic!("no source case {name}"))
}

fn rects(v: &Value) -> Vec<Vec<i64>> {
    v.as_array()
        .expect("rectangle list")
        .iter()
        .map(|r| {
            r.as_array()
                .expect("rectangle")
                .iter()
                .map(|x| x.as_i64().expect("int"))
                .collect()
        })
        .collect()
}

/// The synthetic source of a golden `source` case (rows, columns, rectangles).
pub struct Synth {
    pub name: String,
    pub kind: String,
    pub rows: usize,
    pub cols: usize,
    fill: Vec<Vec<i64>>,
    voids: Vec<Vec<i64>>,
    downsample: bool,
}

impl Synth {
    pub fn from_case(c: &Case) -> Self {
        Self {
            name: c.s("name"),
            kind: c.s("kind"),
            rows: c.i("rows") as usize,
            cols: c.i("cols") as usize,
            fill: rects(&c.args["fill"]),
            voids: rects(&c.args["voids"]),
            downsample: c.args.get("formula").and_then(Value::as_str) == Some("downsample"),
        }
    }

    /// The same source with other fill and void rectangles.
    pub fn with_rects(mut self, fill: Vec<[i64; 4]>, voids: Vec<[i64; 5]>) -> Self {
        self.fill = fill.into_iter().map(|r| r.to_vec()).collect();
        self.voids = voids.into_iter().map(|r| r.to_vec()).collect();
        self
    }

    fn big_endian(&self) -> bool {
        self.kind != "GLOBE"
    }

    /// Sample `(r, c)`: row from the top of the file, column from the west.
    pub fn value(&self, r: usize, c: usize) -> i16 {
        let (r, c) = (r as i64, c as i64);
        let inside = |q: &[i64]| r >= q[0] && r < q[1] && c >= q[2] && c < q[3];
        if let Some(v) = self.voids.iter().find(|q| inside(q)) {
            return v[4] as i16;
        }
        if self.fill.iter().any(|q| inside(q)) {
            return if self.downsample {
                (((r * 7 + c * 3) % 3000) - 100) as i32 as i16
            } else {
                (((r * 31 + c * 17) % 5200) - 700) as i32 as i16
            };
        }
        0
    }

    fn put(&self, v: i16) -> [u8; 2] {
        if self.big_endian() {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    }

    /// One file row.
    pub fn row_bytes(&self, r: usize) -> Vec<u8> {
        (0..self.cols)
            .flat_map(|c| self.put(self.value(r, c)))
            .collect()
    }

    /// The rows that touch a rectangle (the only rows a sparse GLOBE file writes).
    fn touched_rows(&self) -> Vec<usize> {
        let mut rows: Vec<usize> = self
            .fill
            .iter()
            .chain(self.voids.iter())
            .flat_map(|q| q[0] as usize..q[1] as usize)
            .filter(|&r| r < self.rows)
            .collect();
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    /// The whole file in memory.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.rows * self.cols * 2];
        let rows = if self.big_endian() {
            (0..self.rows).collect()
        } else {
            self.touched_rows()
        };
        for r in rows {
            let at = r * self.cols * 2;
            out[at..at + self.cols * 2].copy_from_slice(&self.row_bytes(r));
        }
        out
    }

    /// Write the file as `<dir>/<name>`. A GLOBE file is sparse: `set_len`, then only the rows
    /// that touch a rectangle.
    pub fn write(&self, dir: &Path) -> PathBuf {
        self.write_as(dir, &self.name)
    }

    pub fn write_as(&self, dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        if self.big_endian() {
            std::fs::write(&path, self.bytes()).unwrap();
            return path;
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.set_len((self.rows * self.cols * 2) as u64).unwrap();
        for r in self.touched_rows() {
            f.seek(SeekFrom::Start((r * self.cols * 2) as u64)).unwrap();
            f.write_all(&self.row_bytes(r)).unwrap();
        }
        path
    }
}

/// The in-memory bytes of a golden `source` case.
pub fn source_bytes(c: &Case) -> Vec<u8> {
    Synth::from_case(c).bytes()
}

/// A sparse file of `len` zero bytes.
pub fn sparse(path: &Path, len: u64) {
    std::fs::File::create(path).unwrap().set_len(len).unwrap();
}

/// FNV-1a 64 over the little-endian bytes of the values (the harness `Fnv64`).
pub fn fnv(elv: &[i16]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in elv.iter().flat_map(|v| v.to_le_bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// FNV-1a 64 over raw bytes (the harness `Fnv64` of a file).
pub fn fnv_bytes(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// All `.loc` and `.part` files under `dir`, sorted.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}
