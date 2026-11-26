// Support code for linking AirScout.Core / AirScout.Aircrafts files (upstream 91b02cb).
//   UnitConverter, DateTimeToUNIXTime, UNIXTimeToDateTime: verbatim from
//     ScoutBase/ScoutBase.Core/SupportFunctions.cs L409-477 (that file needs WinForms, so it is not linked).
//   AircraftData.Database: an in-memory stand-in for AircraftDatabase with a fixed table:
//     airlines ICAO DLH/BAW/KLM with IATA LH/BA/KL, registration prefixes D/G/HB/N/PH,
//     airports EDDF/EGLL, aircraft type A320.
//     AircraftRegistrationFindByReg is verbatim from AircraftDatabase.cs L1986-2013.
// Do not "fix" anything here.
using System;
using System.Collections.Generic;

namespace ScoutBase.Core
{
    public static class SupportFunctions
    {
        public static int DateTimeToUNIXTime(DateTime dt)
        {
            if (dt == DateTime.MinValue)
                return int.MinValue;
            else if (dt == DateTime.MaxValue)
                return int.MaxValue;
            return (Int32)(dt.Subtract(new DateTime(1970, 1, 1))).TotalSeconds;
        }

        public static DateTime UNIXTimeToDateTime(int ut)
        {
            if (ut == int.MinValue)
                return DateTime.MinValue;
            else if (ut == int.MaxValue)
                return DateTime.MaxValue;
            DateTime dt = new DateTime(1970, 1, 1, 0, 0, 0, 0, DateTimeKind.Utc);
            dt = dt.AddSeconds(ut);
            return dt;
        }
    }

    public static class UnitConverter
    {
        public static double ft_m(double feet) { return feet / 3.28084; }
        public static double m_ft(double m) { return m * 3.28084; }
        public static double kts_kmh(double kts) { return kts * 1.852; }
        public static double kmh_kts(double kmh) { return kmh / 1.852; }
        public static double km_mi(double km) { return km * 1.609; }
        public static double mi_km(double mi) { return mi / 1.609; }
    }
}

namespace AirScout.Aircrafts
{
    public class AirlineDesignator { }
    public class AirportDesignator { }
    public class AircraftTypeDesignator { }
    public class AircraftRegistrationDesignator { public string Prefix; }

    public class StubAircraftDatabase
    {
        static readonly HashSet<string> AirlinesICAO = new HashSet<string> { "DLH", "BAW", "KLM" };
        static readonly HashSet<string> AirlinesIATA = new HashSet<string> { "LH", "BA", "KL" };
        static readonly HashSet<string> Prefixes = new HashSet<string> { "D", "G", "HB", "N", "PH" };
        static readonly HashSet<string> Airports = new HashSet<string> { "EDDF", "EGLL" };
        static readonly HashSet<string> Types = new HashSet<string> { "A320" };

        public long AircraftRegistrationMinLength = 1;
        public long AircraftTypeICAOMinLength = 4;

        public AirlineDesignator AirlineFindByICAO(string icao) { return AirlinesICAO.Contains(icao) ? new AirlineDesignator() : null; }
        public AirlineDesignator AirlineFindByIATA(string iata) { return AirlinesIATA.Contains(iata) ? new AirlineDesignator() : null; }
        public AirportDesignator AirportFindByICAO(string icao) { return Airports.Contains(icao) ? new AirportDesignator() : null; }
        public AircraftTypeDesignator AircraftTypeFindByICAO(string icao) { return Types.Contains(icao) ? new AircraftTypeDesignator() : null; }
        public AircraftRegistrationDesignator AircraftRegistrationFind(string prefix) { return Prefixes.Contains(prefix) ? new AircraftRegistrationDesignator { Prefix = prefix } : null; }

        public AircraftRegistrationDesignator AircraftRegistrationFindByReg(string reg)
        {
            if (String.IsNullOrEmpty(reg))
                return null;
            reg = reg.ToUpper().Trim();
            // check for US registration --> insert '-' after first letter
            if (reg.StartsWith("N"))
            {
                if (Char.IsDigit(reg[1]))
                    reg = reg[0] + "-" + reg.Substring(1);
                else
                    return null;
            }
            // return null if not a registration
            if (reg.IndexOf("-") == 0)
                return null;
            // stop search at char before '-' should be clean prefix then
            int stop = reg.IndexOf("-") - 1;
            // start search two chars after '-'
            int i = stop + 2;
            while (i >= stop)
            {
                AircraftRegistrationDesignator rd = AircraftRegistrationFind(reg.Substring(0, i));
                if (rd != null)
                    return rd;
                i--;
            }
            return null;
        }
    }

    public static class AircraftData
    {
        public static StubAircraftDatabase Database = new StubAircraftDatabase();
    }
}
