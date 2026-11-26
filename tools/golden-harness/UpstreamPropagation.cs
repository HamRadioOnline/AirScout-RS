// VERBATIM COPIES of upstream algorithms @ 91b02cb7 (DB access removed, BackgroundWorker -> null):
//   ScoutBase/ScoutBase.Propagation/PropagationPath.cs: constructor (LocalObstructed), GetInfoPoints,
//     GetInfoPoint, GetMidPoint, GetIntersectionPoint
//   ScoutBase/ScoutBase.Propagation/PropagationDatabase.cs: PropagationPathCreateFromLatLon L526-591
// Do not "fix" anything here.
using System;
using ScoutBase.Core;

namespace GoldenHarness
{
    public class UpstreamPropagationPoint
    {
        public double Lat, Lon, H1, H2, F1;
        public UpstreamPropagationPoint(double lat, double lon, double h1, double h2, double f1) { Lat = lat; Lon = lon; H1 = h1; H2 = h2; F1 = f1; }
    }

    public class UpstreamIntersectionPoint
    {
        public double Lat, Lon, QRB, Min_H, Dist1, Dist2;
        public UpstreamIntersectionPoint(double lat, double lon, double qrb, double min_h, double dist1, double dist2) { Lat = lat; Lon = lon; QRB = qrb; Min_H = min_h; Dist1 = dist1; Dist2 = dist2; }
    }

    public class UpstreamPropagationPath
    {
        public double Bearing12 { get { return LatLon.Bearing(Lat1, Lon1, Lat2, Lon2); } }
        public double Distance { get { return LatLon.Distance(Lat1, Lon1, Lat2, Lon2); } }
        public double Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth, Eps1_Min, Eps2_Min;
        public bool Valid = false;
        public bool LocalObstructed = false;
        public double LocalObstruction;

        public UpstreamPropagationPath(double lat1, double lon1, double h1, double lat2, double lon2, double h2, double qrg, double radius, double f1_clearance, double stepwidth, double eps1_min, double eps2_min, double localobstruction)
        {
            Lat1 = lat1; Lon1 = lon1; this.h1 = h1; Lat2 = lat2; Lon2 = lon2; this.h2 = h2;
            QRG = qrg; Radius = radius; F1_Clearance = f1_clearance; Eps1_Min = eps1_min; Eps2_Min = eps2_min;
            StepWidth = stepwidth;
            LocalObstruction = localobstruction;
            if (Eps1_Min < LocalObstruction)
                LocalObstructed = true;
        }

        public UpstreamPropagationPoint[] GetInfoPoints()
        {
            if (StepWidth <= 0)
                return null;
            UpstreamPropagationPoint[] d = new UpstreamPropagationPoint[(int)Distance + 2];
            double eps1_min = Math.Max(Eps1_Min, LocalObstruction);
            for (int i = 0; i < d.Length; i++)
            {
                d[i] = GetInfoPoint(i);
            }
            return d;
        }

        public UpstreamPropagationPoint GetInfoPoint(double dist)
        {
            if (StepWidth <= 0)
                return null;
            double eps1_min = Math.Max(Eps1_Min, LocalObstruction);
            LatLon.GPoint p = LatLon.DestinationPoint(Lat1, Lon1, Bearing12, dist);
            return new UpstreamPropagationPoint(
                p.Lat,
                p.Lon,
                ScoutBase.Core.Propagation.HeightFromEpsilon(h1, dist, eps1_min, Radius),
                ScoutBase.Core.Propagation.HeightFromEpsilon(h2, Distance - dist, Eps2_Min, Radius),
                ScoutBase.Core.Propagation.F1Radius(QRG, Distance, Distance - dist));
        }

        public LatLon.GPoint GetMidPoint()
        {
            return LatLon.MidPoint(Lat1, Lon1, Lat2, Lon2);
        }

        public UpstreamIntersectionPoint GetIntersectionPoint(double lat, double lon, double bearing, double maxdistance)
        {
            LatLon.GPoint p = LatLon.IntersectionPoint(Lat1, Lon1, Bearing12, lat, lon, bearing);
            if (p == null)
                return null;
            double dist1 = LatLon.Distance(Lat1, Lon1, p.Lat, p.Lon);
            double dist2 = LatLon.Distance(Lat2, Lon2, p.Lat, p.Lon);
            double eps1_min = Math.Max(Eps1_Min, LocalObstruction);
            double min_H = Math.Max(ScoutBase.Core.Propagation.HeightFromEpsilon(h1, dist1, eps1_min, Radius), ScoutBase.Core.Propagation.HeightFromEpsilon(h2, dist2, Eps2_Min, Radius));
            double qrb = LatLon.Distance(p.Lat, p.Lon, lat, lon);
            if (maxdistance < 0)
                return new UpstreamIntersectionPoint(p.Lat, p.Lon, qrb, min_H, dist1, dist2);
            if ((maxdistance == 0) && (qrb > Distance / 2))
                return null;
            if ((maxdistance > 0) && (qrb < maxdistance))
                return null;
            return new UpstreamIntersectionPoint(p.Lat, p.Lon, qrb, min_H, dist1, dist2);
        }
    }

    public class UpstreamPropagationDb
    {
        public double NearFieldSuppression { get; set; }
        public UpstreamElevationDb Elevation;

        public UpstreamPropagationPath PropagationPathCreateFromLatLon(object caller, double lat1, double lon1, double h1, double lat2, double lon2, double h2, double qrg, double radius, double f1_clearance, double stepwidth, string model, double localobstruction)
        {
            double eps1_min = double.MinValue;
            double eps2_min = double.MinValue;
            UpstreamElevationPath ep = Elevation.ElevationPathCreateFromLatLon(caller, lat1, lon1, lat2, lon2, stepwidth, model);
            if (ep == null)
                return null;
            {
                for (int i = 0; i < ep.Count; i++)
                {
                    double dist1 = i * stepwidth / 1000.0;
                    double dist2 = (ep.Count - i - 1) * stepwidth / 1000.0;
                    double f1c1 = ScoutBase.Core.Propagation.F1Radius(qrg, dist1, dist2) * f1_clearance;
                    double f1c2 = ScoutBase.Core.Propagation.F1Radius(qrg, dist2, dist1) * f1_clearance;
                    double nf = NearFieldSuppression / 1000.0;
                    double eps1;
                    double eps2;
                    if (dist1 > nf)
                        eps1 = ScoutBase.Core.Propagation.EpsilonFromHeights(h1, dist1, ep.Path[i] + f1c1, radius);
                    else
                        eps1 = ScoutBase.Core.Propagation.EpsilonFromHeights(h1, nf, ep.Path[i] + f1c1, radius);
                    if (eps1 > eps1_min)
                        eps1_min = eps1;
                    if (dist2 > nf)
                        eps2 = ScoutBase.Core.Propagation.EpsilonFromHeights(h2, dist2, ep.Path[i] + f1c2, radius);
                    else
                        eps2 = ScoutBase.Core.Propagation.EpsilonFromHeights(h2, nf, ep.Path[i] + f1c2, radius);
                    if (eps2 > eps2_min)
                        eps2_min = eps2;
                }
            }
            UpstreamPropagationPath pp = new UpstreamPropagationPath(lat1, lon1, h1, lat2, lon2, h2, qrg, radius, f1_clearance, stepwidth, eps1_min, eps2_min, localobstruction);
            pp.Valid = ep.Valid;
            return pp;
        }
    }
}
