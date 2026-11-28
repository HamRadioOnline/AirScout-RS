// Golden-fixture harness entry point. Usage: dotnet run -- <output-dir>
using System;
using System.Collections.Generic;
using System.IO;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using ScoutBase.Core;

namespace GoldenHarness
{
    public static partial class Program
    {
        public const string UpstreamCommit = "91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5";

        public static int Main(string[] args)
        {
            string outdir = args.Length > 0 ? args[0] : ".";
            if (Environment.GetEnvironmentVariable("HARNESS_DEBUG") == "1") { DebugMain(outdir); return 0; }
            Directory.CreateDirectory(outdir);
            Write(outdir, "latlon.json", LatLonCases());
            Write(outdir, "maidenhead.json", MaidenheadCases());
            Write(outdir, "callsign.json", CallsignCases());
            Write(outdir, "propagation_formulas.json", FormulaCases());
            Write(outdir, "elevation_tile.json", ElevationTileCases(FixtureDir(outdir)));
            Write(outdir, "elevation_path.json", ElevationPathCases(FixtureDir(outdir)));
            Write(outdir, "propagation_path.json", PropagationPathCases(FixtureDir(outdir)));
            Write(outdir, "propagation_horizon.json", PropagationHorizonCases(FixtureDir(outdir)));
            Write(outdir, "plane_checker.json", PlaneCheckerCases());
            Write(outdir, "plane_cache.json", PlaneCacheCases());
            Write(outdir, "nearest_planes.json", NearestPlanesCases());
            Write(outdir, "adsb_msgs.json", AdsbCases());
            Write(outdir, "adsb_decoder.json", AdsbDecoderCases());
            Write(outdir, "feed_json.json", FeedJsonCases(outdir));
            Write(outdir, "bincraft.json", BinCraftCases());
            Write(outdir, "web_json.json", WebJsonCases());
            Write(outdir, "elevation_tilegen.json", TileGenCases());
            MapsCases(outdir);
            return 0;
        }

        static void Write(string dir, string name, JArray cases)
        {
            var root = new JObject { ["upstream_commit"] = UpstreamCommit, ["cases"] = cases };
            var settings = new JsonSerializerSettings { FloatFormatHandling = FloatFormatHandling.String, Formatting = Formatting.Indented };
            File.WriteAllText(Path.Combine(dir, name), JsonConvert.SerializeObject(root, settings) + "\n");
            Console.WriteLine(name + ": " + cases.Count + " cases");
        }

        static JObject Case(string fn, JObject args, Func<JToken> eval)
        {
            JToken result;
            try { result = eval(); }
            catch (Exception ex) { result = new JObject { ["error"] = ex.GetType().Name }; }
            return new JObject { ["fn"] = fn, ["args"] = args, ["result"] = result ?? JValue.CreateNull() };
        }

        static JToken P(LatLon.GPoint p) => p == null ? (JToken)JValue.CreateNull() : new JObject { ["lat"] = p.Lat, ["lon"] = p.Lon };
        static JToken R(LatLon.GRect r) => new JObject { ["min_lat"] = r.MinLat, ["min_lon"] = r.MinLon, ["max_lat"] = r.MaxLat, ["max_lon"] = r.MaxLon };
        static JToken S(string s) => s == null ? (JToken)JValue.CreateNull() : new JValue(s);

        static string FixtureDir(string goldenDir) => Path.Combine(goldenDir, "..", "fixtures", "elevation");

        /// Loads every .loc tile of the fixture set: GLOBE square zips + loose .loc files.
        public static List<(string model, string name, string json)> FixtureTiles(string dir)
        {
            var list = new List<(string, string, string)>();
            foreach (var model in new[] { "GLOBE", "SRTM3" })
            {
                string mdir = Path.Combine(dir, model);
                if (!Directory.Exists(mdir)) continue;
                foreach (var f in Directory.GetFiles(mdir, "*.*", SearchOption.AllDirectories))
                {
                    if (f.EndsWith(".zip"))
                        using (var z = System.IO.Compression.ZipFile.OpenRead(f))
                            foreach (var e in z.Entries)
                                using (var r = new StreamReader(e.Open()))
                                    list.Add((model, e.Name, r.ReadToEnd()));
                    else if (f.EndsWith(".loc"))
                        list.Add((model, Path.GetFileName(f), File.ReadAllText(f)));
                }
            }
            list.Sort((a, b) => string.CompareOrdinal(a.Item1 + a.Item2, b.Item1 + b.Item2));
            return list;
        }

