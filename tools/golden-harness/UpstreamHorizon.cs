// Verbatim copies (upstream 91b02cb) of:
//   ScoutBase/ScoutBase.Propagation/PropagationDatabase.cs: PropagationHorizonCreate L834-915
//     (BackgroundWorker reporting and DB save removed; ElevationPathFindOrCreateFromBearing inlined as
//      DestinationPoint + ElevationPathCreateFromLatLon, which is what it does without a cached path)
//   ScoutBase/ScoutBase.Propagation/PropagationHorizon.cs: EpsilonMaxFromPath L153-185
//   ScoutBase/ScoutBase.Propagation/HorizonPoint.cs
// Do not "fix" anything here.
using System;
using ScoutBase.Core;

namespace GoldenHarness
{
    public class UpstreamHorizonPoint
    {
        public double Dist { get; set; }
        public double Epsmin { get; set; }
        public short Elv { get; set; }
        public UpstreamHorizonPoint() { Dist = double.MinValue; Epsmin = double.MinValue; Elv = short.MinValue; }
        public UpstreamHorizonPoint(double dist, double epsmin, short elv) { Dist = dist; Epsmin = epsmin; Elv = elv; }
    }

    public static class UpstreamHorizon
    {
        public static UpstreamHorizonPoint[] PropagationHorizonCreate(UpstreamPropagationDb db, double lat, double lon, double h, double dist, double qrg, double radius, double f1_clearance, double stepwidth, string model, out bool hvalid)
        {
            UpstreamHorizonPoint[] hor = new UpstreamHorizonPoint[360];
            bool valid = true;
            for (int j = 0; j < 360; j++)
            {
                try
                {
                    double eps_min = double.MinValue;
                    double eps_dist = 0;
                    short eps_elv = 0;
                    LatLon.GPoint gp = LatLon.DestinationPoint(lat, lon, j, dist);
                    UpstreamElevationPath ep = db.Elevation.ElevationPathCreateFromLatLon(null, lat, lon, gp.Lat, gp.Lon, stepwidth, model);
                    for (int i = 0; i < ep.Count; i++)
                    {
                        double d = i * stepwidth / 1000.0;
                        double nf = db.NearFieldSuppression / 1000.0;
                        double eps;
                        if (d > nf)
                        {
                            double f1c = ScoutBase.Core.Propagation.F1Radius(qrg, d, dist) * f1_clearance;
                            eps = ScoutBase.Core.Propagation.EpsilonFromHeights(h, d, ep.Path[i] + f1c, radius);
                        }
                        else
                        {
                            double f1c = ScoutBase.Core.Propagation.F1Radius(qrg, nf, dist) * f1_clearance;
                            eps = ScoutBase.Core.Propagation.EpsilonFromHeights(h, nf, ep.Path[i] + f1c, radius);
                        }
                        if (eps > eps_min)
                        {
                            eps_min = eps;
                            eps_dist = d;
                            eps_elv = ep.Path[i];
                        }
                    }
                    hor[j] = new UpstreamHorizonPoint(eps_dist, eps_min, eps_elv);
                    if (!ep.Valid)
                        valid = false;
                }
                catch (Exception ex)
                {

                }
            }
            hvalid = valid;
            return hor;
        }

        public static UpstreamHorizonPoint EpsilonMaxFromPath(double h1, ref short[] elv, double dist, double stepwidth, double freq, double f1_clearance, double maxelv, double re)
        {
            UpstreamHorizonPoint hp = new UpstreamHorizonPoint();
            stepwidth = stepwidth / 1000.0;
            double d = 0;
            for (int i = 0; i < elv.Length; i++)
            {
                double f1 = ScoutBase.Core.Propagation.F1Radius(freq, d, dist - d);
                double h2 = elv[i] + f1 * f1_clearance;
                double eps = ScoutBase.Core.Propagation.EpsilonFromHeights(h1, d, h2, re);
                if (eps > hp.Epsmin)
                {
                    // ignore first 100m
                    if (d > 0.1)
                    {
                        hp.Epsmin = eps;
                        hp.Dist = d;
                        hp.Elv = elv[i];
                    }
                }
                if (maxelv > 0)
                {
                    double epsmax = ScoutBase.Core.Propagation.EpsilonFromHeights(h1, d, maxelv, re);
                    if (epsmax < hp.Epsmin)
                    {
                        Array.Resize<short>(ref elv, i);
                        return hp;
                    }
                }
                d += stepwidth;
            }
            return hp;
        }
    }
}
