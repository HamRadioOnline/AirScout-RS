// Golden cases for the aircraft side (stage 2).
using System;
using Newtonsoft.Json.Linq;
using AirScout.Aircrafts;
using AirScout.Core;

namespace GoldenHarness
{
    public static partial class Program
    {
        static JToken T(DateTime t) => t == DateTime.MinValue ? (JToken)JValue.CreateNull() : new JValue(t.ToString("yyyy-MM-ddTHH:mm:ss.fffffff"));
        static JToken IntOrNull(int v) => v == int.MinValue ? (JToken)JValue.CreateNull() : new JValue(v);
        static JToken DblOrNull(double v) => v == double.MinValue ? (JToken)JValue.CreateNull() : new JValue(v);

        static readonly string[] CheckerStrings = {
            "3c6444", "3C6444", "\"3C6444\"", " 3C6444 ", "3C644", "3C64444", "GGGGGG", "1000000", "FFFFFF", "000000", "-12345",
            "DLH1A", "dlh1a", "DLH123", "LH123", "LH1234", "BA12", "KLM", "KLM1", "XYZ123", "D-AIZZ", "[unknown]", "AB", "", " ", "1LH123", "D1H123", "\"BAW1\"", "B4W12",
            "D-AIZZ", "d-aizz", "N123AB", "N12", "NABC", "HB-JZA", "HBJZA", "PH-BXA", "G-EUPT", "X", "XX-ABC", "-ABC", "N", "N1", "2-ABCD", "DAIZZ",
            "A320", "a320", "A32", "B738", "A3-20", "A320 ", "\"A320\"", "C172",
            "EDDF-EGLL", "EDDF-EGL", "EDDFX-EGLL", "XEDDF-EGLL", "EDD-FEGLL", "EGLL-EDDF", "ABCD-EFGH",
            "X-ABC", "1-ABC", "XY-ABC",
            "7700", "7777", "7800", "0000", "770", "77000",
            "1700000000", "\"1700000000\"", "170000000", "2023-11-14 22:13:20", "2023-11-14T22:13:20Z", "14.11.2023 22:13:20", "garbage-text",
            "50.123", "-90.0", "90.5", "-180.0", "180.001", "50", "5,5", "1,000.5", "+50.5", ".5", "1.5E1", "1.", "abc.d",
            "0", "100000", "100001", "35000", "350", "359", "360", "800", "801", "12.5", " 42 "
        };