        static JArray ElevationTileCases(string dir)
        {
            var a = new JArray();
            var tiles = FixtureTiles(dir);
            var rnd = new Random(4711);
            int n = 0;
            foreach (var (model, name, json) in tiles)
            {
                // GLOBE: every 13th tile to keep the fixture small; SRTM3: all
                if (model == "GLOBE" && (n++ % 13) != 0) continue;
                var t = JsonConvert.DeserializeObject<UpstreamTile>(json);
                double h = 1 / 24.0, w = 2 / 24.0;
                var pts = new List<(double, double)>();
                for (int k = 0; k <= 6; k++)
                    for (int m = 0; m <= 6; m++)
                        pts.Add((t.BaseLat + k / 6.0 * h, t.BaseLon + m / 6.0 * w));
                for (int r = 0; r < 20; r++)
                    pts.Add((t.BaseLat + rnd.NextDouble() * h, t.BaseLon + rnd.NextDouble() * w));
                pts.Add((t.BaseLat - 1e-12, t.BaseLon));     // just below the tile
                pts.Add((t.BaseLat - h * 0.5, t.BaseLon));   // clearly outside
                foreach (var (la, lo) in pts)
                    a.Add(Case("get_elevation", new JObject { ["model"] = model, ["tile"] = t.TileIndex, ["lat"] = la, ["lon"] = lo }, () => (int)t.GetElevation(la, lo)));
            }
            return a;
        }

        /// Fixture GLOBE tiles registered under GLOBE and (for the averaging code path) ASTER3.
        public static UpstreamElevationDb FixtureDb(string dir, bool complete)
        {
            var db = new UpstreamElevationDb { Complete = complete, MinLat = 50, MinLon = 10, MaxLat = 52, MaxLon = 14 };
            foreach (var (model, name, json) in FixtureTiles(dir))
            {
                if (model != "GLOBE") continue;
                var t = JsonConvert.DeserializeObject<UpstreamTile>(json);
                foreach (var m in new[] { "GLOBE", "ASTER3" })
                {
                    if (!db.Tiles.ContainsKey(m)) db.Tiles[m] = new Dictionary<string, UpstreamTile>();
                    db.Tiles[m][t.TileIndex] = t;
                }
            }
            return db;
        }

        // Start points must not lie exactly on a tile edge: DestinationPoint(d = 0) differs by 1 ulp
        // between libm implementations, which flips the tile chosen by the inclusive last-tile check
        // (logic-findings.md #10) and makes the result platform dependent even for the C# code.
        public static readonly double[][] PathPairs = new double[][]
        {
            new double[] { 50.0208333333, 10.0416666667, 51.2291666667, 13.7083333333 }, // JO50AA -> JO61UF (in fixture area)
            new double[] { 50.5013, 10.5027, 51.8, 13.9 },       // start off the tile corner (see below)
            new double[] { 51.9, 10.1, 50.1, 13.9 },
            new double[] { 50.0013, 10.0027, 50.0013, 10.0027 },  // zero distance
            new double[] { 51.0, 11.0, 51.0, 11.0005 },           // shorter than one step
            new double[] { 51.5, 13.5, 51.5, 15.5 },              // leaves fixture area -> missing tiles
            new double[] { 51.9987, 11.0027, 50.0013, 11.0027 },  // due south, starts just below the top edge
        };

        static JArray ElevationPathCases(string dir)
        {
            var a = new JArray();
            foreach (var complete in new[] { true, false })
                foreach (var model in new[] { "GLOBE", "ASTER3" })
                    foreach (var step in new[] { 1000.0, 90.0 })
                    {
                        // fresh db per (complete, model, step) group: cache state is part of the behaviour
                        var db = FixtureDb(dir, complete);
                        foreach (var p in PathPairs)
                        {
                            var args = new JObject { ["model"] = model, ["complete"] = complete, ["step"] = step, ["lat1"] = p[0], ["lon1"] = p[1], ["lat2"] = p[2], ["lon2"] = p[3] };
                            a.Add(Case("create_path", args, () =>
                            {
                                var ep = db.ElevationPathCreateFromLatLon(null, p[0], p[1], p[2], p[3], step, model);
                                return new JObject { ["count"] = ep.Count, ["valid"] = ep.Valid, ["path"] = ep.Path == null ? null : new JArray(ep.Path) };
                            }));
                        }
                    }
            return a;
        }

