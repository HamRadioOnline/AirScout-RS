// VERBATIM COPY of the ElevationTileGenerator tile code @ 91b02cb7:
//   SQLiteDatabase/SQLiteDatabase/SQLiteEntry.cs: LastUpdated (L64), ToJSON with its doc comment (L186-199)
//   ScoutBase/ScoutBase.Elevation/ElevationTile.cs: ElevationTileDesignator (L39-124)
//   ElevationTileGenerator/MainDlg.cs: IsGLOBEFile, IsSRTM3File, IsSRTM1File (L72-196),
//     ProcessTile (L199-522), CompareFilesHGT (L1092-1150),
//     GetSRTM1Elevation and ConvertSRTM1ToSRTM3File (L1252-1301)
// Edits, all of them:
//   - SQLiteEntry -> UpstreamSQLiteEntry with only LastUpdated and ToJSON.
//   - ElevationTileDesignator -> UpstreamTileDesignator, without the DataRow/IDataRecord
//     constructors and ToBitmap (L125 on).
//   - ProcessTile(string tilename, int current, int count) ->
//     public ProcessTile(string tilename, string destDir, int oi0, int oj0, int oi1, int oj1).
//   - ProcessTile: no Stopwatch, tsl_Status, ss_Main or Application.DoEvents lines.
//   - ProcessTile: Properties.Settings.Default.DestDir -> destDir;
//     DateTime.UtcNow -> FakeTileClock.UtcNow.
//   - ProcessTile, GLOBE branch: the loops over i0, j0, i1, j1 run only over oi0, oj0, oi1, oj1
//     (a full GLOBE file is 1.15 million tiles); i2, j2 and the body are verbatim.
//   - ProcessTile: one added line per branch right after `filename` is computed,
//     Visited.Add(tilebase + x + y) (SRTM) and Visited.Add(tileindex) (GLOBE), so the harness
//     gets every tile name in loop order, written or not.
//   - ElevationData.Database.ElvMissingFlag -> (short)-32768.
//   - CompareFilesHGT(string filename1, string filename2) ->
//     public CompareFilesHGT(string filename1, string filename2, string outDir);
//     Application.StartupPath -> outDir; the two MessageBox.Show lines dropped.
//   - ConvertSRTM1ToSRTM3File: private -> public.
//   - Say is a no-op here, so the Say lines stay verbatim.
// Do not fix anything here.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using Newtonsoft.Json;
using ScoutBase.Core;

namespace GoldenHarness
{
    public static class FakeTileClock
    {
        public static DateTime UtcNow => new DateTime(2026, 11, 24, 12, 0, 0, DateTimeKind.Utc).AddTicks(1234567);
    }

    public class UpstreamSQLiteEntry
    {
        public DateTime LastUpdated { get; set; }

        /// <summary>
        /// Converts object to JSON string
        /// </summary>
        /// <returns>JSON string representing the object.</returns>
        public string ToJSON()
        {
            JsonSerializerSettings settings = new JsonSerializerSettings();
            settings.DateTimeZoneHandling = DateTimeZoneHandling.Utc;
            settings.FloatFormatHandling = FloatFormatHandling.String;
            settings.Formatting = Newtonsoft.Json.Formatting.Indented;
            settings.Culture = CultureInfo.InvariantCulture;
            string json = JsonConvert.SerializeObject(this, settings);
            return json;
        }
    }

    [System.ComponentModel.DesignerCategory("")]
    public class UpstreamTileDesignator : UpstreamSQLiteEntry
    {
        [JsonIgnore]
        public static new readonly string TableName = "Elevation";

        /////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

        [JsonIgnore]
        public double BaseLat = 0;
        [JsonIgnore]
        public double BaseLon = 0;

        [JsonIgnore]
        public double StepWidthLat = 0;
        [JsonIgnore]
        public double StepWidthLon = 0;

        [JsonIgnore]
        private string _TileIndex = "";
        public string TileIndex
        {
            get
            {
                return _TileIndex;
            }
            set
            {
                _TileIndex = value;
                BaseLat = MaidenheadLocator.LatFromLoc(value, PositionInRectangle.BottomLeft);
                BaseLon = MaidenheadLocator.LonFromLoc(value, PositionInRectangle.BottomLeft);
            }
        }

