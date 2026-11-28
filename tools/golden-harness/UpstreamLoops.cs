// VERBATIM COPIES of upstream algorithms @ 91b02cb7, with only the database access replaced by an
// in-memory tile dictionary and BackgroundWorker replaced by object (always null).
//   ScoutBase/ScoutBase.Elevation/ElevationDatabase.cs:
//     GetElevationTile(model, lat, lon)        L669-718
//     GetElevation(model, lat, lon, ...)       L720-736
//     MovingAverage                            L1375-1395
//     ElevationPathCreateFromLatLon            L1397-1504 (DB save removed)
//   ScoutBase/ScoutBase.Elevation/ElevationPath.cs: constructor (Count / Path init)
// Do not "fix" anything here: the point is to reproduce the original behaviour.
using System;
using System.Collections.Generic;
using System.Collections.Specialized;
using ScoutBase.Core;

namespace GoldenHarness
{
    public class UpstreamElevationPath
    {
        public double Bearing12 { get { return LatLon.Bearing(Lat1, Lon1, Lat2, Lon2); } }
        public double Distance { get { return LatLon.Distance(Lat1, Lon1, Lat2, Lon2); } }
        public double Lat1, Lon1, Lat2, Lon2, StepWidth;
        public int Count;
        public short[] Path;
        public bool Valid = true;

        public UpstreamElevationPath(double lat1, double lon1, double lat2, double lon2, double stepwitdh)
        {
            Lat1 = lat1; Lon1 = lon1; Lat2 = lat2; Lon2 = lon2; StepWidth = stepwitdh;
            Count = (int)(Distance * 1000.0 / StepWidth) + 1;
            Path = new short[Count];
            for (int i = 0; i < Count; i++)
                Path[i] = -32768;
        }
    }

    public class UpstreamElevationDb
    {
        public short ElvMissingFlag { get { return -32768; } }
        public short TileMissingFlag { get { return -32767; } }
        public double MinLat, MinLon, MaxLat, MaxLon;
        public bool Complete;
        public int CacheSize = int.MaxValue / 2;
        // in-memory replacement for the SQLite tile table: (model -> index -> json tile)
        public Dictionary<string, Dictionary<string, UpstreamTile>> Tiles = new Dictionary<string, Dictionary<string, UpstreamTile>>();
        Dictionary<string, UpstreamTile> lastTile = new Dictionary<string, UpstreamTile>();
        Dictionary<string, OrderedDictionary> caches = new Dictionary<string, OrderedDictionary>();

        public int GetElevationPathAveragePeriod(string model)
        {
            if (model == "ASTER1") return 5;
            if (model == "ASTER3") return 5;
            return 0;
        }

        UpstreamTile ElevationTileFind(string loc, string model)
        {
            UpstreamTile t;
            if (Tiles.TryGetValue(model, out var d) && d.TryGetValue(loc, out t)) return t;
            return null;
        }

        private UpstreamTile GetElevationTile(string model, double lat, double lon)
        {
            UpstreamTile tile;
            lastTile.TryGetValue(model, out tile);
            if ((tile != null) && (lat >= tile.Bounds.MinLat) && (lat <= tile.Bounds.MaxLat) && (lon >= tile.Bounds.MinLon) && (lon <= tile.Bounds.MaxLon))
                return tile;
            if (!caches.ContainsKey(model)) caches[model] = new OrderedDictionary();
            OrderedDictionary cache = caches[model];
            string loc = MaidenheadLocator.LocFromLatLon(lat, lon, false, 3);
            UpstreamTile td = null;
            if (cache != null)
            {
                lock (cache)
                {
                    td = (UpstreamTile)cache[loc];
                }
            }
            if (td == null)
            {
                td = this.ElevationTileFind(loc, model);
                if (td != null)
                {
                    if (cache.Count > CacheSize)
                    {
                        cache.RemoveAt(0);
                    }
                    else
                    {
                        cache.Add(loc, td);
                    }
                }
            }
            if (td != null)
            {
                lastTile[model] = td;
            }
            return td;
        }

        private short GetElevation(string model, double lat, double lon, bool setinvalidtozero = true)
        {
            if ((lat < -90) || (lat > 90))
                throw new ArgumentException("Invalid latitude: " + lat.ToString("F8"));
            if ((lon < -180) || (lon > 180))
                throw new ArgumentException("Invalid longitude: " + lon.ToString("F8"));
            short e = ElvMissingFlag;
            UpstreamTile tile = GetElevationTile(model, lat, lon);
            if (tile != null)
                e = tile.GetElevation(lat, lon);
            else
                e = TileMissingFlag;
                if (setinvalidtozero && (e <= TileMissingFlag))
                    e = 0;
            return e;
        }

        private short[] MovingAverage(short[] values, int periods)
        {
            if (values.Length < periods)
                return null;
            short[] averages = new short[values.Length - periods + 1];
            double sum = 0;
            for (int i = 0; i < values.Length; i++)
                if (i < periods)
                {
                    sum += values[i];
                    averages[0] = (short)((i == periods - 1) ? sum / (double)periods : 0);
                }
                else
                {
                    sum = sum - values[i - periods] + values[i];
                    averages[i - periods + 1] = (short)(sum / (double)periods);
                }
            return averages;
        }

        public UpstreamElevationPath ElevationPathCreateFromLatLon(object caller, double lat1, double lon1, double lat2, double lon2, double stepwidth, string model)
        {
            UpstreamElevationPath ep = new UpstreamElevationPath(lat1, lon1, lat2, lon2, stepwidth);
            bool tilemissing = false;
            stepwidth = stepwidth / 1000.0;
            bool complete = Complete;
            int avperiod = GetElevationPathAveragePeriod(model);
            if (avperiod == 0)
            {
                double d = 0;
                for (int i = 0; i < ep.Count; i++)
                {
                    LatLon.GPoint gp = LatLon.DestinationPoint(lat1, lon1, ep.Bearing12, d);
                    short e = GetElevation(model, gp.Lat, gp.Lon, false);
                    if (e > TileMissingFlag)
                    {
                        ep.Path[i] = e;
                    }
                    else
                    {
                        ep.Path[i] = 0;
                        if (e == TileMissingFlag)
                            tilemissing = true;
                    }
                    d += stepwidth;
                }
            }
            else
            {
                short[] raw = new short[ep.Path.Length + avperiod - 1];
                double d = -(avperiod / 2) * stepwidth;
                for (int i = 0; i < raw.Length; i++)
                {
                    LatLon.GPoint gp = LatLon.DestinationPoint(lat1, lon1, ep.Bearing12, d);
                    short e = GetElevation(model, gp.Lat, gp.Lon, false);
                    if (e > TileMissingFlag)
                    {
                        raw[i] = e;
                    }
                    else
                    {
                        raw[i] = 0;
                        if (e == TileMissingFlag)
                            tilemissing = true;
                    }
                    d += stepwidth;
                }
                ep.Path = MovingAverage(raw, avperiod);
            }
            if (complete)
                complete = Complete;
            ep.Valid = complete;
            if (tilemissing)
            {
                if (complete)
                {
                    if ((lat1 < MinLat) || (lat1 > MaxLat) ||
                        (lon1 < MinLon) || (lon1 > MaxLon) ||
                        (lat2 < MinLat) || (lat2 > MaxLat) ||
                        (lon2 < MinLon) || (lon2 > MaxLon))
                        ep.Valid = false;
                }
            }
            return ep;
        }
    }
}