        static void DebugMain(string outdir)
        {
            var r = MaidenheadLocator.BoundsFromLoc("JO50FL");
            Console.WriteLine("FL bounds " + r.MinLat.ToString("R") + " " + r.MinLon.ToString("R") + " " + r.MaxLat.ToString("R") + " " + r.MaxLon.ToString("R"));
            double b = LatLon.Bearing(50.5, 10.5, 51.8, 13.9);
            foreach (var d in new[] { -2.0, -1.0, 0.0 })
            {
                var p = LatLon.DestinationPoint(50.5, 10.5, b, d);
                Console.WriteLine("d=" + d + " " + p.Lat.ToString("R") + " " + p.Lon.ToString("R"));
            }
        }

        static JObject PP(UpstreamPropagationPoint p) => new JObject { ["lat"] = p.Lat, ["lon"] = p.Lon, ["h1"] = p.H1, ["h2"] = p.H2, ["f1"] = p.F1 };

        static JArray PropagationPathCases(string dir)
        {
            var a = new JArray();
            var db = new UpstreamPropagationDb { Elevation = FixtureDb(dir, true) };
            double radius = 6371 * 1.33;
            var hs = new[] { new[] { 10.0, 10.0 }, new[] { 50.0, 10.0 } };
            int pi = 0;
            foreach (var p in PathPairs)
            {
                pi++;
                foreach (var step in new[] { 1000.0, 90.0 })
                    foreach (var h in hs)
                        foreach (var qrg in new[] { 0.144, 1.296, 10.368 })
                            foreach (var f1 in new[] { 0.0, 0.6 })
                                foreach (var nf in new[] { 0.0, 100.0, 500.0 })
                                    foreach (var lo in new[] { double.MinValue, 0.01 })
                                    {
                                        if (step == 90.0 && (qrg != 0.144 || nf != 100.0)) continue; // keep the fixture small
                                        db.NearFieldSuppression = nf;
                                        var args = new JObject { ["lat1"] = p[0], ["lon1"] = p[1], ["h1"] = h[0], ["lat2"] = p[2], ["lon2"] = p[3], ["h2"] = h[1], ["qrg"] = qrg, ["radius"] = radius, ["f1"] = f1, ["step"] = step, ["nf"] = nf, ["lo"] = lo };
                                        UpstreamPropagationPath pp = null;
                                        a.Add(Case("create_path", args, () =>
                                        {
                                            pp = db.PropagationPathCreateFromLatLon(null, p[0], p[1], h[0], p[2], p[3], h[1], qrg, radius, f1, step, "GLOBE", lo);
                                            return new JObject { ["eps1_min"] = pp.Eps1_Min, ["eps2_min"] = pp.Eps2_Min, ["valid"] = pp.Valid, ["local_obstructed"] = pp.LocalObstructed };
                                        }));
                                        bool detail = step == 1000.0 && h[0] == 10.0 && f1 == 0.6 && nf == 100.0;
                                        if (!detail || pp == null) continue;
                                        a.Add(Case("info_points", args, () => { var ip = pp.GetInfoPoints(); var arr = new JArray(); foreach (var x in ip) arr.Add(PP(x)); return arr; }));
                                        a.Add(Case("mid_point", args, () => P(pp.GetMidPoint())));
                                        // plane probes around the path
                                        var mid = pp.GetMidPoint();
                                        foreach (var off in new[] { new[] { 0.0, 0.0 }, new[] { 0.3, -0.4 }, new[] { -0.5, 0.6 }, new[] { 1.5, 1.5 } })
                                            foreach (var brg in new[] { 0.0, 90.0, 200.0 })
                                                foreach (var md in new[] { -1.0, 0.0, 50.0 })
                                                {
                                                    double plat = mid.Lat + off[0], plon = mid.Lon + off[1];
                                                    var ia = (JObject)args.DeepClone();
                                                    ia["plane_lat"] = plat; ia["plane_lon"] = plon; ia["plane_brg"] = brg; ia["maxdistance"] = md;
                                                    a.Add(Case("intersection_point", ia, () =>
                                                    {
                                                        var ip = pp.GetIntersectionPoint(plat, plon, brg, md);
                                                        if (ip == null) return JValue.CreateNull();
                                                        return new JObject { ["lat"] = ip.Lat, ["lon"] = ip.Lon, ["qrb"] = ip.QRB, ["min_h"] = ip.Min_H, ["dist1"] = ip.Dist1, ["dist2"] = ip.Dist2 };
                                                    }));
                                                }
                                    }
            }
            return a;
        }

