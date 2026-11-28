// Golden cases for PlaneInfoCache (Get / GetAll extrapolation).
using System;
using System.Collections.Generic;
using Newtonsoft.Json.Linq;
using AirScout.Aircrafts;
using AirScout.Core;

namespace GoldenHarness
{
    public static partial class Program
    {
        static readonly DateTime CacheT0 = new DateTime(2023, 11, 14, 22, 0, 0, DateTimeKind.Utc);

        // report: hex, t [s after CacheT0], lat, lon, alt [ft], track, speed [kts]
        static readonly (string name, (string hex, double t, double lat, double lon, double alt, double track, double speed)[] reports, double[] queries)[] CacheScenarios = {
            ("straight", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 90.0, 450.0), ("3C6444", 60.0, 50.0, 8.18, 35000.0, 90.0, 450.0) }, new[] { 60.0, 90.0, 120.0, 300.0, 360.0, 361.0 }),
            ("turning", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 80.0, 450.0), ("3C6444", 60.0, 50.02, 8.18, 35500.0, 90.0, 460.0) }, new[] { 90.0, 180.0 }),
            ("fast_turn_45", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 80.0, 450.0), ("3C6444", 60.0, 50.02, 8.18, 35000.0, 120.0, 450.0) }, new[] { 90.0, 120.0, 150.0 }),
            ("wrap_350_10", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 350.0, 450.0), ("3C6444", 60.0, 50.1, 8.0, 35000.0, 10.0, 450.0) }, new[] { 90.0, 120.0 }),
            ("wrap_10_350", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 10.0, 450.0), ("3C6444", 60.0, 50.1, 8.0, 35000.0, 350.0, 450.0) }, new[] { 90.0, 120.0 }),
            ("track_to_360", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 350.0, 450.0), ("3C6444", 60.0, 50.1, 8.0, 35000.0, 355.0, 450.0) }, new[] { 120.0 }),
            ("climb", new[] { ("3C6444", 0.0, 50.0, 8.0, 30000.0, 90.0, 400.0), ("3C6444", 60.0, 50.0, 8.15, 32000.0, 90.0, 420.0) }, new[] { 90.0, 240.0 }),
            ("single_report", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 45.0, 450.0) }, new[] { 0.0, 30.0, 300.0, 301.0 }),
            ("query_before_report", new[] { ("3C6444", 60.0, 50.0, 8.0, 35000.0, 45.0, 450.0) }, new[] { 30.0, 59.0 }),
            ("speed_zero", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 90.0, 0.0), ("3C6444", 60.0, 50.0, 8.0, 35000.0, 90.0, 0.0) }, new[] { 90.0 }),
            ("alt_over_50000", new[] { ("3C6444", 0.0, 50.0, 8.0, 49000.0, 90.0, 450.0), ("3C6444", 60.0, 50.0, 8.18, 51000.0, 90.0, 450.0) }, new[] { 90.0 }),
            ("decelerate_negative", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 90.0, 450.0), ("3C6444", 60.0, 50.0, 8.18, 35000.0, 90.0, 100.0) }, new[] { 90.0 }),
            ("same_time_not_newer", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 90.0, 450.0), ("3C6444", 0.0, 51.0, 9.0, 20000.0, 180.0, 300.0) }, new[] { 30.0 }),
            ("older_report_ignored", new[] { ("3C6444", 60.0, 50.0, 8.18, 35000.0, 90.0, 450.0), ("3C6444", 0.0, 50.0, 8.0, 34000.0, 80.0, 440.0) }, new[] { 90.0 }),
            ("three_reports", new[] { ("3C6444", 0.0, 50.0, 8.0, 35000.0, 90.0, 450.0), ("3C6444", 60.0, 50.0, 8.18, 35100.0, 92.0, 452.0), ("3C6444", 120.0, 49.99, 8.36, 35300.0, 95.0, 455.0) }, new[] { 150.0 }),
            ("two_planes", new[] { ("4CA123", 0.0, 51.0, 9.0, 36000.0, 270.0, 480.0), ("3C6444", 0.0, 50.0, 8.0, 35000.0, 90.0, 450.0), ("4CA123", 30.0, 51.0, 8.9, 36000.0, 270.0, 480.0), ("3C6444", 40.0, 50.0, 8.12, 35000.0, 90.0, 450.0) }, new[] { 60.0 }),
            ("south_west", new[] { ("E80123", 0.0, -33.9, 18.4, 37000.0, 225.0, 470.0), ("E80123", 60.0, -34.0, 18.3, 37000.0, 226.0, 471.0) }, new[] { 120.0 }),
        };

        static JToken PlaneJ(PlaneInfo p)
        {
            if (p == null) return JValue.CreateNull();
            return new JObject {
                ["hex"] = p.Hex, ["time"] = (p.Time - CacheT0).TotalSeconds, ["lat"] = p.Lat, ["lon"] = p.Lon,
                ["alt"] = p.Alt, ["track"] = p.Track, ["speed"] = p.Speed,
                ["old_time"] = p.OldTime == DateTime.MinValue ? (JToken)JValue.CreateNull() : (p.OldTime - CacheT0).TotalSeconds,
                ["old_lat"] = p.OldLat, ["old_lon"] = p.OldLon, ["old_alt"] = p.OldAlt, ["old_track"] = p.OldTrack, ["old_speed"] = p.OldSpeed,
            };
        }

        static JArray PlaneCacheCases()
        {
            var a = new JArray();
            foreach (var sc in CacheScenarios)
            {
                var reports = new JArray();
                foreach (var r in sc.reports)
                    reports.Add(new JObject { ["hex"] = r.hex, ["t"] = r.t, ["lat"] = r.lat, ["lon"] = r.lon, ["alt"] = r.alt, ["track"] = r.track, ["speed"] = r.speed });
                foreach (var q in sc.queries)
                {
                    foreach (var fn in new[] { "get", "get_all" })
                    {
                        // fresh cache per query: Get/GetAll do not modify it
                        var cache = new PlaneInfoCache();
                        var inserted = new JArray();
                        foreach (var r in sc.reports)
                            inserted.Add(cache.InsertOrUpdateIfNewer(new PlaneInfo(CacheT0.AddSeconds(r.t), "DLH1A", "D-AIZZ", r.hex, r.lat, r.lon, r.track, r.alt, r.speed, "A320", "Airbus", "A320", PLANECATEGORY.MEDIUM)));
                        var args = new JObject { ["scenario"] = sc.name, ["reports"] = reports, ["at"] = q, ["ttl"] = 5 };
                        if (fn == "get")
                            a.Add(Case(fn, args, () => new JObject { ["inserted"] = inserted, ["plane"] = PlaneJ(cache.Get(sc.reports[sc.reports.Length - 1].hex, CacheT0.AddSeconds(q), 5)) }));
                        else
                            a.Add(Case(fn, args, () => {
                                var l = new JArray();
                                foreach (var p in cache.GetAll(CacheT0.AddSeconds(q), 5)) l.Add(PlaneJ(p));
                                return new JObject { ["inserted"] = inserted, ["planes"] = l };
                            }));
                    }
                }
            }
            return a;
        }
    }
}
