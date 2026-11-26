// VERBATIM COPY of the elevation-lookup parts of
//   ScoutBase/ScoutBase.Elevation/ElevationTile.cs (ElevationTileDesignator) @ 91b02cb7
//   lines: TileIndex setter, Rows/Columns setters, GetElevation (approx. L40-75, L120-135).
// Only the SQLiteEntry base class, bitmap code and the ElevationData.Database reference were
// removed (ElvMissingFlag inlined as -32768). Do not "fix" anything here.
using System;
using ScoutBase.Core;

namespace GoldenHarness
{
    public class UpstreamTile
    {
        public double BaseLat = 0;
        public double BaseLon = 0;
        public double StepWidthLat = 0;
        public double StepWidthLon = 0;
        private string _TileIndex = "";
        public string TileIndex
        {
            get { return _TileIndex; }
            set
            {
                _TileIndex = value;
                BaseLat = MaidenheadLocator.LatFromLoc(value, PositionInRectangle.BottomLeft);
                BaseLon = MaidenheadLocator.LonFromLoc(value, PositionInRectangle.BottomLeft);
                Bounds = MaidenheadLocator.BoundsFromLoc(value);
            }
        }
        public int MinElv { get; set; }
        public double MinLat { get; set; }
        public double MinLon { get; set; }
        public int MaxElv { get; set; }
        public double MaxLat { get; set; }
        public double MaxLon { get; set; }
        private int _Rows = 50;
        public int Rows
        {
            get { return _Rows; }
            set { _Rows = value; StepWidthLat = 1 / 24.0 / value; }
        }
        private int _Columns = 100;
        public int Columns
        {
            get { return _Columns; }
            set { _Columns = value; StepWidthLon = 2 / 24.0 / value; }
        }
        public short[,] Elv { get; set; }
        // set from the TileIndex like the C# constructors do (BoundsFromLoc)
        [Newtonsoft.Json.JsonIgnore]
        public LatLon.GRect Bounds = new LatLon.GRect(0, 0, 0, 0);
        public DateTime LastUpdated { get; set; }

        public short GetElevation(double lat, double lon)
        {
            int i = (int)(((lat - BaseLat) / StepWidthLat));
            int j = (int)(((lon - BaseLon) / StepWidthLon));
            try
            {
                return Elv[j, i];
            }
            catch
            {
                return -32768;
            }
        }
    }
}
