// Golden cases for LibADSB message decoding (ModeSReply, ExtendedSquitter, message types, CPR).
using System;
using System.Collections.Generic;
using System.Reflection;
using Newtonsoft.Json.Linq;
using LibADSB;

namespace GoldenHarness
{
    public static partial class Program
    {
        // Real vectors (The 1090MHz Riddle) and crafted ones. Crafted messages use parity 000000,
        // which the C# decoder accepts without a CRC check.
        static readonly string[] AdsbMessages = {
            // identification
            "8D4840D6202CC371C32CE0576098",
            "8d4840d6202cc371c32ce0576098",
            "8D4840D623FFFFFFFFFFFF000000",
            "8D4840D6082CC371C32CE0000000",
            "8D4840D6182CC371C32CE0000000",
            // airborne position (even/odd pair) and crafted variants
            "8D40621D58C382D690C8AC2863A7",
            "8D40621D58C386435CC412692AD6",
            "8D40621D5814F2D690C8AC000000",
            "8D40621D582A62D690C8AC000000",
            "8D40621D588032D690C8AC000000",
            "8D40621D580C52D690C8AC000000",
            "8D40621D58FEF2D690C8AC000000",
            "8D40621D580002D690C8AC000000",
            "8D40621DA0C382D690C8AC000000",
            "8D40621D58C38600000000000000",
            "8D40621D58C3864A2B1C3D000000",
            "8D40621D58C387FFFFFFFF000000",
            "8D40621D58C38612345678000000",
            "8D40621D58C38200000000000000",
            "8D40621D90C382D690C8AC000000",
            "8D40621D58C38621A00000000000",
            // surface position
            "8C4841753AAB238733C8CD4020B1",
            "8C4841753A8A35323FAEBDAC702D",
            "8C4841753A9A153237AEF0F275BE",
            "8C484175380A153237AEF0000000",
            "8C484175381A153237AEF0000000",
            "8C4841753FCA153237AEF0000000",
            "8C4841753FDA153237AEF0000000",
            "8C484175281A153237AEF0000000",
            // velocity over ground
            "8D485020994409940838175B284F",
            "8D48502099400914083817000000",
            "8D4850209A440994083817000000",
            "8D48502098440994083817000000",
            "8D48502099440094083817000000",
            "8D48502099440994080017000000",
            "8D48502099440994083800000000",
            "8D48502099440994083880000000",
            "8D48502099470A94083817000000",
            // airspeed and heading
            "8DA05F219B06B6AF189400CBC33F",
            "8DA05F219C06B6AF189400000000",
            "8DA05F219B06B680189400000000",
            "8DA05F219B02B6AF189400000000",
            // other type codes and formats
            "8D40621DE0C382D690C8AC000000",
            "8D40621D00C382D690C8AC000000",
            "5D484FDEA248F5",
            "5D484FDEA248F6",
            "20000F1F684A6C",
            "A0001838CA3E51F0A8000047A36A",
            // bad CRC copies
            "8D4840D6202CC371C32CE0576099",
            "8D40621D58C382D690C8AC2863A8",
            "8D485020994409940838175B284E",
            "8DA05F219B06B6AF189400CBC330",
            // malformed
            "8D4840D6202CC",
            "8D4840D6202CC3",
            "8D4840D6202CZ371C32CE0576098",
            "ZZ4840D6202CC371C32CE0576098",
            "8D4840D6202CC371C32CE05760ZZ",
            "",
        };

        static string Hex(byte[] b) => BitConverter.ToString(b).Replace("-", string.Empty);
        static int Int24(byte[] b) => (b[0] << 16) | (b[1] << 8) | b[2];

