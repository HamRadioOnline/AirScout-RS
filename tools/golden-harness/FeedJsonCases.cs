// Survey #33: what Newtonsoft.Json does with the Readsb / Dump1090 DTOs (alt_baro / altitude
// declared as double?) when a receiver reports "ground" instead of a number.
using System;
using System.IO;
using System.Linq;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace GoldenHarness
{
    public static partial class Program
    {
        static string FeedFixture(string goldenDir, string name) =>
            File.ReadAllText(Path.Combine(goldenDir, "..", "fixtures", "feeds", name));

        // the fixture without the aircraft whose altitude field is the string "ground"
        static string WithoutGround(string json, string altKey)
        {
            var root = JObject.Parse(json);
            var list = (JArray)root["aircraft"];
            foreach (var ac in list.Where(a => a[altKey] != null && a[altKey].Type == JTokenType.String).ToList())
                ac.Remove();
            return root.ToString();
        }

        static JArray FeedJsonCases(string goldenDir)
        {
            var cases = new JArray();
            string readsb = FeedFixture(goldenDir, "readsb_aircraft.json");
            string dump1090 = FeedFixture(goldenDir, "dump1090_aircraft.json");
            cases.Add(Case("readsb_deserialize", new JObject { ["fixture"] = "readsb_aircraft.json" },
                () => JsonConvert.DeserializeObject<Readsb.AircraftListJson>(readsb).Aircraft.Count));
            cases.Add(Case("readsb_deserialize", new JObject { ["fixture"] = "readsb_aircraft.json", ["without_ground"] = true },
                () => JsonConvert.DeserializeObject<Readsb.AircraftListJson>(WithoutGround(readsb, "alt_baro")).Aircraft.Count));
            cases.Add(Case("dump1090_deserialize", new JObject { ["fixture"] = "dump1090_aircraft.json" },
                () => JsonConvert.DeserializeObject<Dump1090.AircraftListJson>(dump1090).Aircraft.Count));
            cases.Add(Case("dump1090_deserialize", new JObject { ["fixture"] = "dump1090_aircraft.json", ["without_ground"] = true },
                () => JsonConvert.DeserializeObject<Dump1090.AircraftListJson>(WithoutGround(dump1090, "altitude")).Aircraft.Count));
            return cases;
        }
    }
}
