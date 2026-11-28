// Stage 4d web server: the exact JSON text Newtonsoft.Json 13.0.3 writes for the web routes.
// Route settings: Webserver.cs:455-460 and SQLiteEntry.cs:190-199 (Utc dates, NaN/Infinity as
// strings, indented). The band and category lists use the default settings (Webserver.cs:484-498).
// Every value is synthetic.
using System;
using System.Collections.Generic;
using System.Data;
using AirScout.Core;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using ScoutBase.Core;

namespace GoldenHarness
{
    public static partial class Program
    {
        static JsonSerializerSettings RouteSettings() => new JsonSerializerSettings
        {
            DateTimeZoneHandling = DateTimeZoneHandling.Utc,
            FloatFormatHandling = FloatFormatHandling.String,
            Formatting = Formatting.Indented,
        };

        static readonly DateTime WebT0 = new DateTime(2026, 11, 24, 12, 0, 0, DateTimeKind.Utc);

        static JArray WebJsonCases()
        {
            var a = new JArray();

            // numbers: one List<double> per value, as every DTO double is written
            var numbers = new (string, double)[]
            {
                ("0", 0), ("1", 1), ("-0.0", -0.0), ("0.1", 0.1), ("1e-5", 1e-5), ("0.0001", 0.0001),
                ("1e14", 1e14), ("1e15", 1e15), ("123456789012345", 123456789012345), ("1234567890123456", 1234567890123456),
                ("MaxValue", double.MaxValue), ("MinValue", double.MinValue), ("5e-324", 5e-324),
                ("NaN", double.NaN), ("+Infinity", double.PositiveInfinity), ("-Infinity", double.NegativeInfinity),
                // extra cases that pin where the exponent form starts
                ("1e16", 1e16), ("1e17", 1e17), ("1.5e17", 1.5e17), ("1e18", 1e18), ("1.2345e-5", 1.2345e-5), ("-2.5e-7", -2.5e-7),
            };
            foreach (var (name, x) in numbers)
                a.Add(Case("number", new JObject { ["name"] = name, ["bits"] = BitConverter.DoubleToInt64Bits(x).ToString() },
                    () => JsonConvert.SerializeObject(new List<double> { x }, RouteSettings())));

            // dates: unix seconds plus a sub-second part in 100 ns ticks
            foreach (var ticks in new long[] { 0, 5_000_000, 1_234_567 })
            {
                var d = WebT0.AddTicks(ticks);
                a.Add(Case("date", new JObject { ["unix_s"] = 1795521600L, ["ticks"] = ticks },
                    () => JsonConvert.SerializeObject(d, RouteSettings())));
            }
            a.Add(Case("date_min", new JObject(), () => JsonConvert.SerializeObject(DateTime.MinValue, RouteSettings())));

            // PlaneInfo with the defaults of the parameterless constructor; only Time is fixed
            var p0 = new PlaneInfo { Time = WebT0 };
            a.Add(Case("plane_info_default", new JObject(), () => JsonConvert.SerializeObject(p0, RouteSettings())));

            // PlaneInfo with IntPoint and every member set
            var p1 = new PlaneInfo(WebT0, "TST123", "X-TEST", "ABC123", 50.25, 11.125, 87.5, 35000, 450, "A320", "Airbus", "A320-200", PLANECATEGORY.MEDIUM)
            {
                From = "XXAA",
                To = "XXBB",
                VSpeed = -640,
                IntPoint = new LatLon.GPoint(50.125, 11.0625),
                IntQRB = 123.456,
                AltDiff = -1500.5,
                Potential = 75,
                Eps1 = 0.25,
                Eps2 = -0.125,
                Theta1 = 1.5,
                Theta2 = 2.5,
                Angle = 33.3,
                Squint = 4.75,
                SignalStrength = -97.5,
                Ambiguous = true,
                Comment = "synthetic",
                OldTime = WebT0.AddSeconds(-30),
                OldLat = 50.2,
                OldLon = 11.1,
                OldAlt = 34900,
                OldTrack = 87,
                OldSpeed = 449,
            };
            a.Add(Case("plane_info_intpoint", new JObject { ["alt_m"] = p1.Alt_m, ["speed_kmh"] = p1.Speed_kmh },
                () => JsonConvert.SerializeObject(p1, RouteSettings())));

            // string escaping: quotes, backslash, control characters and the three characters
            // Newtonsoft escapes beyond JSON's minimum (U+0085, U+2028, U+2029)
            var text = "a\"b\\c/<>\u0001\b\t\n\f\r\u001f\u007f\u0085\u2028\u2029\u00e9\U0001F600";
            a.Add(Case("string", new JObject { ["text"] = text }, () => JsonConvert.SerializeObject(new List<string> { text }, RouteSettings())));

            // band and category lists, default settings (compact)
            a.Add(Case("band_values", new JObject(), () => JsonConvert.SerializeObject(Bands.GetBandsExceptNoneAndAll())));
            a.Add(Case("aircraft_categories", new JObject(), () => JsonConvert.SerializeObject(PlaneCategories.GetPlaneCategories())));

            // Path_Band_Settings: a DataTable with the BandSettings columns (BandSettings.cs:38-45)
            var t = new DataTable("BANDSETTINGS");
            t.Columns.Add("BAND");
            foreach (var c in new[] { "K-FACTOR", "F1-CLEARANCE", "GROUNDCLEARANCE", "MAXDISTANCE", "MAXSQUINT", "MAXELEVATION" })
                t.Columns.Add(c, typeof(double));
            t.Rows.Add("144M", 1.5, 0.2, 0.0, 10.0, double.MaxValue, double.MaxValue);
            t.Rows.Add("1.2G", 1.33, 0.6, 2.5, 100.0, 45.0, 90.0);
            a.Add(Case("band_settings", new JObject(), () => JsonConvert.SerializeObject(t, RouteSettings())));

            return a;
        }
    }
}