        public int MinElv { get; set; }
        public double MinLat { get; set; }
        public double MinLon { get; set; }

        public int MaxElv { get; set; }
        public double MaxLat { get; set; }
        public double MaxLon { get; set; }

        [JsonIgnore]
        private int _Rows = 50;
        public int Rows
        {
            get
            {
                return _Rows;
            }
            set
            {
                _Rows = value;
                StepWidthLat = 1 / 24.0 / value;
            }
        }
        [JsonIgnore]
        private int _Columns = 100;
        public int Columns
        {
            get
            {
                return _Columns;
            }
            set
            {
                _Columns = value;
                StepWidthLon = 2 / 24.0 / value;
            }
        }

        public short[,] Elv { get; set; }

        // MEMBERS ONLY TO STORE VALUES TEMPORARLY --> NOT STORED IN DATABASE
        // locator bounds
        public LatLon.GRect Bounds;


        public UpstreamTileDesignator()
        {
            MinElv = short.MaxValue;
            MinLat = 0;
            MinLon = 0;
            MaxElv = short.MinValue;
            MaxLat = 0;
            MaxLon = 0;
            Elv = new short[Rows, Columns];
            Bounds = new LatLon.GRect(0, 0, 0, 0);
        }
    }

    public class UpstreamTileGen
    {
        public List<string> Visited = new List<string>();

        private void Say(string txt) { }

        public bool IsGLOBEFile(string filename)
        {
            // checks for a valid filename with/witout extension
            // filename must have a xxxx. notation
            // [0]: 'A'..'P'
            // [1],[2]: nn = version
            // [3]: 'G','B','S','T' = status
            // 'S' and 'T' are not used
            if (String.IsNullOrEmpty(filename))
                return false;
            string upperfilename = Path.GetFileNameWithoutExtension(filename).ToUpper();
            // return on extension not empty
            string extension = Path.GetExtension(filename);
            if (!String.IsNullOrEmpty(extension))
                return false;
            char index = upperfilename[0];
            char status = upperfilename[3];
            // check length of filename
            if (upperfilename.Length != 4)
                return false;
            // check allowed chars
            if ((index < 'A')
                || (index > 'P')
                || !Char.IsDigit(upperfilename[1])
                || !Char.IsDigit(upperfilename[2])
                || ((status != 'G') && (status != 'B'))
                )
                return false;
            //check length
            long l = 0;
            switch (index)
            {
                case 'A':
                case 'B':
                case 'C':
                case 'D':
                case 'M':
                case 'N':
                case 'O':
                case 'P':
                    l = 4800 * 10800 * 2;
                    break;
                case 'E':
                case 'F':
                case 'G':
                case 'H':
                case 'I':
                case 'J':
                case 'K':
                case 'L':
                    l = 6000 * 10800 * 2;
                    break;
            }
            if (l != new System.IO.FileInfo(filename).Length)
                return false;
            return true;
        }

        public bool IsSRTM3File(string filename)
        {
            // checks for a valid SRTM filename with extension
            // filename must have a XnnYnnn.hgt notation
            // [0]:         'N' or 'S'
            // [1],[2]:     nn = Latitude
            // [3]:         'E' or 'W'
            // [4],[5],[6]: nnn = Longitude
            if (String.IsNullOrEmpty(filename))
                return false;
            if (Path.GetExtension(filename).ToUpper() != ".HGT")
                return false;
            string upperfilename = Path.GetFileNameWithoutExtension(filename).ToUpper();
            if ((upperfilename[0] != 'N') && (upperfilename[0] != 'S'))
                return false;
            if ((upperfilename[3] != 'W') && (upperfilename[3] != 'E'))
                return false;
            if (!Char.IsDigit(upperfilename[1]))
                return false;
            if (!Char.IsDigit(upperfilename[2]))
                return false;
            if (!Char.IsDigit(upperfilename[4]))
                return false;
            if (!Char.IsDigit(upperfilename[5]))
                return false;
            if (!Char.IsDigit(upperfilename[6]))
                return false;
            // check file size
            long l = 1201 * 1201 * 2;
            if (l != new System.IO.FileInfo(filename).Length)
                return false;
            return true;
        }