        static JObject AdsbDescribe(ModeSReply m)
        {
            var o = new JObject
            {
                ["df"] = m.DownlinkFormat,
                ["ca"] = m.Capabilities,
                ["icao24"] = Int24(m.ICAO24),
                ["payload"] = Hex(m.Payload),
                ["parity"] = Int24(m.Parity),
                ["recalc_parity"] = Int24(m.RecalculatedParity),
                ["parity_ok"] = m.CheckParity,
                ["zero_parity"] = m.IsZeroParity(),
            };
            switch (m)
            {
                case IdentificationMsg i:
                    o["kind"] = "identification";
                    o["tc"] = i.FormatTypeCode;
                    o["emitter_category"] = i.EmitterCategory;
                    o["call"] = i.getIdentity();
                    break;
                case AirbornePositionMsg p:
                    o["kind"] = "airborne_position";
                    o["tc"] = p.FormatTypeCode;
                    o["odd"] = p.IsOddFormat;
                    o["cpr_lat"] = p.CPREncodedLat;
                    o["cpr_lon"] = p.CPREncodedLon;
                    o["has_position"] = p.HasValidPosition;
                    o["has_altitude"] = p.HasValidAltitude;
                    o["altitude"] = p.Altitude;
                    o["surveillance_status"] = p.SurveillanceStatus;
                    o["nic_suppl_b"] = p.NICSupplementB;
                    o["utc"] = p.IsUTCTime;
                    break;
                case SurfacePositionMsg s:
                    o["kind"] = "surface_position";
                    o["tc"] = s.FormatTypeCode;
                    o["odd"] = s.IsOddFormat;
                    o["cpr_lat"] = s.CPREncodedLat;
                    o["cpr_lon"] = s.CPREncodedLon;
                    o["has_position"] = s.HasValidPosition;
                    o["has_ground_speed"] = s.HasGroundSpeed;
                    try { o["ground_speed"] = s.GroundSpeed; } catch (Exception ex) { o["ground_speed"] = ex.GetType().Name; }
                    o["has_heading"] = s.HasValidHeading;
                    o["heading"] = s.Heading;
                    break;
                case VelocityOverGroundMsg v:
                    o["kind"] = "velocity_over_ground";
                    o["tc"] = v.FormatTypeCode;
                    o["supersonic"] = v.isSupersonic();
                    o["has_velocity"] = v.HasValidVelocity;
                    o["velocity"] = v.Velocity;
                    o["ew"] = v.EastToWestVelocity;
                    o["ns"] = v.NorthToSouthVelocity;
                    o["heading"] = v.Heading;
                    o["has_vrate"] = v.HasValidVerticalRate;
                    o["vrate"] = v.VerticalRate;
                    o["has_geo_minus_baro"] = v.HasGeoMinusBaro;
                    o["geo_minus_baro"] = v.GeoMinusBaro;
                    break;
                case AirspeedHeadingMsg a:
                    o["kind"] = "airspeed_heading";
                    o["tc"] = a.FormatTypeCode;
                    o["supersonic"] = a.isSupersonic();
                    o["has_airspeed"] = a.HasValidAirspeed;
                    o["airspeed"] = a.Airspeed;
                    o["true_airspeed"] = a.IsTrueAirspeed;
                    o["has_heading"] = a.HasValidHeading;
                    o["heading"] = a.Heading;
                    o["has_vrate"] = a.HasValidVerticalRate;
                    o["vrate"] = a.VerticalRate;
                    o["has_geo_minus_baro"] = a.HasGeoMinusBaro;
                    o["geo_minus_baro"] = a.GeoMinusBaro;
                    break;
                case ExtendedSquitter e:
                    o["kind"] = "extended_squitter";
                    o["tc"] = e.FormatTypeCode;
                    break;
                default:
                    o["kind"] = "mode_s";
                    break;
            }
            return o;
        }

        static double AdsbNL(double lat)
        {
            var mi = typeof(AirbornePositionMsg).GetMethod("NL", BindingFlags.NonPublic | BindingFlags.Instance);
            var msg = (AirbornePositionMsg)Decoder.GenericDecoder("8D40621D58C382D690C8AC2863A7");
            return (double)mi.Invoke(msg, new object[] { lat });
        }