        static JObject HP(UpstreamHorizonPoint p) => p == null ? null : new JObject { ["dist"] = p.Dist, ["eps_min"] = p.Epsmin, ["elv"] = p.Elv };

        static JArray PropagationHorizonCases(string dir)
        {
            var a = new JArray();
            var db = new UpstreamPropagationDb { Elevation = FixtureDb(dir, true) };
            double radius = 6371 * 1.33;
            var stations = new[] { new[] { 51.0013, 11.5027 }, new[] { 50.6021, 12.9034 } };
            foreach (var s in stations)
                foreach (var dist in new[] { 50.0, 200.0 })
                    foreach (var qrg in new[] { 0.144, 10.368 })
                        foreach (var nf in new[] { 0.0, 500.0 })
                        {
                            db.NearFieldSuppression = nf;
                            var args = new JObject { ["lat"] = s[0], ["lon"] = s[1], ["h"] = 10.0, ["dist"] = dist, ["qrg"] = qrg, ["radius"] = radius, ["f1"] = 0.6, ["step"] = 1000.0, ["nf"] = nf };
                            a.Add(Case("horizon", args, () =>
                            {
                                bool valid;
                                var hor = UpstreamHorizon.PropagationHorizonCreate(db, s[0], s[1], 10.0, dist, qrg, radius, 0.6, 1000.0, "GLOBE", out valid);
                                var arr = new JArray();
                                foreach (var x in hor) arr.Add(HP(x));
                                return new JObject { ["valid"] = valid, ["points"] = arr };
                            }));
                        }
            // EpsilonMaxFromPath on real elevation paths, with and without the max elevation cut
            foreach (var p in PathPairs)
                foreach (var maxelv in new[] { 0.0, 1000.0, 12000.0 })
                    foreach (var qrg in new[] { 0.144, 10.368 })
                    {
                        var ep = db.Elevation.ElevationPathCreateFromLatLon(null, p[0], p[1], p[2], p[3], 1000.0, "GLOBE");
                        double dist = LatLon.Distance(p[0], p[1], p[2], p[3]);
                        var elvIn = new JArray(); foreach (var e in ep.Path) elvIn.Add(e);
                        var args = new JObject { ["h1"] = 10.0, ["elv"] = elvIn, ["dist"] = dist, ["step"] = 1000.0, ["qrg"] = qrg, ["f1"] = 0.6, ["maxelv"] = maxelv, ["radius"] = radius };
                        a.Add(Case("epsilon_max_from_path", args, () =>
                        {
                            short[] elv = (short[])ep.Path.Clone();
                            var hp = UpstreamHorizon.EpsilonMaxFromPath(10.0, ref elv, dist, 1000.0, qrg, 0.6, maxelv, radius);
                            var o = HP(hp);
                            o["len"] = elv.Length;
                            return o;
                        }));
                    }
            return a;
        }

        static JArray LatLonCases()
        {
            var a = new JArray();
            foreach (var p in Cases.Pairs)
            {
                var args = new JObject { ["lat1"] = p[0], ["lon1"] = p[1], ["lat2"] = p[2], ["lon2"] = p[3] };
                a.Add(Case("distance", args, () => LatLon.Distance(p[0], p[1], p[2], p[3])));
                a.Add(Case("bearing", args, () => LatLon.Bearing(p[0], p[1], p[2], p[3])));
                a.Add(Case("midpoint", args, () => P(LatLon.MidPoint(p[0], p[1], p[2], p[3]))));
                foreach (var b1 in Cases.Bearings)
                    foreach (var b2 in Cases.Bearings)
                    {
                        var ia = new JObject { ["lat1"] = p[0], ["lon1"] = p[1], ["brg1"] = b1, ["lat2"] = p[2], ["lon2"] = p[3], ["brg2"] = b2 };
                        a.Add(Case("intersection_point", ia, () => P(LatLon.IntersectionPoint(p[0], p[1], b1, p[2], p[3], b2))));
                    }
                foreach (var b in Cases.Bearings)
                    foreach (var d in Cases.Distances)
                    {
                        var da = new JObject { ["lat"] = p[0], ["lon"] = p[1], ["bearing"] = b, ["distance"] = d };
                        a.Add(Case("destination_point", da, () => P(LatLon.DestinationPoint(p[0], p[1], b, d))));
                    }
            }
            foreach (var l1 in new[] { "JO50AA", "JO61UF", "IO91WM" })
                foreach (var l2 in new[] { "JO50AA", "JN58TD", "KP20LE" })
                {
                    var la = new JObject { ["loc1"] = l1, ["loc2"] = l2 };
                    a.Add(Case("distance_loc", la, () => LatLon.Distance(l1, l2)));
                    a.Add(Case("bearing_loc", la, () => LatLon.Bearing(l1, l2)));
                }
            foreach (var loc in new[] { "JO50AA", "jo50aa", "JO50" })
                foreach (var p in Cases.Points)
                {
                    var la = new JObject { ["loc"] = loc, ["lat"] = p[0], ["lon"] = p[1] };
                    a.Add(Case("is_precise_loc", la, () => LatLon.IsPrecise(loc, p[0], p[1])));
                }
            return a;
        }

