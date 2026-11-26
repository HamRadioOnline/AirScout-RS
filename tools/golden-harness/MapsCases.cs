// Stage 5c: reference vectors for the GMap provider DbIds and the map preloader's tile ranges.
// Written to maps.json directly (not through Program.Write): { upstream_commit, db_ids, ranges }.
using System;
using System.IO;
using System.Security.Cryptography;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace GoldenHarness
{
    public static partial class Program
    {
        // GMapProvider.cs:232-236, verbatim formula: Math.Abs(BitConverter.ToInt32(SHA1(Id.ToByteArray()), 0)).
        static int MapsDbId(Guid id)
        {
            using (var sha1 = SHA1.Create())
                return Math.Abs(BitConverter.ToInt32(sha1.ComputeHash(id.ToByteArray()), 0));
        }

        // MapPreloader.cs:57-65, verbatim except that the raw double is returned (before the int cast).
        static double MapsLong2TileXRaw(double lon, int z)
        {
            return Math.Floor((lon + 180.0) / 360.0 * (1 << z));
        }

        static double MapsLat2TileYRaw(double lat, int z)
        {
            return Math.Floor((1 - Math.Log(Math.Tan((lat / 180.0 * Math.PI)) + 1 / Math.Cos((lat / 180.0 * Math.PI))) / Math.PI) / 2 * (1 << z));
        }

        // The C# (int) cast of +-Infinity/NaN is architecture dependent (x86-64: int.MinValue, arm64: saturating/0).
        // The golden pins the x86-64 result explicitly; the raw double is recorded next to it.
        static int MapsCast(double v) => double.IsNaN(v) || double.IsInfinity(v) ? int.MinValue : (int)v;

        static JToken MapsRaw(double v) =>
            double.IsNaN(v) ? new JValue("NaN") :
            double.IsPositiveInfinity(v) ? new JValue("Infinity") :
            double.IsNegativeInfinity(v) ? new JValue("-Infinity") : new JValue(v);

        static void MapsCases(string dir)
        {
            var dbIds = new JArray();
            foreach (var (name, guid) in new[] {
                ("OpenStreetMap", "0521335C-92EC-47A8-98A5-6FD333DDA9C0"),
                ("OpenTopoMap", "B8A6A7B4-3034-495D-BC83-96F6034748B1"),
                ("OCM Landscape", "BDBAA939-6597-4D87-8F4F-261C49E35F56") })
                dbIds.Add(new JObject { ["name"] = name, ["guid"] = guid, ["db_id"] = MapsDbId(new Guid(guid)) });

            var ranges = new JArray();
            // (min_lat, max_lat, min_lon, max_lon); the first is the default area.
            foreach (var a in new[] {
                new double[] { 35, 60, -15, 30 }, new double[] { 50, 51, 10, 11 }, new double[] { -10, 10, 170, 180 },
                new double[] { 80, 90, -180, 180 }, new double[] { -90, -80, -10, 10 } })
            {
                for (int z = 0; z <= 11; z++)
                {
                    // MapPreloader.cs:148-156
                    double rxmin = MapsLong2TileXRaw(a[2], z), rxmax = MapsLong2TileXRaw(a[3], z);
                    double rymin = MapsLat2TileYRaw(a[1], z), rymax = MapsLat2TileYRaw(a[0], z);
                    int xmin = MapsCast(rxmin), xmax = MapsCast(rxmax), ymin = MapsCast(rymin), ymax = MapsCast(rymax);
                    long total = (xmax >= xmin && ymax >= ymin) ? (long)(xmax - xmin + 1) * (ymax - ymin + 1) : 0;
                    ranges.Add(new JObject
                    {
                        ["area"] = new JArray(a[0], a[1], a[2], a[3]),
                        ["z"] = z,
                        ["xmin"] = xmin, ["xmax"] = xmax, ["ymin"] = ymin, ["ymax"] = ymax,
                        ["raw"] = new JObject { ["xmin"] = MapsRaw(rxmin), ["xmax"] = MapsRaw(rxmax), ["ymin"] = MapsRaw(rymin), ["ymax"] = MapsRaw(rymax) },
                        ["total"] = total,
                    });
                }
            }

            var root = new JObject { ["upstream_commit"] = UpstreamCommit, ["db_ids"] = dbIds, ["ranges"] = ranges };
            var settings = new JsonSerializerSettings { FloatFormatHandling = FloatFormatHandling.String, Formatting = Formatting.Indented };
            File.WriteAllText(Path.Combine(dir, "maps.json"), JsonConvert.SerializeObject(root, settings) + "\n");
            Console.WriteLine("maps.json: " + dbIds.Count + " db_ids, " + ranges.Count + " ranges");
        }
    }
}
