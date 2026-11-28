// binCraft parsers of the BinCraft and PublicBinCraft plugins on one synthetic buffer:
// header + 3 records of 112 bytes, every field holding a distinct value so that each
// offset can be told apart.
using System;
using System.Collections.Generic;
using System.Text;
using Newtonsoft.Json.Linq;
using AirScout.PlaneFeeds.Plugin.PublicBinCraft;

namespace GoldenHarness
{
    public static partial class Program
    {
        const int BcStride = 112;

        static void PutU16(byte[] b, int o, int v) { b[o] = (byte)v; b[o + 1] = (byte)(v >> 8); }
        static void PutU32(byte[] b, int o, long v) { for (int i = 0; i < 4; i++) b[o + i] = (byte)(v >> (8 * i)); }
        static void PutStr(byte[] b, int o, string s) { var a = Encoding.ASCII.GetBytes(s); Array.Copy(a, 0, b, o, a.Length); }

        public static byte[] BinCraftBuffer(int stride, int records)
        {
            var b = new byte[stride * (records + 1)];
            long now = 1795435200700L;
            PutU32(b, 0, now & 0xFFFFFFFF); PutU32(b, 4, now >> 32);
            PutU32(b, 8, stride); PutU32(b, 12, records); PutU32(b, 16, 7);
            PutU16(b, 20, 50); PutU16(b, 22, 10); PutU16(b, 24, 51); PutU16(b, 26, 12);
            PutU32(b, 28, 123456); PutU32(b, 32, records); PutU32(b, 40, 20240218); PutU32(b, 44, 1234); PutU32(b, 48, 0);
            for (int r = 0; r < records && stride >= 112; r++)
            {
                int o = stride * (r + 1);
                PutU32(b, o + 0, 0x05000000L | (0x3C6589 + r));
                PutU16(b, o + 4, 10 + r); PutU16(b, o + 6, 20 + r);
                PutU32(b, o + 8, 10987654 + 1000 * r); PutU32(b, o + 12, 50612345 + 1000 * r);
                PutU16(b, o + 16, 8 + r); PutU16(b, o + 18, 9 + r);
                PutU16(b, o + 20, 1480 + r); PutU16(b, o + 22, 1505 + r);
                PutU16(b, o + 24, 2400 + r); PutU16(b, o + 26, 2600 + r);
                PutU16(b, o + 32, 0x1000 + r); PutU16(b, o + 34, 4823 + r);
                PutU16(b, o + 40, 22114 + r); PutU16(b, o + 44, 22000 + r); PutU16(b, o + 46, 22050 + r);
                PutU16(b, o + 54, 120 + r);
                b[o + 64] = (byte)(0xA3 + r);
                b[o + 68] = (byte)(0x40 | (r == 1 ? 1 : 2));
                // record 0: all valid; 1: callsign invalid; 2: no position, no baro alt
                b[o + 73] = r == 0 ? (byte)0xF8 : r == 1 ? (byte)0xF0 : (byte)0xA8;
                b[o + 74] = 0xC8; b[o + 75] = 0x01; b[o + 76] = 0x04;
                PutStr(b, o + 78, r == 0 ? "DLH4AB  " : r == 1 ? "RYR12KA" : "EWG7MB");
                PutStr(b, o + 88, r == 0 ? "A388" : r == 1 ? "B738" : "AT76");
                PutStr(b, o + 92, r == 0 ? "D-AIMA" : r == 1 ? "EI-DWE" : "D-AEWM");
                PutU32(b, o + 108, 12 + r);
            }
            return b;
        }

        static JToken N(double? v) => v.HasValue ? new JValue(v.Value) : JValue.CreateNull();

        static JToken BinCraftOld(byte[] data)
        {
            var d = BinCraftParser.Parse(data, false);
            var list = new JArray();
            foreach (var ac in (List<Dictionary<string, object>>)d["aircraft"])
            {
                var o = new JObject();
                foreach (var kv in ac) o[kv.Key] = JToken.FromObject(kv.Value);
                list.Add(o);
            }
            return new JObject { ["now"] = (double)d["now"], ["aircraft"] = list };
        }

        static JToken BinCraftPublic(byte[] data)
        {
            var h = PublicBinCraftParser.ParseHeader(data);
            var list = new JArray();
            foreach (var ac in PublicBinCraftParser.ParseAircraft(data, h))
                list.Add(new JObject
                {
                    ["hex"] = ac.Hex, ["seen"] = N(ac.Seen), ["lat"] = N(ac.Lat), ["lon"] = N(ac.Lon),
                    ["baro_rate"] = N(ac.BaroRate), ["baro_alt"] = N(ac.BaroAlt), ["geom_alt"] = N(ac.GeomAlt),
                    ["squawk"] = S(ac.Squawk), ["gs"] = N(ac.Gs), ["track"] = N(ac.Track),
                    ["category"] = ac.Category, ["air_ground"] = (int)ac.AirGround,
                    ["callsign"] = S(ac.Callsign), ["type"] = S(ac.TypeCode), ["reg"] = S(ac.Registration),
                    ["seen_pos"] = N(ac.SeenPos),
                });
            return new JObject
            {
                ["now_ms"] = (long)(h.Now - new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc)).TotalMilliseconds,
                ["element_size"] = h.ElementSize,
                ["aircraft"] = list,
            };
        }

        static JArray BinCraftCases()
        {
            var cases = new JArray();
            var inputs = new List<(string name, byte[] data)>
            {
                ("three_records", BinCraftBuffer(BcStride, 3)),
                // a trailing partial record is ignored by the loop condition
                ("partial_tail", Truncate(BinCraftBuffer(BcStride, 3), BcStride * 4 - 10)),
                ("empty", new byte[0]),
                ("header_only_short", Truncate(BinCraftBuffer(BcStride, 0), 20)),
                // stride 32: the last record's fields reach past the buffer
                ("stride_too_small", BinCraftBuffer(32, 2)),
            };
            foreach (var (name, data) in inputs)
            {
                var args = new JObject { ["name"] = name, ["data"] = Convert.ToBase64String(data) };
                cases.Add(Case("bincraft", args, () => BinCraftOld(data)));
                cases.Add(Case("public_bincraft", args, () => BinCraftPublic(data)));
            }
            return cases;
        }

        static byte[] Truncate(byte[] b, int n) { var r = new byte[n]; Array.Copy(b, r, n); return r; }
    }
}