        static JArray CallsignCases()
        {
            var a = new JArray();
            var calls = new[] {
                "DL2ALF", "DL2ALF/P", "PA/DL2ALF/M", "PA/DL2ALF", "DL2ALF/MM", "DL2ALF/QRP", "dl2alf", " DL2ALF ", "",
                "D", "DL", "DL2", "9A5O", "G7RAU", "T91CO", "T91", "2E0ABO", "M0ABC", "EA8/DL2ALF", "DL/AB1",
                "/AB", "AB/", "A/B/C/D", "DL2-ALF", "DLÄALF", "OH2BH", "HB9/DL2ALF/P", "HB0ABC", "YU1AA", "YU2AA",
                "UA3ABC", "RA2ABC", "SV9ABC", "EH8ABC", "TM1A", "I0ABC", "IZ1ABC", "IS0ABC", "4N1A", "XX1ABC",
                "LX/DL2ALF", "OE/DL2ALF/P", "DL2ALF/OE", "5B4AAA", "P33W", "C31X", "K1ABC", "W1AW", "VE3ABC", "JA1ABC",
                "S5/9A/DL1A", "123", "A1B", "1AB", "AB1", "DL0GTH", "HV0A", "OT4A", "ZB4/G4ABC", "AM70A" };
            foreach (var c in calls)
            {
                var args = new JObject { ["call"] = c };
                a.Add(Case("check", args, () => Callsign.Check(c)));
                a.Add(Case("cut", args, () => Callsign.Cut(c)));
                a.Add(Case("prefix", args, () => Callsign.Prefix(c)));
            }
            return a;
        }

        static JArray MaidenheadCases()
        {
            var a = new JArray();
            foreach (var loc in Cases.Locators)
            {
                var la = new JObject { ["loc"] = loc };
                a.Add(Case("check", la, () => MaidenheadLocator.Check(loc)));
                foreach (var small in new[] { false, true })
                    a.Add(Case("convert", new JObject { ["loc"] = loc, ["small"] = small }, () => S(MaidenheadLocator.Convert(loc, small))));
                foreach (PositionInRectangle pos in Enum.GetValues(typeof(PositionInRectangle)))
                {
                    var pa = new JObject { ["loc"] = loc, ["pos"] = pos.ToString() };
                    a.Add(Case("lat_from_loc", pa, () => MaidenheadLocator.LatFromLoc(loc, pos)));
                    a.Add(Case("lon_from_loc", pa, () => MaidenheadLocator.LonFromLoc(loc, pos)));
                }
                a.Add(Case("bounds_from_loc", la, () => R(MaidenheadLocator.BoundsFromLoc(loc))));
                if (loc.Length <= 6)
                    foreach (MaidenheadLocator.SubGridsFilter f in Enum.GetValues(typeof(MaidenheadLocator.SubGridsFilter)))
                        foreach (var small in new[] { false, true })
                            foreach (var rev in new[] { false, true })
                                a.Add(Case("get_sub_grids", new JObject { ["loc"] = loc, ["filter"] = f.ToString(), ["small"] = small, ["reverse"] = rev },
                                    () => new JArray(MaidenheadLocator.GetSubGrids(loc, f, small, rev).ToArray())));
            }
            foreach (var p in Cases.Points)
            {
                for (int prec = 0; prec <= 10; prec++)
                {
                    int pr = prec;
                    a.Add(Case("is_precise", new JObject { ["lat"] = p[0], ["lon"] = p[1], ["precision"] = pr }, () => MaidenheadLocator.IsPrecise(p[0], p[1], pr)));
                    foreach (var small in new[] { false, true })
                        foreach (var auto in new[] { false, true })
                            a.Add(Case("loc_from_lat_lon", new JObject { ["lat"] = p[0], ["lon"] = p[1], ["small"] = small, ["precision"] = pr, ["autolength"] = auto },
                                () => S(MaidenheadLocator.LocFromLatLon(p[0], p[1], small, pr, auto))));
                }
            }
            // dense sweep of locator round trip
            for (double lat = -89.95; lat < 90; lat += 7.3)
                for (double lon = -179.95; lon < 180; lon += 11.7)
                {
                    double la = lat, lo = lon;
                    a.Add(Case("loc_from_lat_lon", new JObject { ["lat"] = la, ["lon"] = lo, ["small"] = false, ["precision"] = 3, ["autolength"] = false },
                        () => S(MaidenheadLocator.LocFromLatLon(la, lo, false, 3, false))));
                }
            return a;
        }

