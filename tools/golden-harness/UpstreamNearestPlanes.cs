// VERBATIM COPY of upstream 91b02cb AirScout.Aircrafts/AircraftDatabase.cs GetNearestPlanes (L2477-2650).
// Only type names are adapted (PropagationPathDesignator -> UpstreamPropagationPath,
// IntersectionPoint -> UpstreamIntersectionPoint), Log -> Console.Error, unused Stopwatch removed.
// Do not "fix" anything here.
using System;
using System.Collections.Generic;
using System.Linq;
using ScoutBase.Core;
using AirScout.Core;

namespace GoldenHarness
{
    public static class UpstreamNearestPlanes
    {
        public static List<PlaneInfo> GetNearestPlanes(DateTime at, UpstreamPropagationPath ppath, List<PlaneInfo> planes, double maxradius, double maxdist, double maxalt, double maxdelta)
        {
            List<PlaneInfo> l = new List<PlaneInfo>();
            // return empty list on empty list or null
            if (planes == null)
                return l;
            if (planes.Count() == 0)
                return l;
            // adjust maxradius when automatic calculation is required
            if (maxradius < 0)
                maxradius = double.MaxValue;
            if (maxradius == 0)
                maxradius = ppath.Distance / 2;
            // get midpoint value
            double midlat = ppath.GetMidPoint().Lat;
            double midlon = ppath.GetMidPoint().Lon;
            try
            {
//                StreamWriter sw = new StreamWriter("GetNearestPlanes.csv");
//                sw.WriteLine("utc;hex;call;lat;lon;alt;speed;track;maxdist");
                // get intersection info for each plane in list
                foreach (PlaneInfo info in planes)
                {
                    PlaneInfo plane;
                    try
                    {
                        // skip if plane is out of range
                        double dist = LatLon.Distance(midlat, midlon, info.Lat, info.Lon);
/*
                        sw.WriteLine(info.Time.ToString() + ";" +
                            info.Hex + ";" +
                            info.Call + ";" +
                            info.Lat.ToString("") + ";" +
                            info.Lon.ToString("") + ";" +
                            info.Alt_m.ToString("") + ";" +
                            info.Speed_kmh.ToString("") + ";" +
                            info.Track.ToString("") + ";" +
                            maxdist.ToString(""));
*/
                        if (dist > maxradius)
                            continue;

                        // clone object
                        plane = new PlaneInfo(info.Time, info.Call, info.Reg, info.Hex, info.Lat, info.Lon, info.Track, info.Alt, info.Speed, info.Type, info.Manufacturer, info.Model, info.Category);
                        //estimate new position
                        // change speed to km/h
                        double speed = info.Speed_kmh;
                        // calculate distance after timespan
                        double di = speed * (at - info.Time).TotalHours;
                        // estimate new position 
                        LatLon.GPoint newpos = LatLon.DestinationPoint(info.Lat, info.Lon, info.Track, di);
                        info.Lat = newpos.Lat;
                        info.Lon = newpos.Lon;
                        info.Time = at;

                        // Test!!!
                        if (info.Call.StartsWith("xxx"))
                        {
                            int k = 3;
                        }

                        // calculate four possible intersections
                        // i1 -->       plane heading
                        // i2 -->       plane heading +90°
                        // i3 -->       plane heading - 90°
                        // i4 -->       opposite plane heading
                        // imin -->     intpoint with shortest distance

                        UpstreamIntersectionPoint imin = null;
                        UpstreamIntersectionPoint i1 = null;
                        UpstreamIntersectionPoint i2 = null;
                        UpstreamIntersectionPoint i3 = null;
                        UpstreamIntersectionPoint i4 = null;
                        
                        i1 = ppath.GetIntersectionPoint(plane.Lat, plane.Lon, plane.Track, 0);
                        i2 = ppath.GetIntersectionPoint(plane.Lat, plane.Lon, ppath.Bearing12 - 90, 0);
                        // calculate right opposite direction only if no left intersection was found
                        if (i2 == null)
                            i3 = ppath.GetIntersectionPoint(plane.Lat, plane.Lon, ppath.Bearing12 + 90, 0);
                        // calcalute opposite direction only if no forward intersection was found
                        if (i1 == null)
                            i4 = ppath.GetIntersectionPoint(plane.Lat, plane.Lon, plane.Track - 180, 0);
                        // find the minimum distance first
                        if (i1 != null)
                            imin = i1;
                        if ((i2 != null) && ((imin == null) || (i2.QRB < imin.QRB)))
                            imin = i2;
                        if ((i3 != null) && ((imin == null) || (i3.QRB < imin.QRB)))
                            imin = i3;
                        if ((i4 != null) && ((imin == null) || (i4.QRB < imin.QRB)))
                            imin = i4;
                        // check hot planes which are very near the path first
                        if ((imin != null) && (imin.QRB <= maxdist) && (imin.Dist1 <= ppath.Distance))
                        {
                            // plane is near path
                            // use the minimum qrb info
                            plane.IntPoint = new LatLon.GPoint(imin.Lat, imin.Lon);
                            plane.IntQRB = imin.QRB;
                            plane.AltDiff = plane.Alt_m - imin.Min_H;
                            double c1 = LatLon.Bearing(plane.IntPoint.Lat, plane.IntPoint.Lon, ppath.Lat2, ppath.Lon2);
                            double c2 = plane.Track;
                            double ca = c1 - c2;
                            if (ca < 0)
                                ca = ca + 360;
                            if ((ca > 180) && (ca < 360))
                                ca = 360 - ca;
                            // save in rad 
                            plane.Angle = ca / 180.0 * Math.PI;
                            plane.Eps1 = ScoutBase.Core.Propagation.EpsilonFromHeights(ppath.h1, imin.Dist1, plane.Alt_m, ppath.Radius);
                            plane.Eps2 = ScoutBase.Core.Propagation.EpsilonFromHeights(ppath.h2, imin.Dist2, plane.Alt_m, ppath.Radius);
                            plane.Theta1 = ScoutBase.Core.Propagation.ThetaFromHeights(ppath.h1, imin.Dist1, plane.Alt_m, ppath.Radius);
                            plane.Theta2 = ScoutBase.Core.Propagation.ThetaFromHeights(ppath.h2, imin.Dist2, plane.Alt_m, ppath.Radius);
                            plane.Squint = Math.Abs(ScoutBase.Core.Propagation.ThetaFromHeights(ppath.h1, imin.Dist1, plane.Alt_m, ppath.Radius) - ScoutBase.Core.Propagation.ThetaFromHeights(ppath.h2, imin.Dist2, plane.Alt_m, ppath.Radius));
                            if (plane.AltDiff > 0)
                            {
                                // plane is high enough
                                plane.Potential = 100;
                            }
                            else if ((imin.Min_H <= maxalt) && (plane.AltDiff > maxdelta))
                            {
                                // plane is not high enough yet but might be in the future
                                plane.Potential = 50;
                            }
                            else
                            {
                                // minimal needed altitude is higher than Planes_MaxAlt --> no way to reach
                                // plane is not interesting
                                plane.Potential = 0;
                            }
                        }
                        else
                        {
                            // plane is far from path --> check only intersection i1 = planes moves towards path
                            if ((i1 != null) && (i1.Min_H <= maxalt) && (i1.Dist1 <= ppath.Distance))
                            {
                                plane.IntPoint = new LatLon.GPoint(i1.Lat, i1.Lon);
                                plane.IntQRB = i1.QRB;
                                plane.AltDiff = plane.Alt_m - i1.Min_H;
                                plane.Eps1 = ScoutBase.Core.Propagation.EpsilonFromHeights(ppath.h1, i1.Dist1, plane.Alt_m, ppath.Radius);
                                plane.Eps2 = ScoutBase.Core.Propagation.EpsilonFromHeights(ppath.h2, i1.Dist2, plane.Alt_m, ppath.Radius);
                                plane.Squint = Math.Abs(ScoutBase.Core.Propagation.ThetaFromHeights(ppath.h1, imin.Dist1, plane.Alt_m, ppath.Radius) - ScoutBase.Core.Propagation.ThetaFromHeights(ppath.h2, imin.Dist2, plane.Alt_m, ppath.Radius));
                                if (plane.AltDiff > 0)
                                {
                                    // plane wil cross path in a suitable altitude
                                    plane.Potential = 75;
                                }
                                else if (plane.AltDiff > -maxdelta)
                                {
                                    // plane wil cross path not in a suitable altitude
                                    plane.Potential = 50;
                                }
                            }
                            else
                            {
                                // plane is not interesting
                                plane.Potential = 0;
                            }
                        }

                        // add plane to list
                        l.Add(plane);
                    }
                    catch (Exception ex)
                    {
                        Console.Error.WriteLine(ex.ToString());
                    }
                }
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine(ex.ToString());
            }
            return l;
        }
    }
}
