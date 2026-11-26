// Fixed input case set for the golden fixtures (see Stage 1 plan, Task 1 Step 4).
using System.Collections.Generic;

namespace GoldenHarness
{
    public static class Cases
    {
        // (lat1, lon1, lat2, lon2)
        public static readonly double[][] Pairs = new double[][]
        {
            new double[] { 50.0208333333, 10.0416666667, 51.2291666667, 13.7083333333 }, // JO50AA <-> JO61UF centres
            new double[] { 51.5, 10.0, 51.5, -0.1 },          // DL <-> G
            new double[] { 10.0, 179.99, 10.0, -179.99 },     // across antimeridian
            new double[] { 89.9, 0.0, 89.9, 180.0 },          // near pole
            new double[] { 50.0, 10.0, 50.0, 10.0 },          // identical
            new double[] { -33.9, 18.4, -34.6, -58.4 },       // southern hemisphere
        };

        public static readonly double[] Bearings = { 0, 45, 90, 135, 180, 225, 270, 315, 359.9 };
        public static readonly double[] Distances = { 0, 0.05, 1, 100, 400, 800 };

        public static readonly string[] Locators =
        {
            "JO50AA", "jo50aa", "JO50", "JO50AA12", "JO50AA12AB", "AA00AA", "RR99XX", "ZZ99", "", "JO5", "jo50Aa", "JO50AY", "JN", "JO50AA12AB34CD56EF78",
        };

        // (lat, lon) for LocFromLatLon / IsPrecise
        public static readonly double[][] Points = new double[][]
        {
            new double[] { 50.0, 10.0 }, new double[] { 50.02083333333333, 10.041666666666666 }, new double[] { -90, -180 },
            new double[] { 90, 180 }, new double[] { 89.999999, 179.999999 }, new double[] { 0, 0 }, new double[] { -33.9, 18.4 },
            new double[] { 51.2291666667, 13.7083333333 }, new double[] { 90.0001, 0 }, new double[] { 0, -180.0001 },
            new double[] { 48.1234567, 11.7654321 },
        };

        public static readonly double[] Hs = { 0, 10, 500, 2000 };
        public static readonly double[] FormulaDists = { 0, 0.05, 1, 100, 400, 800 };
        public static readonly double[] BigHs = { 0, 5000, 12000 };
        public static readonly double[] Radii = { 6371, 6371 * 1.33 };
        public static readonly double[] Epsilons = { -0.01, 0, 0.005, 0.05 };
        public static readonly double[] Qrgs = { 0.144, 0.432, 1.296, 10.368 };
    }
}
