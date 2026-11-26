// Golden cases for the crossing prediction (AircraftDatabase.GetNearestPlanes).
using System;
using System.Collections.Generic;
using Newtonsoft.Json.Linq;
using ScoutBase.Core;
using AirScout.Core;

namespace GoldenHarness
{
    public static partial class Program
    {
        // lat1, lon1, h1, lat2, lon2, h2 [m asl], eps1_min, eps2_min, local obstruction [rad]
        static readonly double[][] CrossingPaths = {
            new[] { 50.0, 8.0, 300.0, 51.0, 11.0, 200.0, 0.002, 0.003, double.MinValue },
            new[] { 48.0, 2.0, 150.0, 52.0, 12.0, 400.0, 0.001, 0.001, double.MinValue },
            new[] { 50.0, 8.0, 300.0, 51.0, 11.0, 200.0, 0.002, 0.003, 0.01 },
        };

        static UpstreamPropagationPath CrossingPath(double[] p) =>
            new UpstreamPropagationPath(p[0], p[1], p[2], p[3], p[4], p[5], 0.144, 6371 * 1.33, 0.6, 1000, p[6], p[7], p[8]);

        static readonly DateTime CrossingAt = new DateTime(2023, 11, 14, 22, 0, 0, DateTimeKind.Utc);

        /// Planes placed relative to the path: (name, offset along path from midpoint [km, + towards 2],
        /// perpendicular offset [km, + right of bearing12], track relative to bearing12, alt [ft], speed [kts]).
        static readonly (string name, double along, double across, double relTrack, double alt, double speed)[] CrossingPlanes = {
            ("cross_near_high", 0, 2, -90, 40000, 450),
            ("cross_near_mid", 0, 5, -90, 25000, 450),
            ("cross_near_low", 0, 8, -90, 8000, 450),
            ("cross_near_verylow", 0, 3, -90, 2000, 450),
            ("cross_far_high", 0, 40, -90, 40000, 450),
            ("cross_far_low", 0, 40, -90, 5000, 450),
            ("cross_far_left", 0, -40, 90, 36000, 450),
            ("away_far", 0, 40, 90, 36000, 450),
            ("parallel_near", 0, 5, 0, 36000, 450),
            ("parallel_far", 0, 30, 180, 36000, 450),
            // a plane exactly on the path great circle is ill-conditioned (every intersection is
            // degenerate, results depend on the last ulp of sin/cos); 0.5 km off at 3° keeps the case
            ("near_path_shallow", 0, 0.5, -3, 36000, 450),
            ("diag_towards", 20, 25, -135, 33000, 420),
            ("diag_near_end1", -50, 6, -60, 30000, 400),
            ("outside_radius", 0, 600, -90, 36000, 450),
            ("beyond_station2", 200, 5, -90, 36000, 450),
            ("slow", 10, 15, -90, 30000, 120),
        };

        static PlaneInfo MakeCrossingPlane(UpstreamPropagationPath pp, (string name, double along, double across, double relTrack, double alt, double speed) d, int idx)
        {
            var mid = pp.GetMidPoint();
            var a = LatLon.DestinationPoint(mid.Lat, mid.Lon, pp.Bearing12 + (d.along >= 0 ? 0 : 180), Math.Abs(d.along));
            var b = LatLon.DestinationPoint(a.Lat, a.Lon, pp.Bearing12 + (d.across >= 0 ? 90 : -90), Math.Abs(d.across));
            double track = pp.Bearing12 + d.relTrack;
            while (track < 0) track += 360;
            while (track >= 360) track -= 360;
            return new PlaneInfo(CrossingAt.AddSeconds(-30), "DLH" + idx, "D-AI" + idx, (0x3C6400 + idx).ToString("X6"), b.Lat, b.Lon, track, d.alt, d.speed, "A320", "Airbus", "A320", PLANECATEGORY.MEDIUM);
        }

