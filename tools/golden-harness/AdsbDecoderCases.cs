// Golden cases for the stateful LibADSB ADSBDecoder (UpstreamADSBDecoder.shim.cs) under a fake clock.
using System;
using System.Collections.Generic;
using System.Reflection;
using Newtonsoft.Json.Linq;
using LibADSB;

namespace GoldenHarness
{
    public static class FakeClock
    {
        public static DateTime UtcNow = new DateTime(2026, 11, 23, 12, 0, 0, DateTimeKind.Utc);
    }

    public static partial class Program
    {
        static readonly DateTime AdsbT0 = new DateTime(2026, 11, 23, 12, 0, 0, DateTimeKind.Utc);

        // 22 hex chars (DF/CA + payload) -> "*<hex><crc>;"
        static string Frame(string body)
        {
            var b = new byte[body.Length / 2];
            for (int i = 0; i < b.Length; i++) b[i] = Convert.ToByte(body.Substring(2 * i, 2), 16);
            return "*" + body + Hex(ModeSReply.calcParity(b)) + ";";
        }

        static JToken AdsbTs(DateTime t) => t == DateTime.MinValue ? (JToken)JValue.CreateNull() : (t - AdsbT0).TotalSeconds;
        static JToken AdsbInt(int v) => v == int.MinValue ? (JToken)JValue.CreateNull() : v;

        static JObject InfoJ(ADSBInfo i) => new JObject
        {
            ["timestamp"] = AdsbTs(i.Timestamp), ["icao24"] = i.ICAO24, ["call"] = i.Call,
            ["lat"] = i.Lat, ["lon"] = i.Lon, ["baro_alt"] = AdsbInt(i.BaroAlt), ["geo_minus_baro"] = AdsbInt(i.GeoMinusBaro),
            ["heading"] = AdsbInt(i.Heading), ["speed"] = AdsbInt(i.Speed),
            ["last_even"] = AdsbTs(i.LastEvenTimestamp), ["last_odd"] = AdsbTs(i.LastOddTimestamp), ["last_pos"] = AdsbTs(i.LastPosTimestamp),
            ["num_pos"] = i.NumPos,
        };

        static JArray AdsbDecoderCases()
        {
            // aircraft A (40621D) and B (4840D6), valid CRC
            string aId = Frame("8D40621D202CC371C32CE0"), aEven = Frame("8D40621D58C382D690C8AC"), aOdd = Frame("8D40621D58C386435CC412");
            string aVel = Frame("8D40621D99440994083817"), aAirspeed = Frame("8D40621D9B06B6AF189400");
            string aEven2 = Frame("8D40621D58C382D6A0C8B0"), aOddStraddle = Frame("8D40621D58C38621A00000");
            string bId = Frame("8D4840D6202CC371C32CE0"), bEven = Frame("8D4840D658C382D690C8AC"), bOdd = Frame("8D4840D658C38612345678");
            string bVel = Frame("8D4840D69A400914083817");
            string unknownTc = Frame("8D40621DE0C382D690C8AC");

            var scenarios = new (string name, (double t, string op, string raw)[] steps)[] {
                ("two_aircraft", new[] {
                    (1.0, "d", aId), (2.0, "d", aEven), (3.0, "d", aOdd), (4.0, "d", aVel),
                    (4.5, "d", bId), (5.0, "d", bEven), (6.0, "d", bOdd), (7.0, "d", bVel),
                    (8.0, "p", ""), (9.0, "d", aEven2), (10.0, "p", ""), (11.0, "p", ""),
                }),
                ("pair_11s", new[] {
                    (1.0, "d", aId), (2.0, "d", aVel), (3.0, "d", aEven), (14.0, "d", aOdd), (15.0, "p", ""),
                    (15.5, "d", aEven), (16.0, "p", ""),
                }),
                ("pair_exactly_10s", new[] { (1.0, "d", aOdd), (11.0, "d", aEven) }),
                ("odd_then_even", new[] { (1.0, "d", aOdd), (2.0, "d", aEven), (3.0, "d", aOdd) }),
                ("position_before_ident", new[] {
                    (1.0, "d", aEven), (2.0, "d", aOdd), (3.0, "d", aVel), (4.0, "p", ""), (5.0, "d", aId), (6.0, "p", ""),
                    (7.0, "d", aOdd), (8.0, "p", ""),
                }),
                ("straddle", new[] { (1.0, "d", aEven), (2.0, "d", aOddStraddle), (3.0, "d", aEven) }),
                ("airspeed_heading", new[] { (1.0, "d", aAirspeed), (2.0, "d", aVel), (3.0, "d", aAirspeed) }),
                ("rejects", new[] {
                    (1.0, "d", "*8D4840D6202CC371C32CE0576099;"), (2.0, "d", "*8D4840D623FFFFFFFFFFFF000000;"),
                    (3.0, "d", unknownTc), (4.0, "d", "*5D484FDEA248F5;"), (5.0, "d", "*8D4840D6202CC;"),
                    (6.0, "d", ""), (7.0, "d", "*"), (8.0, "d", "*ZZ4840D6202CC371C32CE0576098;"), (9.0, "p", ""),
                }),
            };

            var field = typeof(ADSBDecoder).GetField("adsbinfos", BindingFlags.NonPublic | BindingFlags.Instance);
            var a = new JArray();
            foreach (var sc in scenarios)
            {
                FakeClock.UtcNow = AdsbT0;
                var dec = new ADSBDecoder();
                var steps = new JArray();
                var results = new JArray();
                foreach (var st in sc.steps)
                {
                    FakeClock.UtcNow = AdsbT0.AddSeconds(st.t);
                    steps.Add(new JObject { ["t"] = st.t, ["op"] = st.op, ["raw"] = st.raw });
                    var r = new JObject();
                    if (st.op == "d")
                    {
                        try { r["ret"] = dec.DecodeMessage(st.raw, FakeClock.UtcNow, false); }
                        catch (Exception ex) { r["error"] = ex.GetType().Name; }
                    }
                    else
                    {
                        var pl = new JArray();
                        foreach (ADSBInfo i in dec.GetPlanes()) pl.Add(InfoJ(i));
                        r["planes"] = pl;
                    }
                    var state = new JArray();
                    foreach (var kv in (Dictionary<string, ADSBInfo>)field.GetValue(dec)) state.Add(InfoJ(kv.Value));
                    r["state"] = state;
                    results.Add(r);
                }
                a.Add(new JObject { ["fn"] = "scenario", ["args"] = new JObject { ["name"] = sc.name, ["steps"] = steps }, ["result"] = results });
            }
            return a;
        }
    }
}