        static JArray PlaneCheckerCases()
        {
            var a = new JArray();
            var C = new PlaneInfoConverter();
            foreach (var s in CheckerStrings)
            {
                var args = new JObject { ["s"] = s };
                a.Add(Case("check_hex", args, () => PlaneInfoChecker.Check_Hex(s)));
                a.Add(Case("check_call", args, () => PlaneInfoChecker.Check_Call(s)));
                a.Add(Case("check_reg", args, () => PlaneInfoChecker.Check_Reg(s)));
                a.Add(Case("check_type", args, () => PlaneInfoChecker.Check_Type(s)));
                a.Add(Case("to_utc", args, () => T(C.To_UTC(s))));
                a.Add(Case("to_hex", args, () => S(C.To_Hex(s))));
                a.Add(Case("to_reg", args, () => S(C.To_Reg(s))));
                a.Add(Case("to_flight", args, () => S(C.To_Flight(s))));
                a.Add(Case("to_type", args, () => S(C.To_Type(s))));
                a.Add(Case("to_type_nocheck", args, () => S(C.To_Type(s, false))));
                a.Add(Case("to_radar", args, () => S(C.To_Radar(s))));
                a.Add(Case("to_route", args, () => S(C.To_Route(s))));
                a.Add(Case("to_call", args, () => S(C.To_Call(s))));
                a.Add(Case("to_call_nocheck", args, () => S(C.To_Call(s, false))));
                a.Add(Case("to_squawk", args, () => IntOrNull(C.To_Squawk(s))));
                a.Add(Case("to_lat", args, () => DblOrNull(C.To_Lat(s))));
                a.Add(Case("to_lon", args, () => DblOrNull(C.To_Lon(s))));
                a.Add(Case("to_alt", args, () => IntOrNull(C.To_Alt(s))));
                a.Add(Case("to_track", args, () => IntOrNull(C.To_Track(s))));
                a.Add(Case("to_speed", args, () => IntOrNull(C.To_Speed(s))));
            }
            foreach (var v in new[] { double.NaN, -0.0001, 0.0, 12.5, 89.9999, 90.0, 90.0001, 179.9, 180.0, 180.0001, 359.9, 360.0, 799.9, 800.0, 800.1, 5000.0, 99999.0, 100000.0, 100001.0, -90.0, -180.0, -180.1 })
            {
                var args = new JObject { ["v"] = v };
                a.Add(Case("check_lat", args, () => PlaneInfoChecker.Check_Lat(v)));
                a.Add(Case("check_lon", args, () => PlaneInfoChecker.Check_Lon(v)));
                a.Add(Case("check_alt", args, () => PlaneInfoChecker.Check_Alt(v)));
                a.Add(Case("check_track", args, () => PlaneInfoChecker.Check_Track(v)));
                a.Add(Case("check_speed", args, () => PlaneInfoChecker.Check_Speed(v)));
            }
            // Check(PlaneInfo): base plane, then one field varied at a time
            var t0 = new DateTime(2023, 11, 14, 22, 13, 20, DateTimeKind.Utc);
            var planes = new (string name, string hex, string call, bool mintime, double lat, double lon, double alt, double track, double speed)[] {
                ("ok", "3C6444", "DLH1A", false, 50, 8, 35000, 90, 450),
                ("bad_hex", "3C644", "DLH1A", false, 50, 8, 35000, 90, 450),
                ("short_call", "3C6444", "DLH", false, 50, 8, 35000, 90, 450),
                ("unknown_airline_ok_nocheck", "3C6444", "XYZ1", false, 50, 8, 35000, 90, 450),
                ("min_time", "3C6444", "DLH1A", true, 50, 8, 35000, 90, 450),
                ("lat_91", "3C6444", "DLH1A", false, 91, 8, 35000, 90, 450),
                ("lon_-181", "3C6444", "DLH1A", false, 50, -181, 35000, 90, 450),
                ("alt_neg", "3C6444", "DLH1A", false, 50, 8, -1, 90, 450),
                ("alt_100001", "3C6444", "DLH1A", false, 50, 8, 100001, 90, 450),
                ("track_360", "3C6444", "DLH1A", false, 50, 8, 35000, 360, 450),
                ("track_360_1", "3C6444", "DLH1A", false, 50, 8, 35000, 360.1, 450),
                ("speed_5000", "3C6444", "DLH1A", false, 50, 8, 35000, 90, 5000),
                ("speed_5001", "3C6444", "DLH1A", false, 50, 8, 35000, 90, 5001),
                ("lat_nan", "3C6444", "DLH1A", false, double.NaN, 8, 35000, 90, 450),
            };
            foreach (var p in planes)
            {
                var args = new JObject { ["name"] = p.name, ["hex"] = p.hex, ["call"] = p.call, ["min_time"] = p.mintime, ["lat"] = p.lat, ["lon"] = p.lon, ["alt"] = p.alt, ["track"] = p.track, ["speed"] = p.speed };
                var info = new PlaneInfo(p.mintime ? DateTime.MinValue : t0, p.call, "D-AIZZ", p.hex, p.lat, p.lon, p.track, p.alt, p.speed, "A320", "Airbus", "A320", PLANECATEGORY.MEDIUM);
                if (p.mintime) info.Time = DateTime.MinValue;
                a.Add(Case("check_plane", args, () => PlaneInfoChecker.Check(info)));
            }
            return a;
        }
    }
}