        static JToken LatLonJ(double[] p) => new JObject { ["lat"] = p[0], ["lon"] = p[1] };

        static JArray AdsbCases()
        {
            var a = new JArray();
            foreach (var m in AdsbMessages)
                a.Add(Case("decode", new JObject { ["hex"] = m }, () => AdsbDescribe(Decoder.GenericDecoder(m))));

            foreach (var lat in new[] { 0.0, 10.0, 10.47047130, 10.4704713, 45.0, 52.2572, -52.2572, 59.95, 86.99, 87.0, 87.01, -87.0, 89.9, 90.0, -90.0 })
                a.Add(Case("nl", new JObject { ["lat"] = lat }, () => AdsbNL(lat)));

            // global airborne: (newer, other)
            var airborne = new[] {
                "8D40621D58C382D690C8AC2863A7", "8D40621D58C386435CC412692AD6",
                "8D40621D58C38600000000000000", "8D40621D58C3864A2B1C3D000000",
                "8D40621D58C387FFFFFFFF000000", "8D40621D58C38612345678000000",
                "8D40621D58C38200000000000000", "8D40621D58C38621A00000000000",
            };
            foreach (var newer in airborne)
                foreach (var other in airborne)
                {
                    if (newer == other) continue;
                    a.Add(Case("global_airborne", new JObject { ["newer"] = newer, ["other"] = other }, () =>
                    {
                        var n = (AirbornePositionMsg)Decoder.GenericDecoder(newer);
                        var o = (AirbornePositionMsg)Decoder.GenericDecoder(other);
                        return LatLonJ(n.getGlobalPosition(o));
                    }));
                }
            // different transmitter
            a.Add(Case("global_airborne", new JObject { ["newer"] = "8D40621D58C382D690C8AC2863A7", ["other"] = "8D40621E58C386435CC412000000" }, () =>
            {
                var n = (AirbornePositionMsg)Decoder.GenericDecoder("8D40621D58C382D690C8AC2863A7");
                var o = (AirbornePositionMsg)Decoder.GenericDecoder("8D40621E58C386435CC412000000");
                return LatLonJ(n.getGlobalPosition(o));
            }));

            var surface = new[] { "8C4841753AAB238733C8CD4020B1", "8C4841753A8A35323FAEBDAC702D", "8C4841753A9A153237AEF0F275BE" };
            foreach (var newer in surface)
                foreach (var other in surface)
                {
                    if (newer == other) continue;
                    a.Add(Case("global_surface", new JObject { ["newer"] = newer, ["other"] = other }, () =>
                    {
                        var n = (SurfacePositionMsg)Decoder.GenericDecoder(newer);
                        var o = (SurfacePositionMsg)Decoder.GenericDecoder(other);
                        return LatLonJ(n.getGlobalPosition(o));
                    }));
                }

            var refs = new[] { (52.258, 3.918), (51.99, 4.375), (0.0, 0.0), (-33.9, 18.4), (60.0, -150.0), (52.2572, 3.9194) };
            foreach (var m in airborne)
                foreach (var r in refs)
                    a.Add(Case("local_airborne", new JObject { ["hex"] = m, ["ref_lat"] = r.Item1, ["ref_lon"] = r.Item2 }, () =>
                        LatLonJ(((AirbornePositionMsg)Decoder.GenericDecoder(m)).getLocalPosition(r.Item1, r.Item2))));
            foreach (var m in surface)
                foreach (var r in refs)
                    a.Add(Case("local_surface", new JObject { ["hex"] = m, ["ref_lat"] = r.Item1, ["ref_lon"] = r.Item2 }, () =>
                        LatLonJ(((SurfacePositionMsg)Decoder.GenericDecoder(m)).getLocalPosition(r.Item1, r.Item2))));
            return a;
        }
    }
}