        public bool IsSRTM1File(string filename)
        {
            // checks for a valid SRTM filename with extension
            // filename must have a XnnYnnn.hgt notation
            // [0]:         'N' or 'S'
            // [1],[2]:     nn = Latitude
            // [3]:         'E' or 'W'
            // [4],[5],[6]: nnn = Longitude
            if (String.IsNullOrEmpty(filename))
                return false;
            if (Path.GetExtension(filename).ToUpper() != ".HGT")
                return false;
            string upperfilename = Path.GetFileNameWithoutExtension(filename).ToUpper();
            if ((upperfilename[0] != 'N') && (upperfilename[0] != 'S'))
                return false;
            if ((upperfilename[3] != 'W') && (upperfilename[3] != 'E'))
                return false;
            if (!Char.IsDigit(upperfilename[1]))
                return false;
            if (!Char.IsDigit(upperfilename[2]))
                return false;
            if (!Char.IsDigit(upperfilename[4]))
                return false;
            if (!Char.IsDigit(upperfilename[5]))
                return false;
            if (!Char.IsDigit(upperfilename[6]))
                return false;
            // check file size
            long l = 3601 * 3601 * 2;
            if (l != new System.IO.FileInfo(filename).Length)
                return false;
            return true;
        }


        public void ProcessTile(string tilename, string destDir, int oi0, int oj0, int oi1, int oj1)
        {
            if (IsSRTM3File(tilename) || IsSRTM1File(tilename))
            {
                string srtmindex = Path.GetFileNameWithoutExtension(tilename).ToUpper();
                int latbase = (srtmindex[0] == 'S') ? -System.Convert.ToInt32(srtmindex.Substring(1, 2)) : System.Convert.ToInt32(srtmindex.Substring(1, 2));
                int lonbase = (srtmindex[3] == 'W') ? -System.Convert.ToInt32(srtmindex.Substring(4, 3)) : System.Convert.ToInt32(srtmindex.Substring(4, 3));
                string tilebase = MaidenheadLocator.LocFromLatLon(latbase, lonbase, false, 3).Substring(0,4);

                // creating dir
                string dir = Path.Combine(destDir, tilebase.Substring(0, 2), tilebase.Substring(2, 2));
                int rows;
                int cols;
                if (IsSRTM3File(tilename))
                {
                    rows = 1201;
                    cols = 1201;
                }
                else
                {
                    rows = 3601;
                    cols = 3601;
                }
                FileStream fs = File.OpenRead(tilename);
                MemoryStream ms = new MemoryStream();
                ms.SetLength(fs.Length);
                fs.Read(ms.GetBuffer(), 0, (int)fs.Length);
                using (BinaryReader br = new BinaryReader(ms))
                {
                    bool all_void = true;
                    UpstreamTileDesignator tile;
                    char x = (lonbase % 2 == 0) ? 'A' : 'M';
                    for (int i = 0; i < 12; i++)
                    {
                        char y = 'A';
                        for (int j = 0; j < 24; j++)
                        {
                            string filename = tilebase + x + y + ".loc";
                            Visited.Add(tilebase + x + y);
                            if (!File.Exists(Path.Combine(dir, filename)))
                            {
                                tile = new UpstreamTileDesignator();
                                tile.TileIndex = tilebase + x + y;
                                tile.Rows = rows / 24;
                                tile.Columns = cols / 12;
                                short[,] b = new short[tile.Columns, tile.Rows];
                                byte[] a16 = new byte[2];
                                for (int k = 0; k < tile.Columns; k++)
                                {
                                    for (int l = 0; l < tile.Rows; l++)
                                    {
                                        br.BaseStream.Position = ((rows - 2 - j * tile.Rows - l) * cols + i * tile.Columns + k) * 2;
                                        br.Read(a16, 0, 2);
                                        Array.Reverse(a16);
                                        short e = BitConverter.ToInt16(a16, 0);
                                        b[k, l] = e;
                                        if ((e != (short)-32768) && (e < tile.MinElv))
                                        {
                                            tile.MinElv = e;
                                            tile.MinLon = tile.BaseLon + k * tile.StepWidthLon;
                                            tile.MinLat = tile.BaseLat + l * tile.StepWidthLat;
                                        }
                                        if ((e != (short)-32768) && (e > tile.MaxElv))
                                        {
                                            tile.MaxElv = e;
                                            tile.MaxLon = tile.BaseLon + k * tile.StepWidthLon;
                                            tile.MaxLat = tile.BaseLat + l * tile.StepWidthLat;
                                        }
                                        if (e != (short)-32768)
                                            all_void = false;
                                    }
                                }
                                tile.Elv = b;
                                tile.LastUpdated = FakeTileClock.UtcNow;
                                if (!all_void)
                                {
                                    //                                ElevationModel.Database.ElevationTileInsertOrUpdateIfNewer(tile);
                                    // create dir if not exists
                                    if (!Directory.Exists(dir))
                                        Directory.CreateDirectory(dir);
                                    // write json file
                                    ms = new MemoryStream();
                                    using (StreamWriter sw = new StreamWriter(ms))
                                    {
                                        string json = tile.ToJSON();
                                        sw.WriteLine(json);
                                        sw.Flush();
                                        ms.Position = 0;
                                        File.WriteAllBytes(Path.Combine(dir, filename), ms.ToArray());
                                    }
                                    File.SetCreationTimeUtc(Path.Combine(dir, filename), tile.LastUpdated);
                                    File.SetLastWriteTimeUtc(Path.Combine(dir, filename), tile.LastUpdated);
                                }
                            }
                            y++;
                        }
                        x++;
                    }
                }
            }
            else if (IsGLOBEFile(tilename))
            {
                char index = Path.GetFileName(tilename.ToUpper())[0];
                int latbase;
                int lonbase;
                int rows;
                int cols;
                switch (index)
                {
                    case 'A':
                        latbase = 50;
                        lonbase = -180;
                        cols = 10800;
                        rows = 4800;
                        break;
                    case 'B':
                        latbase = 50;
                        lonbase = -90;
                        cols = 10800;
                        rows = 4800;
                        break;
                    case 'C':
                        latbase = 50;
                        lonbase = 0;
                        cols = 10800;
                        rows = 4800;
                        break;
                    case 'D':
                        latbase = 50;
                        lonbase = 90;
                        cols = 10800;
                        rows = 4800;
                        break;
                    case 'E':
                        latbase = 0;
                        lonbase = -180;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'F':
                        latbase = 0;
                        lonbase = -90;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'G':
                        latbase = 0;
                        lonbase = 0;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'H':
                        latbase = 0;
                        lonbase = 90;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'I':
                        latbase = -50;
                        lonbase = -180;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'J':
                        latbase = -50;
                        lonbase = -90;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'K':
                        latbase = -50;
                        lonbase = 0;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'L':
                        latbase = -50;
                        lonbase = 90;
                        cols = 10800;
                        rows = 6000;
                        break;
                    case 'M':
                        latbase = -90;
                        lonbase = -180;
                        cols = 10800;
                        rows = 4800;
                        break;
                    case 'N':
                        latbase = -90;
                        lonbase = -90;
                        cols = 10800;
                        rows = 4800;
                        break;
                    case 'O':
                        latbase = -90;
                        lonbase = -0;
                        cols = 10800;
                        rows = 4800;
                        break;
                    case 'P':
                        latbase = -90;
                        lonbase = 90;
                        cols = 10800;
                        rows = 4800;
                        break;
                    default:
                        latbase = 0;
                        lonbase = 0;
                        cols = 0;
                        rows = 0;
                        break;
                }

                string tilebase = MaidenheadLocator.LocFromLatLon(latbase, lonbase, false, 3).Substring(0, 4);
                FileStream fs = File.OpenRead(tilename);
                MemoryStream ms = new MemoryStream();
                ms.SetLength(fs.Length);
                fs.Read(ms.GetBuffer(), 0, (int)fs.Length);
                using (BinaryReader br = new BinaryReader(ms))
                {
                    UpstreamTileDesignator tile;
                    for (int i0 = oi0; i0 <= oi0; i0++)
                    {
                        int j0max = (rows == 6000) ? 4 : 3;
                        for (int j0 = oj0; j0 <= oj0; j0++)
                        {
                            int i1max = (i0 < 4) ? 9 : 4;
                            for (int i1 = oi1; i1 <= oi1; i1++)
                            {
                                for (int j1 = oj1; j1 <= oj1; j1++)
                                {
                                    for (int i2 = 0; i2 < 24; i2++)
                                    {
                                        for (int j2 = 0; j2 < 24; j2++)
                                        {
                                            // generate lat/lon from tile's midpoint
                                            double lat = latbase + j0 * 10.0 + j1 * 1.0 + j2 * 1.0 / 24.0 + 0.5 / 24.0;
                                            double lon = lonbase + i0 * 20.0 + i1 * 2.0 + i2 * 2.0 / 24.0 + 1.0 / 24.0;
                                            // generate filename from lat/lon
                                            string tileindex = MaidenheadLocator.LocFromLatLon(lat, lon, false, 3);
                                            string filename = tileindex + ".loc";
                                            Visited.Add(tileindex);
                                            // creating dir
                                            string dir = Path.Combine(destDir, filename.Substring(0, 2), filename.Substring(2,2));
                                            if (!File.Exists(Path.Combine(dir, filename)))
                                            {
                                                bool all_void = true;
                                                tile = new UpstreamTileDesignator();
                                                tile.TileIndex = tileindex;
                                                tile.Rows = (rows == 6000) ? rows / 50 / 24 : rows / 40 / 24;
                                                tile.Columns = cols / 90 / 24 * 2;
                                                short[,] b = new short[tile.Columns, tile.Rows];
                                                for (int k = 0; k < tile.Columns; k++)
                                                {
                                                    for (int l = 0; l < tile.Rows; l++)
                                                    {
                                                        br.BaseStream.Position = ((rows - 1 - j0 * 10 * 24 * tile.Rows - j1 * 24 * tile.Rows - j2 * tile.Rows - l) * cols + i0 * 10 * 24 * tile.Columns + i1 * 24 * tile.Columns + i2 * tile.Columns + k) * 2;
                                                        short e = br.ReadInt16();
                                                        if (e <= -500)
                                                            e = (short)-32768;
                                                        b[k, l] = e;
                                                        if ((e != (short)-32768) && (e < tile.MinElv))
                                                        {
                                                            tile.MinElv = e;
                                                            tile.MinLon = tile.BaseLon + k * tile.StepWidthLon;
                                                            tile.MinLat = tile.BaseLat + l * tile.StepWidthLat;
                                                        }
                                                        if ((e != (short)-32768) && (e > tile.MaxElv))
                                                        {
                                                            tile.MaxElv = e;
                                                            tile.MaxLon = tile.BaseLon + k * tile.StepWidthLon;
                                                            tile.MaxLat = tile.BaseLat + l * tile.StepWidthLat;
                                                        }
                                                        if (e != (short)-32768)
                                                            all_void = false;
                                                    }
                                                }
                                                tile.Elv = b;
                                                tile.LastUpdated = FakeTileClock.UtcNow;
                                                if (!all_void)
                                                {
                                                    //                                ElevationModel.Database.ElevationTileInsertOrUpdateIfNewer(tile);
                                                    // create dir if not exists
                                                    try
                                                    {
                                                        if (!Directory.Exists(dir))
                                                            Directory.CreateDirectory(dir);
                                                    }
                                                    catch (Exception ex)
                                                    {

                                                    }
                                                    // write json file
                                                    ms = new MemoryStream();
                                                    using (StreamWriter sw = new StreamWriter(ms))
                                                    {
                                                        string json = tile.ToJSON();
                                                        sw.WriteLine(json);
                                                        sw.Flush();
                                                        ms.Position = 0;
                                                        File.WriteAllBytes(Path.Combine(dir, filename), ms.ToArray());
                                                    }
                                                    File.SetCreationTimeUtc(Path.Combine(dir, filename), tile.LastUpdated);
                                                    File.SetLastWriteTimeUtc(Path.Combine(dir, filename), tile.LastUpdated);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        public string CompareFilesHGT(string filename1, string filename2, string outDir)
        {
            string difffilename = Path.Combine(outDir, Path.GetFileName(filename1).Replace(".hgt",".hgtdiff"));
            long length1 = new System.IO.FileInfo(filename1).Length / 2;
            long length2 = new System.IO.FileInfo(filename2).Length / 2;
            if (length1 != length2)
            {
                return "";
            }
            long l = 0;
            long diff = 0;
            // delete diff file
            try
            {
                File.Delete(difffilename);
                FileStream fs1 = File.OpenRead(filename1);
                FileStream fs2 = File.OpenRead(filename2);
                using (BinaryReader br1 = new BinaryReader(fs1))
                {
                    using (BinaryReader br2 = new BinaryReader(fs2))
                    {
                        using (BinaryWriter bw = new BinaryWriter(File.Create(difffilename)))
                        {
                            while (br1.BaseStream.Position < br1.BaseStream.Length)
                            {
                                byte[] b1 = br1.ReadBytes(2);
                                byte[] b2 = br2.ReadBytes(2);
                                Array.Reverse(b1);
                                Array.Reverse(b2);
                                short e1 = BitConverter.ToInt16(b1, 0);
                                short e2 = BitConverter.ToInt16(b2, 0);
                                short e = (short)-32768;
                                if ((e1 >= -500) && (e2 >= -500))
                                {
                                    e = (short)(e1 - e2);
                                    if (e > 1000)
                                        Console.WriteLine("Error at position:" + br1.BaseStream.Position.ToString("0X"));
                                    if (e != 0)
                                        diff++;
                                }
                                bw.Write(e);
                                if (l % 100000 == 0)
                                {
                                    Say("Comparing point " + l.ToString() + " of " + length1.ToString() + ": " + diff.ToString() + " differences");
                                }
                                l++;
                            }
                        }
                    }
                }
            }
            catch (Exception ex)
            {
            }

            return difffilename;
        }

        private short GetSRTM1Elevation(BinaryReader br, int x, int y)
        {
            int rows = 3601;
            int cols = 3601;
            long l = (x * cols + y) * 2;
            br.BaseStream.Seek(l,0);
            byte[] b = br.ReadBytes(2);
            Array.Reverse(b);
            short e = BitConverter.ToInt16(b, 0);
            return e;
        }
        public void ConvertSRTM1ToSRTM3File(string srtm1file, string srtm3file)
        {
            Say("Processing " + srtm1file);
            using (BinaryReader br = new BinaryReader(File.OpenRead(srtm1file)))
            {
                using (BinaryWriter bw = new BinaryWriter(File.Create(srtm3file)))
                {
                    short e = 0;
                    for (int i = 0; i < 1200; i++)
                    {
                        for (int j = 0; j < 1200; j++)
                        {
                            e = 0;
                            for (int k = 0; k < 3; k++)
                            {
                                for (int l = 0; l < 3; l++)
                                {
                                    short elv = GetSRTM1Elevation(br, i * 3 + k, j * 3 + l);
                                    e += elv;
                                }
                            }
                            e = (short)(e / 9);
                            bw.BaseStream.WriteByte((byte)((e >> 8) & 0xFF));
                            bw.BaseStream.WriteByte((byte)(e & 0xFF));
                        }
                        // write point #1201
                        bw.BaseStream.WriteByte((byte)((e >> 8) & 0xFF));
                        bw.BaseStream.WriteByte((byte)(e & 0xFF));
                    }
                    for (int j = 0; j < 1201; j++)
                    {
                        e= GetSRTM1Elevation(br, 3600, j * 3);
                        bw.BaseStream.WriteByte((byte)((e >> 8) & 0xFF));
                        bw.BaseStream.WriteByte((byte)(e & 0xFF));
                    }
                }
            }
            Say("Ready.");
        }
    }
}
