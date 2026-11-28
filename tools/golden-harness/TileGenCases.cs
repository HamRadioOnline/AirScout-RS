// Stage 5b: reference vectors for ElevationTileGenerator (UpstreamTileGen.cs) from synthetic
// sources. Sample (r, c), row from the top of the file, column from the west:
//   in a voids rectangle [r0, r1, c0, c1, v] (half-open): v;
//   else in a fill rectangle [r0, r1, c0, c1]: ((r * 31 + c * 17) % 5200) - 700
//     ("formula": "downsample": ((r * 7 + c * 3) % 3000) - 100);
//   else 0.
// SRTM files are big-endian, GLOBE files little-endian. The sources live in
// Path.GetTempPath()/tilegen and are deleted afterwards; no DEM data is committed.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace GoldenHarness
{
    public static partial class Program
    {
        sealed class TileGenSource
        {
            public string Name, Kind, Formula;
            public int Rows, Cols;
            public int[][] Fill = new int[0][];
            public int[][] Voids = new int[0][];

            public short Value(int r, int c)
            {
                foreach (var v in Voids)
                    if (r >= v[0] && r < v[1] && c >= v[2] && c < v[3]) return (short)v[4];
                foreach (var f in Fill)
                    if (r >= f[0] && r < f[1] && c >= f[2] && c < f[3])
                        return Formula == "downsample" ? (short)(((r * 7 + c * 3) % 3000) - 100) : (short)(((r * 31 + c * 17) % 5200) - 700);
                return 0;
            }

            public JObject Args()
            {
                var o = new JObject { ["name"] = Name, ["kind"] = Kind, ["rows"] = Rows, ["cols"] = Cols, ["fill"] = Rects(Fill), ["voids"] = Rects(Voids) };
                if (Formula != null) o["formula"] = Formula;
                return o;
            }

            static JArray Rects(int[][] rs) => new JArray(rs.Select(r => new JArray(r)));

            public void WriteTo(string path)
            {
                bool bigEndian = Kind != "GLOBE";
                if (Kind != "GLOBE")
                {
                    var bytes = new byte[Rows * Cols * 2];
                    for (int r = 0; r < Rows; r++)
                        for (int c = 0; c < Cols; c++)
                            Put(bytes, (r * Cols + c) * 2, Value(r, c), bigEndian);
                    File.WriteAllBytes(path, bytes);
                    return;
                }
                // GLOBE: sparse file, only the rows that touch a rectangle are written
                using (var fs = new FileStream(path, FileMode.Create, FileAccess.Write))
                {
                    fs.SetLength((long)Rows * Cols * 2);
                    var rows = new SortedSet<int>();
                    foreach (var q in Fill.Concat(Voids))
                        for (int r = q[0]; r < q[1]; r++) rows.Add(r);
                    var line = new byte[Cols * 2];
                    foreach (int r in rows)
                    {
                        for (int c = 0; c < Cols; c++) Put(line, c * 2, Value(r, c), bigEndian);
                        fs.Position = (long)r * Cols * 2;
                        fs.Write(line, 0, line.Length);
                    }
                }
            }

            static void Put(byte[] b, int at, short v, bool bigEndian)
            {
                byte hi = (byte)((v >> 8) & 0xFF), lo = (byte)(v & 0xFF);
                b[at] = bigEndian ? hi : lo;
                b[at + 1] = bigEndian ? lo : hi;
            }
        }

        static string Fnv64(IEnumerable<byte> bytes)
        {
            ulong h = 0xcbf29ce484222325UL;
            foreach (byte b in bytes) { h ^= b; h *= 0x100000001b3UL; }
            return h.ToString("x16");
        }

        static string Bits(double x) => BitConverter.DoubleToInt64Bits(x).ToString("x16");

        static JArray TileGenCases()
        {
            var a = new JArray();
            string tmp = Path.Combine(Path.GetTempPath(), "tilegen");
            if (Directory.Exists(tmp)) Directory.Delete(tmp, true);
            Directory.CreateDirectory(tmp);
            try
            {
                const int V = -32768;
                var all = new[] { 0, 1201, 0, 1201 };
                var all1 = new[] { 0, 3601, 0, 3601 };
                var n50e011Voids = new[] { new[] { 1150, 1200, 0, 100, V }, new[] { 1150, 1200, 100, 200, V } };
                // (source, GLOBE loop values i0, j0, i1, j1)
                var tileSources = new (TileGenSource, int[])[]
                {
                    (new TileGenSource { Name = "N50E011.hgt", Kind = "SRTM3", Rows = 1201, Cols = 1201, Fill = new[] { all }, Voids = n50e011Voids }, null),
                    (new TileGenSource { Name = "S01W002.hgt", Kind = "SRTM3", Rows = 1201, Cols = 1201, Fill = new[] { all } }, null),
                    (new TileGenSource { Name = "N50E010.hgt", Kind = "SRTM1", Rows = 3601, Cols = 3601, Fill = new[] { all1 } }, null),
                    (new TileGenSource { Name = "C10G", Kind = "GLOBE", Rows = 4800, Cols = 10800, Fill = new[] { new[] { 4680, 4800, 1200, 1440 } }, Voids = new[] { new[] { 4795, 4800, 1200, 1210, -600 } } }, new[] { 0, 0, 5, 0 }),
                    (new TileGenSource { Name = "G10G", Kind = "GLOBE", Rows = 6000, Cols = 10800, Fill = new[] { new[] { 0, 120, 10560, 10800 } } }, new[] { 4, 4, 4, 9 }),
                };
                foreach (var (src, loops) in tileSources)
                {
                    string path = Path.Combine(tmp, src.Name);
                    src.WriteTo(path);
                    a.Add(Case("source", src.Args(), () => null));
                    // fresh output tree per source: ProcessTile skips existing files (MainDlg.cs:239, :448)
                    string dest = Path.Combine(tmp, "out", src.Name);
                    Directory.CreateDirectory(dest);
                    var gen = new UpstreamTileGen();
                    var l = loops ?? new[] { 0, 0, 0, 0 };
                    gen.ProcessTile(path, dest, l[0], l[1], l[2], l[3]);
                    string fn = src.Kind == "GLOBE" ? "globe_tile" : "srtm_tile";
                    foreach (string tile in gen.Visited)
                    {
                        string t = tile;
                        a.Add(Case(fn, new JObject { ["source"] = src.Name, ["tile"] = t },
                            () => TileResult(Path.Combine(dest, t.Substring(0, 2), t.Substring(2, 2), t + ".loc"))));
                    }
                }

                // downsample: C# output as-is. "counts" and "cells" classify the 3 x 3 blocks so the
                // port (i32 sum; a void in the block gives a void; owner decision S13) can be checked
                // where it agrees and where it deviates.
                var dsrc = new TileGenSource { Name = "dsrc.hgt", Kind = "SRTM1", Rows = 3601, Cols = 3601, Fill = new[] { all1 }, Formula = "downsample" };
                a.Add(Case("source", dsrc.Args(), () => null));
                a.Add(Case("downsample", new JObject { ["source"] = dsrc.Name }, () => Downsample(tmp, dsrc)));
                var dov = new TileGenSource
                {
                    Name = "dsrc-ov.hgt", Kind = "SRTM1", Rows = 3601, Cols = 3601, Fill = new[] { all1 },
                    Voids = new[] { new[] { 0, 3, 0, 3, V }, new[] { 100, 101, 200, 201, V }, new[] { 3600, 3601, 0, 3, V } },
                };
                a.Add(Case("source", dov.Args(), () => null));
                a.Add(Case("downsample_deviation", new JObject { ["source"] = dov.Name }, () => Downsample(tmp, dov)));

                // compare: N50E011.hgt (written above) against cmp-b.hgt
                var cmpb = new TileGenSource
                {
                    Name = "cmp-b.hgt", Kind = "SRTM3", Rows = 1201, Cols = 1201, Fill = new[] { all },
                    Voids = n50e011Voids.Concat(new[] { new[] { 0, 10, 0, 1201, V }, new[] { 600, 601, 0, 1201, 1000 } }).ToArray(),
                };
                cmpb.WriteTo(Path.Combine(tmp, cmpb.Name));
                a.Add(Case("source", cmpb.Args(), () => null));
                a.Add(Case("compare", new JObject { ["a"] = "N50E011.hgt", ["b"] = cmpb.Name }, () =>
                {
                    string outDir = Path.Combine(tmp, "cmp");
                    Directory.CreateDirectory(outDir);
                    // CompareFilesHGT prints a line for every difference > 1000 (MainDlg.cs:1128-1129)
                    var stdout = Console.Out;
                    string diff;
                    Console.SetOut(TextWriter.Null);
                    try { diff = new UpstreamTileGen().CompareFilesHGT(Path.Combine(tmp, "N50E011.hgt"), Path.Combine(tmp, cmpb.Name), outDir); }
                    finally { Console.SetOut(stdout); }
                    var bytes = File.ReadAllBytes(diff);
                    return new JObject { ["len"] = bytes.Length, ["fnv"] = Fnv64(bytes) };
                }));
            }
            finally
            {
                Directory.Delete(tmp, true);
            }
            return a;
        }

        static JToken TileResult(string loc)
        {
            if (!File.Exists(loc)) return new JObject { ["written"] = false };
            JObject o;
            using (var jr = new JsonTextReader(new StringReader(File.ReadAllText(loc))) { DateParseHandling = DateParseHandling.None })
                o = JObject.Load(jr);
            var elv = new List<byte>();
            int n = 0;
            foreach (JArray col in (JArray)o["Elv"])
                foreach (var v in col)
                {
                    short e = (short)v;
                    elv.Add((byte)(e & 0xFF));
                    elv.Add((byte)((e >> 8) & 0xFF));
                    n++;
                }
            var r = new JObject { ["written"] = true, ["members"] = new JArray(o.Properties().Select(p => p.Name)) };
            r["min_elv"] = (int)o["MinElv"];
            r["max_elv"] = (int)o["MaxElv"];
            r["rows"] = (int)o["Rows"];
            r["columns"] = (int)o["Columns"];
            foreach (var (key, member) in new[] { ("min_lat", "MinLat"), ("min_lon", "MinLon"), ("max_lat", "MaxLat"), ("max_lon", "MaxLon") })
            {
                double x = (double)o[member];
                r[key] = x;
                r[key + "_bits"] = Bits(x);
            }
            r["elv_len"] = n;
            r["elv_fnv"] = Fnv64(elv);
            r["last_updated"] = (string)o["LastUpdated"];
            return r;
        }

        /// Runs the verbatim ConvertSRTM1ToSRTM3File on the source. Result: len and fnv of the
        /// output bytes, plus a classification of the 1200 x 1200 averaged points by their 3 x 3
        /// source block: "void" (a sample is -32768), else "overflow" (the int sum is outside
        /// short, so the C# short sum wraps), else "clean". "cells" lists the C# value of the
        /// first 8 points of each class (row-major), and the layout points: column 1200 ("dup",
        /// a copy of the row's last averaged value) and row 1200 ("raw", source sample (3600, 3 * col)).
        static JToken Downsample(string tmp, TileGenSource src)
        {
            string inPath = Path.Combine(tmp, src.Name), outPath = Path.Combine(tmp, Path.GetFileNameWithoutExtension(src.Name) + "-3.hgt");
            src.WriteTo(inPath);
            new UpstreamTileGen().ConvertSRTM1ToSRTM3File(inPath, outPath);
            var input = File.ReadAllBytes(inPath);
            var output = File.ReadAllBytes(outPath);
            File.Delete(inPath);
            short In(int r, int c) => (short)((input[(r * 3601 + c) * 2] << 8) | input[(r * 3601 + c) * 2 + 1]);
            short Out(int r, int c) => (short)((output[(r * 1201 + c) * 2] << 8) | output[(r * 1201 + c) * 2 + 1]);
            string Class(int i, int j)
            {
                int sum = 0;
                bool hasVoid = false;
                for (int k = 0; k < 3; k++)
                    for (int l = 0; l < 3; l++)
                    {
                        short e = In(i * 3 + k, j * 3 + l);
                        if (e == -32768) hasVoid = true;
                        sum += e;
                    }
                if (hasVoid) return "void";
                return (sum < short.MinValue || sum > short.MaxValue) ? "overflow" : "clean";
            }
            var counts = new Dictionary<string, int> { ["clean"] = 0, ["overflow"] = 0, ["void"] = 0 };
            var cells = new JArray();
            JObject Cell(int i, int j, string cls) => new JObject { ["row"] = i, ["col"] = j, ["class"] = cls, ["csharp"] = (int)Out(i, j) };
            int firstOverflowRowAtEnd = -1;
            for (int i = 0; i < 1200; i++)
                for (int j = 0; j < 1200; j++)
                {
                    string cls = Class(i, j);
                    if (counts[cls]++ < 8) cells.Add(Cell(i, j, cls));
                    if (j == 1199 && cls == "overflow" && firstOverflowRowAtEnd < 0) firstOverflowRowAtEnd = i;
                }
            var dupRows = new List<int> { 0, 1199 };
            if (firstOverflowRowAtEnd >= 0) dupRows.Insert(1, firstOverflowRowAtEnd);
            foreach (int i in dupRows)
            {
                cells.Add(Cell(i, 1199, Class(i, 1199)));
                cells.Add(Cell(i, 1200, "dup"));
            }
            foreach (int j in new[] { 0, 1, 1200 })
                cells.Add(Cell(1200, j, "raw"));
            return new JObject
            {
                ["len"] = output.Length,
                ["fnv"] = Fnv64(output),
                ["counts"] = new JObject { ["clean"] = counts["clean"], ["overflow"] = counts["overflow"], ["void"] = counts["void"] },
                ["cells"] = cells,
            };
        }
    }
}