        static JObject PlaneInJ(PlaneInfo p) => new JObject {
            ["hex"] = p.Hex, ["call"] = p.Call, ["time"] = (p.Time - CrossingAt).TotalSeconds, ["lat"] = p.Lat, ["lon"] = p.Lon,
            ["alt"] = p.Alt, ["track"] = p.Track, ["speed"] = p.Speed };

        static JObject PlaneOutJ(PlaneInfo p) => new JObject {
            ["hex"] = p.Hex, ["time"] = (p.Time - CrossingAt).TotalSeconds, ["lat"] = p.Lat, ["lon"] = p.Lon,
            ["int_point"] = P(p.IntPoint), ["int_qrb"] = p.IntQRB, ["alt_diff"] = p.AltDiff, ["potential"] = p.Potential,
            ["eps1"] = p.Eps1, ["eps2"] = p.Eps2, ["theta1"] = p.Theta1, ["theta2"] = p.Theta2, ["angle"] = p.Angle, ["squint"] = p.Squint };

        static JArray NearestPlanesCases()
        {
            var a = new JArray();
            double maxdist = 10, maxalt = 12200, maxdelta = 1000;
            for (int pi = 0; pi < CrossingPaths.Length; pi++)
                foreach (var maxradius in new[] { -1.0, 0.0, 50.0 })
                {
                    var pp = CrossingPath(CrossingPaths[pi]);
                    var planes = new List<PlaneInfo>();
                    for (int i = 0; i < CrossingPlanes.Length; i++)
                        planes.Add(MakeCrossingPlane(pp, CrossingPlanes[i], i));
                    var input = new JArray();
                    foreach (var p in planes) input.Add(PlaneInJ(p));
                    var args = new JObject { ["path"] = new JArray(CrossingPaths[pi]), ["planes"] = input, ["max_radius"] = maxradius, ["max_dist"] = maxdist, ["max_alt"] = maxalt, ["max_delta"] = maxdelta };
                    a.Add(Case("nearest", args, () => {
                        var res = UpstreamNearestPlanes.GetNearestPlanes(CrossingAt, pp, planes, maxradius, maxdist, maxalt, maxdelta);
                        var o = new JArray(); foreach (var p in res) o.Add(PlaneOutJ(p));
                        var after = new JArray(); foreach (var p in planes) after.Add(PlaneInJ(p));
                        return new JObject { ["result"] = o, ["caller_after"] = after };
                    }));
                }
            // MULTI mode: two paths in sequence share one plane list
            {
                var p1 = CrossingPath(CrossingPaths[0]);
                var p2 = CrossingPath(CrossingPaths[1]);
                var planes = new List<PlaneInfo>();
                for (int i = 0; i < CrossingPlanes.Length; i++)
                    planes.Add(MakeCrossingPlane(p1, CrossingPlanes[i], i));
                var input = new JArray(); foreach (var p in planes) input.Add(PlaneInJ(p));
                var args = new JObject { ["paths"] = new JArray(new JArray(CrossingPaths[0]), new JArray(CrossingPaths[1])), ["planes"] = input, ["max_radius"] = 0.0, ["max_dist"] = maxdist, ["max_alt"] = maxalt, ["max_delta"] = maxdelta };
                a.Add(Case("nearest_multi", args, () => {
                    var r1 = UpstreamNearestPlanes.GetNearestPlanes(CrossingAt, p1, planes, 0, maxdist, maxalt, maxdelta);
                    var r2 = UpstreamNearestPlanes.GetNearestPlanes(CrossingAt.AddSeconds(30), p2, planes, 0, maxdist, maxalt, maxdelta);
                    var o1 = new JArray(); foreach (var p in r1) o1.Add(PlaneOutJ(p));
                    var o2 = new JArray(); foreach (var p in r2) o2.Add(PlaneOutJ(p));
                    var after = new JArray(); foreach (var p in planes) after.Add(PlaneInJ(p));
                    return new JObject { ["result1"] = o1, ["result2"] = o2, ["caller_after"] = after };
                }));
            }
            return a;
        }
    }
}