        static JArray FormulaCases()
        {
            var a = new JArray();
            foreach (var re in Cases.Radii)
                foreach (var d in Cases.FormulaDists)
                {
                    a.Add(Case("alpha_from_distance", new JObject { ["dist"] = d, ["re"] = re }, () => Propagation.AlphaFromDistance(d, re)));
                    foreach (var h in Cases.Hs)
                    {
                        foreach (var H in Cases.BigHs)
                        {
                            var ha = new JObject { ["h"] = h, ["dist"] = d, ["H"] = H, ["radius"] = re };
                            a.Add(Case("slant_range_from_heights", ha, () => Propagation.SlantRangeFromHeights(h, d, H, re)));
                            a.Add(Case("epsilon_from_heights", ha, () => Propagation.EpsilonFromHeights(h, d, H, re)));
                            a.Add(Case("theta_from_heights", ha, () => Propagation.ThetaFromHeights(h, d, H, re)));
                        }
                        foreach (var eps in Cases.Epsilons)
                            a.Add(Case("height_from_epsilon", new JObject { ["h"] = h, ["dist"] = d, ["eps"] = eps, ["radius"] = re }, () => Propagation.HeightFromEpsilon(h, d, eps, re)));
                    }
                }
            foreach (var re in Cases.Radii)
                foreach (var h in Cases.Hs)
                    foreach (var H in Cases.BigHs)
                    {
                        foreach (var eps in Cases.Epsilons)
                            a.Add(Case("distance_from_epsilon", new JObject { ["h"] = h, ["H"] = H, ["eps"] = eps, ["radius"] = re }, () => Propagation.DistanceFromEpsilon(h, H, eps, re)));
                        a.Add(Case("visibility_distance", new JObject { ["h1"] = h, ["h2"] = H, ["re"] = re }, () => Propagation.VisibilityDistance(h, H, re)));
                    }
            foreach (var p in Cases.Pairs)
                a.Add(Case("slant_range_from_heights_latlon", new JObject { ["h"] = 10.0, ["lat1"] = p[0], ["lon1"] = p[1], ["lat2"] = p[2], ["lon2"] = p[3], ["H"] = 10000.0, ["radius"] = 6371 * 1.33 },
                    () => Propagation.SlantRangeFromHeights(10.0, p[0], p[1], p[2], p[3], 10000.0, 6371 * 1.33)));
            foreach (var q in Cases.Qrgs)
                foreach (var d1 in Cases.FormulaDists)
                    foreach (var d2 in Cases.FormulaDists)
                        a.Add(Case("f1_radius", new JObject { ["qrg"] = q, ["dist1"] = d1, ["dist2"] = d2 }, () => Propagation.F1Radius(q, d1, d2)));
            foreach (var q in new[] { 144.3e6, 10368.1e6 })
                foreach (var v in new[] { -300.0, -50.0, 0.0, 50.0, 300.0 })
                    a.Add(Case("doppler_shift", new JObject { ["qrg"] = q, ["speed"] = v }, () => Propagation.DopplerShift(q, v)));
            return a;
        }
    }
}
