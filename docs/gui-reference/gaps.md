# GUI 1:1 gap inventory

Upstream pinned at `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`. For every control of every upstream window:
is there Rust logic behind it?

Rows: every checked control (visible, non-empty text, kind in the fidelity list),
every control with a subscribed C# event, and every settings field
(`ComboBox`/`TextBox`/`NumericUpDown`/`CheckBox`). The `Rust` column is
`ok <path>:<line>` where the Rust code has the same effect, `missing` where it
does not, or `not ported: <reason>` for the spec's non-goals only.

## AircraftDatabaseMaintenanceDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| tsl_Status | ToolStripStatusLabel | Status |  | ok ui/dialogs/db_maintenance.rs:154 |  |
| groupBox1 | GroupBox | Tables |  | ok ui/dialogs/db_maintenance.rs:136 |  |
| ch_TableName | ColumnHeader | TableName |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Description | ColumnHeader | Description |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Entries | ColumnHeader | Entries |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Action | ColumnHeader | Action |  | ok ui/dialogs/db_maintenance.rs:145 |  |
| lbl_Info | Label | The Scoutbase database is containing location and QRV infos. Each information has a unique key: callsign and 6-digit Maidenhead locator square.  The database is periodically updated from a global web database. The operator can modify/add entries. |  | ok ui/dialogs/db_maintenance.rs:131 |  |

## CleanupDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| label1 | Label | This will cleanup your AirScout database and user settings. Please choose which parts should be erased: |  | ok ui/dialogs/cleanup.rs:12 |  |
| cb_UserSettings | CheckBox | User Settings |  | ok ui/dialogs/cleanup.rs:19 |  |
| cb_AirScout_Database | CheckBox | AirScout Database (Aircraft data and positions) |  | ok ui/dialogs/cleanup.rs:20 |  |
| cb_ScoutBase_Database | CheckBox | ScoutBase Database (Elevation, propagation and station data) |  | ok ui/dialogs/cleanup.rs:24 |  |
| cb_LogFiles | CheckBox | Log Directory |  | missing | The port has no Log directory; the choice is not offered. |
| cb_ElevationFiles | CheckBox | Elevation Directory (up to V1.2.0.7) |  | missing | The legacy elevation directory is not part of the port. |
| cb_TmpFiles | CheckBox | Tmp Directory |  | ok ui/dialogs/cleanup.rs:28 |  |
| btn_OK | Button | Continue | Click: btn_OK_Click | ok cleanup.rs:179 |  |
| btn_Cancel | Button | Cancel | Click: btn_Cancel_Click | ok cleanup.rs:224 |  |
| cb_PluginFiles | CheckBox | Plugin Directory |  | not ported: plugin download | Plugin support is a spec non-goal. |

## CrossingHistoryDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| tsl_Main | ToolStripStatusLabel | Press Calculate to start... |  | ok ui/dialogs/crossing_history.rs:136 |  |
| label2 | Label | secs. |  | ok ui/dialogs/crossing_history.rs:111 |  |
| label1 | Label | Timespan within two crossings considered as ambiguous: |  | ok ui/dialogs/crossing_history.rs:108 |  |
| ud_Analysis_AmbiguousGap | NumericUpDown |  |  | ok ui/dialogs/crossing_history.rs:109 |  |
| cb_Analysis_CrossingHistory_WithSignlaLevel | CheckBox | Consider crossings with recorded signal level only |  | ok ui/dialogs/crossing_history.rs:103 |  |
| btn_History_Export | Button | Export to CSV | Click: btn_History_Export_Click | ok ui/dialogs/crossing_history.rs:120 |  |
| btn_History_Cancel | Button | Back | Click: btn_History_Cancel_Click | ok ui/dialogs/crossing_history.rs:126 |  |
| btn_History_Calculate | Button | Calculate | Click: btn_History_Calculate_Click | ok ui/dialogs/crossing_history.rs:115 |  |

## DatabaseEditDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| tsl_Status | ToolStripStatusLabel | Status |  | ok ui/dialogs/db_maintenance.rs:193 |  |
| dgv_Main | DataGridView |  | CellValidating: dgv_Main_CellValidating, DataError: dgv_Main_DataError, RowsAdded: dgv_Main_RowsAdded, RowValidating: dgv_Main_RowValidating, KeyDown: dgv_Main_KeyDown | missing | The port shows the first rows read-only; cell edit and validation are not reproduced. |
| label1 | Label | Find in any column: |  | ok ui/dialogs/db_maintenance.rs:184 |  |
| tb_Search | TextBox |  | TextChanged: tb_Search_TextChanged, KeyDown: tb_Search_KeyDown | ok ui/dialogs/db_maintenance.rs:186 |  |
| btn_FindNext | Button | Find next (F3) | Click: btn_FindNext_Click, KeyDown: btn_FindNext_KeyDown | ok ui/dialogs/db_maintenance.rs:190 |  |

## DatabaseMaintenanceDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| lbl_Description | Label | Database Description |  | ok ui/dialogs/db_maintenance.rs:131 |  |
| groupBox1 | GroupBox | Tables |  | ok ui/dialogs/db_maintenance.rs:136 |  |
| ch_TableName | ColumnHeader | TableName |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Description | ColumnHeader | Description |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Entries | ColumnHeader | Entries |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Action1 | ColumnHeader | Action 1 |  | ok ui/dialogs/db_maintenance.rs:145 |  |
| tsl_Status | ToolStripStatusLabel | Status |  | ok ui/dialogs/db_maintenance.rs:154 |  |
| ch_Action2 | ColumnHeader | Action 2 |  | ok ui/dialogs/db_maintenance.rs:148 |  |

## DeleteSingleStationDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| label1 | Label | Use this dialog to find and remove a single station/locator combination from the local station database.  CAUTION: You can only remove own entries permanently. If a station/locator combination is part of the global database it would be re-inserted on next update. |  | ok ui/dialogs/delete_station.rs:79 |  |
| cb_Call | ComboBox |  | DropDown: cb_Call_DropDown, SelectedIndexChanged: cb_Call_SelectedIndexChanged, TextChanged: cb_Call_TextChanged, KeyPress: cb_Call_KeyPress | ok ui/dialogs/delete_station.rs:86 |  |
| cb_Locator | ComboBox |  |  | ok ui/dialogs/delete_station.rs:113 |  |
| label2 | Label | Call: |  | ok ui/dialogs/delete_station.rs:84 |  |
| label3 | Label | Locator: |  | ok ui/dialogs/delete_station.rs:111 |  |
| btn_Delete | Button | Delete |  | ok ui/dialogs/delete_station.rs:130 |  |
| btn_Cancel | Button | Cancel |  | ok ui/dialogs/delete_station.rs:131 |  |

## ElevationCopyrightDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|

## FirstRunWizard

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| wp_TermsAndConditions | WizardPage | Agree to the Terms and C. | Enter: wp_TermsAndConditions_Enter | ok ui/dialogs/wizard.rs:226 |  |
| cb_TermsAndConditions | CheckBox | I agree with the terms and conditions above. | CheckedChanged: cb_TermsAndConditions_CheckedChanged | ok ui/dialogs/wizard.rs:228 |  |
| wp_GeneralCoverage | WizardPage | Select the Covered Area | Enter: wp_GeneralCoverage_Enter | ok ui/dialogs/wizard.rs:233 |  |
| label6 | Label | CAUTION: Select area of interest just as large to cover a range of 1000-2000km around your qth. Larger areas together with SRTM3 and SRTM1 elevation data will result in extensive disk space usage! |  | ok ui/dialogs/wizard.rs:234 |  |
| label17 | Label | Select a covered area of your interest by entering lat/long values in the numeric boxes below. |  | ok ui/dialogs/wizard.rs:234 |  |
| label4 | Label | Max. Latitude (-90 ... + 90): |  | ok ui/dialogs/wizard.rs:238 |  |
| label3 | Label | Min. Latitude (-90 ... +90): |  | ok ui/dialogs/wizard.rs:237 |  |
| ud_MaxLat | NumericUpDown |  | ValueChanged: ud_MaxLat_ValueChanged | ok ui/dialogs/wizard.rs:238 |  |
| ud_MinLat | NumericUpDown |  | ValueChanged: ud_MinLat_ValueChanged | ok ui/dialogs/wizard.rs:237 |  |
| ud_MaxLon | NumericUpDown |  | ValueChanged: ud_MaxLon_ValueChanged | ok ui/dialogs/wizard.rs:240 |  |
| label2 | Label | Max. Longitude (-180 ... +180): |  | ok ui/dialogs/wizard.rs:240 |  |
| label1 | Label | Min. Longitude (-180 ...  +180): |  | ok ui/dialogs/wizard.rs:239 |  |
| ud_MinLon | NumericUpDown |  | ValueChanged: ud_MinLon_ValueChanged | ok ui/dialogs/wizard.rs:239 |  |
| gm_Coverage | GMapControl |  | Enter: gm_Coverage_Enter | missing | The wizard has no coverage map; the area is entered numerically. |
| wp_ElevationModel | WizardPage | Elevation Model | Enter: wp_ElevationModel_Enter | ok ui/dialogs/wizard.rs:243 |  |
| cb_ASTER1 | CheckBox | Use ASTER1 Elevation Model (experimental) | CheckedChanged: cb_ASTER1_CheckedChanged | ok ui/dialogs/wizard.rs:253 |  |
| cb_ASTER3 | CheckBox | Use ASTER3 Elevation Model (experimental) | CheckedChanged: cb_ASTER3_CheckedChanged | ok ui/dialogs/wizard.rs:252 |  |
| cb_SRTM1 | CheckBox | Use SRTM1 Elevation Model (not recommended) | CheckedChanged: cb_SRTM1_CheckedChanged | ok ui/dialogs/wizard.rs:251 |  |
| cb_SRTM3 | CheckBox | Use SRTM3 Elevation Model (optional) | CheckedChanged: cb_SRTM3_CheckedChanged | ok ui/dialogs/wizard.rs:250 |  |
| cb_GLOBE | CheckBox | Use GLOBE Elevation Model (strongly recommended) | CheckedChanged: cb_GLOBE_CheckedChanged | ok ui/dialogs/wizard.rs:249 |  |
| wp_GLOBE | WizardPage | GLOBE | Commit: wp_GLOBE_Commit, Enter: wp_GLOBE_Enter, Leave: wp_GLOBE_Leave | missing | No per-model download page in the wizard. |
| lbl_GLOBE_Status | Label | Status |  | missing | No per-model download page in the wizard. |
| label5 | Label | (G)lobal (L)and (O)ne-km (B)ase (E)levation Project |  | missing | No per-model download page in the wizard. |
| cb_GLOBE_EnableCache | CheckBox | Keep downloaded elevation tiles in cache |  | missing | Elevation tiles are always cached; there is no toggle. |
| wp_SRTM3 | WizardPage | SRTM3 | Enter: wp_SRTM3_Enter, Leave: wp_SRTM3_Leave | missing | No per-model download page in the wizard. |
| lbl_SRTM3_Status | Label | Status |  | missing | No per-model download page in the wizard. |
| label21 | Label | (S)huttle (R)adar (T)opography (M)ission 3 arcsec |  | missing | No per-model download page in the wizard. |
| cb_SRTM3_EnableCache | CheckBox | Keep downloaded elevation tiles in cache |  | missing | Elevation tiles are always cached; there is no toggle. |
| wp_SRTM1 | WizardPage | SRTM1 | Enter: wp_SRTM1_Enter, Leave: wp_SRTM1_Leave | missing | No per-model download page in the wizard. |
| lbl_SRTM1_Status | Label | Status |  | missing | No per-model download page in the wizard. |
| label7 | Label | (S)huttle (R)adar (T)opography (M)ission 1 arcsec |  | missing | No per-model download page in the wizard. |
| cb_SRTM1_EnableCache | CheckBox | Keep downloaded elevation tiles in cache |  | missing | Elevation tiles are always cached; there is no toggle. |
| wp_ASTER3 | WizardPage | ASTER3 | Enter: wp_ASTER3_Enter, Leave: wp_ASTER3_Leave | missing | No per-model download page in the wizard. |
| lbl_ASTER3_Status | Label | Status |  | missing | No per-model download page in the wizard. |
| label23 | Label | (A)dvanced (S)paceborne (T)hermal (E)mission and (R)eflection 3 arcsec |  | missing | No per-model download page in the wizard. |
| cb_ASTER3_EnableCache | CheckBox | Keep downloaded elevation tiles in cache |  | missing | Elevation tiles are always cached; there is no toggle. |
| wp_ASTER1 | WizardPage | ASTER1 | Enter: wp_ASTER1_Enter, Leave: wp_ASTER1_Leave | missing | No per-model download page in the wizard. |
| lbl_ASTER1_Status | Label | Status |  | missing | No per-model download page in the wizard. |
| label25 | Label | (A)dvanced (S)paceborne (T)hermal (E)mission and (R)eflection 1 arcsec |  | missing | No per-model download page in the wizard. |
| cb_ASTER1_EnableCache | CheckBox | Keep downloaded elevation tiles in cache |  | missing | Elevation tiles are always cached; there is no toggle. |
| wp_UserDetails | WizardPage | User Details | Enter: wp_UserDetails_Enter | ok ui/dialogs/wizard.rs:255 |  |
| label19 | Label | Update your user details here. Exact lat/long values are essential for calculation of the radio horizon. Start with entering a valid callsign into the callsign box. Use the lat/lon textboxes for numeric input or enter a valid Maidenhead Locator. Drag the needle on the map for exact location. Use mouse wheel to zoom in and out. |  | ok ui/dialogs/wizard.rs:273 |  |
| label18 | Label | Map Zoom: |  | missing | The wizard user page has no map. |
| tb_Zoom | TextBox |  |  | missing | The wizard user page has no map. |
| btn_Zoom_Out | Button | - | Click: btn_Zoom_Out_Click | missing | The wizard user page has no map. |
| btn_Zoom_In | Button | + | Click: btn_Zoom_In_Click | missing | The wizard user page has no map. |
| btn_QRZ | Button | QRZ lookup | Click: btn_QRZ_Click | missing | QRZ lookup is not ported. |
| label11 | Label | Locator: |  | ok ui/dialogs/wizard.rs:281 |  |
| label10 | Label | Longitude: |  | ok ui/dialogs/wizard.rs:286 |  |
| label9 | Label | Latitude: |  | ok ui/dialogs/wizard.rs:285 |  |
| label8 | Label | Callsign: |  | ok ui/dialogs/wizard.rs:277 |  |
| gm_Callsign | GMapControl |  | OnMarkerEnter: gm_Callsign_OnMarkerEnter, OnMapZoomChanged: gm_Callsign_OnMapZoomChanged, MouseDown: gm_Callsign_MouseDown, MouseMove: gm_Callsign_MouseMove, MouseUp: gm_Callsign_MouseUp | missing | The wizard user page has no map. |
| tb_Callsign | CallsignTextBox |  | TextChanged: tb_Callsign_TextChanged | ok ui/dialogs/wizard.rs:278 |  |
| tb_Locator | LocatorTextBox |  | TextChanged: tb_Locator_TextChanged | ok ui/dialogs/wizard.rs:282 |  |
| tb_Longitude | DoubleTextBox |  | TextChanged: tb_Longitude_TextChanged | ok ui/dialogs/wizard.rs:286 |  |
| tb_Latitude | DoubleTextBox |  | TextChanged: tb_Latitude_TextChanged | ok ui/dialogs/wizard.rs:285 |  |
| wp_PlaneFeeds | WizardPage | Select a Plane Feed | Enter: wp_PlaneFeeds_Enter | ok ui/dialogs/wizard.rs:256 |  |
| label20 | Label | Plane feeds are necessary to get aircraft positions in real time. You can choose between web feeds, local feeds or dummy feed only for demonstration. You can combine up to three different feeds.  Please enter at least one plane feed to continue. |  | ok ui/dialogs/wizard.rs:257 |  |
| label16 | Label | Plane Feed 3: |  | missing | The wizard offers only feed 1. |
| label15 | Label | Plane Feed 2: |  | missing | The wizard offers only feed 1. |
| label14 | Label | Plane Feed 1: |  | ok ui/dialogs/wizard.rs:259 |  |
| cb_PlaneFeed3 | ComboBox |  | SelectedIndexChanged: cb_PlaneFeed3_SelectedIndexChanged | missing | The wizard offers only feed 1. |
| cb_PlaneFeed2 | ComboBox |  | SelectedIndexChanged: cb_PlaneFeed2_SelectedIndexChanged | missing | The wizard offers only feed 1. |
| cb_PlaneFeed1 | ComboBox |  | SelectedIndexChanged: cb_PlaneFeed1_SelectedIndexChanged | ok ui/dialogs/wizard.rs:259 |  |
| wp_Finish | WizardPage | Finish | Enter: wp_Finish_Enter | ok ui/dialogs/wizard.rs:265 |  |
| lbl_Finish | Label | Finish statement. |  | ok ui/dialogs/wizard.rs:266 |  |
| tsl_Status | ToolStripStatusLabel | Status. |  | ok ui/dialogs/wizard.rs:191 |  |

## HistoryFromURLDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| label1 | Label | Download from: |  | ok ui/dialogs/history_url.rs:105 |  |
| label2 | Label | Date: |  | ok ui/dialogs/history_url.rs:114 |  |
| btn_History_Start | Button | Start |  | ok ui/dialogs/history_url.rs:129 |  |
| label3 | Label | Use this dialog to download one day of plane positions history and convert it to AirScout's database  import file format. |  | ok ui/dialogs/history_url.rs:101 |  |
| label4 | Label | CAUTION: This will download a large file (abt. 8GB) and will use abt. 50GB of temporary disk space! |  | ok ui/dialogs/history_url.rs:123 |  |
| label5 | Label | Directory: |  | ok ui/dialogs/history_url.rs:108 |  |
| tb_History_Select | Button | Select | Click: tb_History_Select_Click | missing | The directory is typed; there is no browse dialog. |
| btn_History_Cancel | Button | Cancel |  | ok ui/dialogs/history_url.rs:135 |  |
| label6 | Label | Date: |  | ok ui/dialogs/history_url.rs:114 |  |
| ud_History_Stepwidth | NumericUpDown |  |  | ok ui/dialogs/history_url.rs:119 |  |
| tb_History_Directory | TextBox |  |  | ok ui/dialogs/history_url.rs:110 |  |
| tb_History_URL | TextBox |  |  | ok ui/dialogs/history_url.rs:106 |  |

## HorizonDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| label1 | Label | Latitude: |  | ok ui/dialogs/horizon.rs:233 |  |
| tb_Horizon_Lat | TextBox |  |  | ok ui/dialogs/horizon.rs:234 |  |
| tb_Horizon_Lon | TextBox |  |  | ok ui/dialogs/horizon.rs:236 |  |
| label2 | Label | Longiitude: |  | ok ui/dialogs/horizon.rs:235 |  |
| tb_Horizon_Elevation | TextBox |  |  | ok ui/dialogs/horizon.rs:239 |  |
| label3 | Label | Elevation: |  | ok ui/dialogs/horizon.rs:238 |  |
| tb_Horizon_K_Factor | TextBox |  |  | ok ui/dialogs/horizon.rs:244 |  |
| label4 | Label | K-Factor: |  | ok ui/dialogs/horizon.rs:243 |  |
| tb_Horizon_QRG | TextBox |  |  | ok ui/dialogs/horizon.rs:246 |  |
| label5 | Label | QRG: |  | ok ui/dialogs/horizon.rs:245 |  |
| tb_Horizon_F1_Clearance | TextBox |  |  | ok ui/dialogs/horizon.rs:249 |  |
| label6 | Label | Fresnel Zone F1-Clearance: |  | ok ui/dialogs/horizon.rs:248 |  |
| tb_Horizon_ElevationModel | TextBox |  |  | ok ui/dialogs/horizon.rs:251 |  |
| label7 | Label | Elevation Model |  | ok ui/dialogs/horizon.rs:250 |  |
| gb_Parameter | GroupBox | Parameters |  | ok ui/dialogs/horizon.rs:232 |  |
| label8 | Label | Antenna Height: |  | ok ui/dialogs/horizon.rs:240 |  |
| tb_Horizon_Height | TextBox |  |  | ok ui/dialogs/horizon.rs:241 |  |
| tsl_Main | ToolStripStatusLabel | Press Calculate to begin calculation. |  | ok ui/dialogs/horizon.rs:201 |  |
| btn_Horizon_Cancel | Button | Back | Click: btn_Horizon_Cancel_Click | ok ui/dialogs/horizon.rs:217 |  |
| btn_Horizon_Export | Button | Export to CSV | Click: btn_Horizon_Export_Click | ok ui/dialogs/horizon.rs:212 |  |
| btn_Horizon_Calculate | Button | Calculate | Click: btn_Horizon_Calculate_Click | ok ui/dialogs/horizon.rs:205 |  |
| gb_Distance | GroupBox | Horizon Distance versus True North [km] |  | ok ui/dialogs/horizon.rs:270 |  |
| gb_Options | GroupBox | Plot Options |  | ok ui/dialogs/horizon.rs:193 |  |
| rb_Horizon_Plot_Map | RadioButton | Map | Click: rb_Horizon_Plot_Map_Click | missing | The map view of the horizon is not ported. |
| rb_Horizon_Plot_Polar | RadioButton | Polar | Click: rb_Horizon_Plot_Polar_Click | ok ui/dialogs/horizon.rs:194 |  |
| rb_Horizon_Plot_Cartesian | RadioButton | Cartesian | Click: rb_Horizon_Plot_Cartesian_Click | ok ui/dialogs/horizon.rs:195 |  |
| gm_Horizon | GMapControl |  | MouseDown: gm_Horizon_MouseDown, MouseUp: gm_Horizon_MouseUp | missing | The map view of the horizon is not ported. |
| gb_Elevation | GroupBox | Minimum Elevation Angle versus True North [deg] |  | ok ui/dialogs/horizon.rs:265 |  |

## LicenseDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| btn_Options_License_OK | Button | Close |  | ok ui/options/mod.rs:484 |  |

## LocalObstructionsDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| btn_Cancel | Button | Cancel |  | ok ui/dialogs/obstructions.rs:88 |  |
| btn_OK | Button | OK |  | ok ui/dialogs/obstructions.rs:78 |  |
| btn_ClearAll | Button | Clear All | Click: btn_ClearAll_Click | ok ui/dialogs/obstructions.rs:83 |  |
| label1 | Label | Fill in your local obstructions here. You can set obe obstruction per degree azimuth. Please enter the distance to obstruction and the obstructions' height above ground. |  | ok ui/dialogs/obstructions.rs:69 |  |

## MapDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| sc_Map | SplitContainer |  | SplitterMoved: sc_Map_SplitterMoved | ok ui/mod.rs:35 |  |
| tc_Map | TabControl |  | Selecting: tc_Map_Selecting | missing | No Map/News tab: the map is always central and the News tab is dropped. |
| tp_Map | TabPage | Map | Enter: tp_Map_Enter | ok ui/mod.rs:60 |  |
| gm_Main | GMapControl |  | OnMarkerClick: gm_Main_OnMarkerClick, OnMarkerEnter: gm_Main_OnMarkerEnter, OnMarkerLeave: gm_Main_OnMarkerLeave, OnPositionChanged: gm_Main_OnPositionChanged, OnTileLoadComplete: gm_Main_OnTileLoadComplete, OnMapDrag: gm_Main_OnMapDrag, OnMapZoomChanged: gm_Main_OnMapZoomChanged, SizeChanged: gm_Main_SizeChanged, Paint: gm_Main_Paint, MouseDown: gm_Main_MouseDown, MouseMove: gm_Main_MouseMove, MouseUp: gm_Main_MouseUp | ok ui/map.rs:169 |  |
| tp_News | TabPage | Latest News | Enter: tp_News_Enter | missing | The news feed is not ported. |
| tc_Main | TabControl |  | DrawItem: tc_Main_DrawItem, SelectedIndexChanged: tc_Main_SelectedIndexChanged, Selecting: tc_Main_Selecting | ok ui/mod.rs:65 |  |
| tp_Elevation | TabPage | Path Profile | Enter: tp_Elevation_Enter, Resize: tp_Elevation_Resize | ok ui/mod.rs:67 |  |
| tp_Spectrum | TabPage | Spectrum | Enter: tp_Spectrum_Enter, Resize: tp_Spectrum_Resize | ok ui/mod.rs:69 |  |
| gb_Spectrum_NearestInfo | GroupBox | Nearest Plane Info |  | ok ui/spectrum.rs:35 |  |
| lbl_Nearest_Dist | Label | 0 |  | ok ui/planes.rs:144 |  |
| label22 | Label | Distance: |  | ok ui/planes.rs:144 |  |
| lbl_Nearest_Call | Label | 0 |  | ok ui/planes.rs:139 |  |
| label21 | Label | Call: |  | ok ui/planes.rs:139 |  |
| lbl_Nearest_Angle | Label | 0 |  | ok ui/planes.rs:143 |  |
| label7 | Label | Angle: |  | ok ui/planes.rs:143 |  |
| lbl_Nearest_Alt | Label | 0 |  | ok ui/planes.rs:142 |  |
| label4 | Label | Altitude: |  | ok ui/planes.rs:142 |  |
| lbl_Nearest_Cat | Label | 0 |  | ok ui/planes.rs:141 |  |
| label5 | Label | Category: |  | ok ui/planes.rs:141 |  |
| lbl_Nearest_Type | Label | 0 |  | ok ui/planes.rs:140 |  |
| label2 | Label | Type: |  | ok ui/planes.rs:140 |  |
| gb_Spectrum_Status | GroupBox | Status |  | ok ui/spectrum.rs:46 |  |
| tb_Spectrum_Status | TextBox |  |  | ok ui/spectrum.rs:46 |  |
| gb_NearestPlaneMap | GroupBox | Nearest Plane Map |  | missing | The nearest-plane map overlay (gm_Nearest) is not ported. |
| gb_Spectrum | GroupBox | Spectrum |  | ok ui/spectrum.rs:55 |  |
| tp_Analysis | TabPage | Analysis | Enter: tp_Analysis_Enter, Leave: tp_Analysis_Leave | ok ui/mod.rs:70 |  |
| gb_Analysis_Player | GroupBox | Player | SizeChanged: gb_Analysis_Player_SizeChanged | ok ui/analysis.rs:393 |  |
| dtp_Analysis_MaxValue | DateTimePicker |  | ValueChanged: dtp_Analysis_MaxValue_ValueChanged | ok ui/analysis.rs:436 |  |
| dtp_Analysis_MinValue | DateTimePicker |  | ValueChanged: dtp_Analysis_MinValue_ValueChanged | ok ui/analysis.rs:433 |  |
| sb_Analysis_Play | ScrollBarEx |  | Scroll: sb_Analysis_Play_Scroll, SizeChanged: sb_Analysis_Play_SizeChanged | ok ui/analysis.rs:465 |  |
| gb_Analysis_Controls | GroupBox | Controls |  | ok ui/analysis.rs:402 |  |
| btn_Analysis_FastForward | Button |  | Click: btn_Analysis_FastForward_Click | ok ui/analysis.rs:408 |  |
| btn_Analysis_Forward | Button |  | Click: btn_Analysis_Forward_Click | ok ui/analysis.rs:407 |  |
| btn_Analysis_Pause | Button |  | Click: btn_Analysis_Pause_Click | ok ui/analysis.rs:406 |  |
| btn_Analysis_Back | Button |  | Click: btn_Analysis_Back_Click | ok ui/analysis.rs:405 |  |
| btn_Analysis_Rewind | Button |  | Click: btn_Analysis_Rewind_Click | ok ui/analysis.rs:404 |  |
| gb_Analysis_Functions | GroupBox | Functions |  | ok ui/analysis.rs:294 |  |
| btn_Analysis_Plane_History | Button | Export Plane History | Click: btn_Analysis_Plane_History_Click | ok ui/analysis.rs:316 |  |
| btn_Analysis_CrossingHistory | Button | Crossing History | Click: btn_Analysis_CrossingHistory_Click | ok ui/analysis.rs:310 |  |
| btn_Analysis_Path_SaveToFile | Button | Export Path | Click: btn_Analysis_Path_SaveToFile_Click | ok ui/analysis.rs:304 |  |
| btn_Analysis_Planes_ShowTraffic | Button | Show All Traffic | Click: btn_Analysis_Planes_ShowTraffic_Click | ok ui/analysis.rs:298 |  |
| gb_Analysis_Database | GroupBox | Database |  | ok ui/analysis.rs:172 |  |
| tb_Analysis_Status | TextBox |  |  | ok ui/analysis.rs:139 |  |
| btn_Analysis_Planes_History | Button | Load History from URL | Click: btn_Analysis_Planes_History_Click | ok ui/analysis.rs:202 |  |
| btn_Analysis_Planes_Clear | Button | Clear Positions | Click: btn_Analysis_Planes_Clear_Click | ok ui/analysis.rs:196 |  |
| btn_Analysis_Planes_Save | Button | Save Positions to File | Click: btn_Analysis_Planes_Save_Click | ok ui/analysis.rs:189 |  |
| btn_Analysis_Planes_Load | Button | Load Positions from File | Click: btn_Analysis_Planes_Load_Click | ok ui/analysis.rs:238 |  |
| gb_Analysis_Mode | GroupBox | Analysis |  | ok ui/analysis.rs:149 |  |
| btn_Analysis_OFF | Button | OFF | Click: btn_Analysis_OFF_Click | ok ui/analysis.rs:162 |  |
| btn_Analysis_ON | Button | ON | Click: btn_Analysis_ON_Click | ok ui/analysis.rs:153 |  |
| gb_Analysis_Time | GroupBox | Time |  | ok ui/analysis.rs:421 |  |
| label15 | Label | sec. |  | ok ui/analysis.rs:401 |  |
| label14 | Label | Stepwidth: |  | ok ui/analysis.rs:401 |  |
| tb_Analysis_Stepwidth | TextBox |  |  | ok ui/analysis.rs:401 |  |
| tb_Analysis_Time | TextBox |  |  | ok ui/analysis.rs:399 |  |
| tsl_Status | ToolStripStatusLabel | No Messages. |  | ok ui/status.rs:31 |  |
| tsl_Calculations | ToolStripStatusLabel | Calculations. |  | ok ui/status.rs:40 |  |
| tsl_Database | ToolStripStatusLabel | Database status. |  | ok ui/status.rs:62 |  |
| tsl_Database_LED_Aircraft | ToolStripStatusLabel | Aircraft database status LED |  | ok ui/status.rs:63 |  |
| tsl_Database_LED_Stations | ToolStripStatusLabel | Station database status LED |  | ok ui/status.rs:63 |  |
| tsl_Database_LED_Rig | ToolStripStatusLabel | Rig database status LED |  | ok ui/status.rs:63 |  |
| tsl_Database_LED_GLOBE | ToolStripStatusLabel | GLOBE database status LED |  | ok ui/status.rs:63 |  |
| tsl_Database_LED_SRTM3 | ToolStripStatusLabel | SRTM3 database status LED |  | ok ui/status.rs:63 |  |
| tsl_Database_LED_SRTM1 | ToolStripStatusLabel | SRTM3 database status LED |  | ok ui/status.rs:63 |  |
| tsl_Database_LED_ASTER3 | ToolStripStatusLabel | ASTER3 database status LED |  | ok ui/status.rs:63 |  |
| tsl_Database_LED_ASTER1 | ToolStripStatusLabel | ASTER1 database status LED |  | ok ui/status.rs:63 |  |
| tsl_Track | ToolStripStatusLabel | TRK |  | ok ui/status.rs:57 |  |
| tsl_Rot | ToolStripStatusLabel | ROT |  | ok ui/status.rs:59 |  |
| tsl_CAT | ToolStripStatusLabel | CAT |  | ok ui/status.rs:61 |  |
| btn_Map_PlayPause | Button |  | Click: btn_Map_PlayPause_Click | ok ui/control.rs:15 |  |
| btn_Map_Save | Button | Save | Click: btn_Map_Save_Click | missing | Settings are saved automatically; there is no Save button. |
| btn_Options | Button | Options | Click: btn_Options_Click | ok ui/control.rs:25 |  |
| cb_Band | ComboBox |  | SelectedIndexChanged: cb_Band_SelectedIndexChanged | ok ui/mod.rs:107 |  |
| tb_QTF | TextBox |  |  | ok ui/control.rs:106 |  |
| tb_QRB | TextBox |  |  | ok ui/control.rs:107 |  |
| gb_Map_Alarms | GroupBox | Alarms |  | ok ui/control.rs:149 |  |
| cb_Alarms_Activate | CheckBox | Activate Alarms | CheckedChanged: cb_Alarms_Activate_CheckedChanged | ok ui/control.rs:151 |  |
| gb_Map_Zoom | GroupBox | Map Zoom |  | ok ui/control.rs:113 |  |
| cb_AutoCenter | CheckBox | Auto Center |  | ok ui/control.rs:122 |  |
| tb_Zoom | TextBox |  |  | missing | No zoom-level text box; zoom is the +/− buttons. |
| btn_Zoom_Out | Button | - | Click: btn_Zoom_Out_Click | ok ui/control.rs:118 |  |
| btn_Zoom_In | Button | + | Click: btn_Zoom_In_Click | ok ui/control.rs:115 |  |
| gb_Map_Filter | GroupBox | Planes Filter |  | ok ui/control.rs:124 |  |
| label12 | Label | km |  | ok ui/control.rs:131 |  |
| label13 | Label | Max. Circ.: |  | ok ui/control.rs:131 |  |
| cb_Planes_Filter_Min_Cat | ComboBox |  | SelectedIndexChanged: cb_Planes_Filter_Min_Category_SelectedIndexChanged | ok ui/control.rs:134 |  |
| label11 | Label | Min. Cat: |  | ok ui/control.rs:134 |  |
| label10 | Label | m |  | ok ui/control.rs:128 |  |
| label9 | Label | Min. Alt.: |  | ok ui/control.rs:128 |  |
| sc_Main | SplitContainer |  | SplitterMoved: sc_Main_SplitterMoved | ok ui/mod.rs:27 |  |
| tc_Control | TabControl |  | DrawItem: tc_Control_DrawItem, Selecting: tc_Control_Selecting | ok ui/control.rs:33 |  |
| tp_Control_Single | TabPage | Single | Enter: tp_Control_Single_Enter | ok ui/control.rs:34 |  |
| gb_Map_Info | GroupBox | Info |  | ok ui/control.rs:81 |  |
| cb_DXCall | CallsignComboBox |  | DropDown: cb_DXCall_DropDown, SelectedIndexChanged: cb_DXCall_SelectedIndexChanged, TextChanged: cb_DXCall_TextChanged | ok ui/control.rs:93 |  |
| cb_DXLoc | LocatorComboBox |  | DropDown: cb_DXLoc_DropDown, SelectedIndexChanged: cb_DXLoc_SelectedIndexChanged, SelectionChangeCommitted: cb_DXLoc_SelectionChangeCommittedWithNoUpdate, TextChanged: cb_DXLoc_TextChanged | ok ui/control.rs:101 |  |
| cb_MyLoc | LocatorComboBox |  | DropDown: cb_MyLoc_DropDown, SelectedIndexChanged: cb_MyLoc_SelectedIndexChanged, SelectionChangeCommitted: cb_MyLoc_SelectionChangeCommittedWithNoUpdate, TextChanged: cb_MyLoc_TextChanged | ok ui/control.rs:90 |  |
| cb_MyCall | CallsignComboBox |  | DropDown: cb_MyCall_DropDown, SelectedIndexChanged: cb_MyCall_SelectedIndexChanged, TextChanged: cb_MyCall_TextChanged | ok ui/control.rs:86 |  |
| label6 | Label | QTF |  | ok ui/control.rs:106 |  |
| label16 | Label | QRB |  | ok ui/control.rs:107 |  |
| label17 | Label | DX Loc |  | ok ui/control.rs:101 |  |
| label18 | Label | DX Call |  | ok ui/control.rs:93 |  |
| label19 | Label | My Loc |  | ok ui/control.rs:90 |  |
| label20 | Label | My Call |  | ok ui/control.rs:86 |  |
| tp_Control_Multi | TabPage | Multi | Enter: tp_Control_Multi_Enter | ok ui/control.rs:35 |  |
| lv_Control_Watchlist | ListView |  | ColumnClick: lv_Control_Watchlist_ColumnClick, ColumnWidthChanged: lv_Control_Watchlist_ColumnWidthChanged, DrawColumnHeader: lv_Control_Watchlist_DrawColumnHeader, DrawItem: lv_Control_Watchlist_DrawItem, DrawSubItem: lv_Control_Watchlist_DrawSubItem, ItemCheck: lv_Control_Watchlist_ItemCheck, ItemChecked: lv_Control_Watchlist_ItemChecked, ItemMouseHover: lv_Control_Watchlist_ItemMouseHover, SelectedIndexChanged: lv_Control_Watchlist_SelectedIndexChanged, MouseMove: lv_Control_Watchlist_MouseMove, Resize: lv_Control_Watchlist_Resize | ok ui/multi.rs:57 |  |
| ch_Control_Watchlist_Call | ColumnHeader | Call |  | ok ui/multi.rs:67 |  |
| ch_Control_Watchlist_Loc | ColumnHeader | Loc |  | ok ui/multi.rs:75 |  |
| btn_Control_Manage_Watchlist | Button | Manage Watchlist | Click: btn_Control_Manage_Watchlist_Click | ok ui/multi.rs:58 |  |
| tp_Control_Options | TabPage | Opt | Enter: tp_Control_Options_Enter | ok ui/control.rs:36 |  |
| label1 | Label | UTC |  | ok ui/mod.rs:98 |  |
| tb_UTC | TextBox |  |  | ok ui/mod.rs:101 |  |
| label8 | Label | Band |  | ok ui/mod.rs:107 |  |
| gb_Map_Buttons | GroupBox | Control |  | ok ui/control.rs:11 |  |

## MapProviderDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| btn_MapProviderDlg_Decline | Button | Decline |  | missing | The map provider terms dialog is not ported. |
| btn_MapProviderDlg_Accept | Button | Accept |  | missing | The map provider terms dialog is not ported. |

## MapStationDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| label19 | Label | Update station details here. Exact lat/long values are essential for calculation of the radio horizon. Use the lat/lon textboxes for numeric input or enter a valid Maidenhead Locator. Drag the needle on the map for exact location. Use mouse wheel or +/- buttons to zoom in and out. |  | ok ui/dialogs/map_station.rs:222 |  |
| label18 | Label | Map Zoom: |  | missing | No zoom readout; the map zooms with the mouse. |
| tb_Zoom | TextBox |  |  | missing | No zoom readout; the map zooms with the mouse. |
| btn_Zoom_Out | Button | - | Click: btn_Zoom_Out_Click | missing | No zoom buttons; the map zooms with the mouse. |
| btn_Zoom_In | Button | + | Click: btn_Zoom_In_Click | missing | No zoom buttons; the map zooms with the mouse. |
| label11 | Label | Locator: |  | ok ui/dialogs/map_station.rs:263 |  |
| label10 | Label | Longitude: |  | ok ui/dialogs/map_station.rs:254 |  |
| label9 | Label | Latitude: |  | ok ui/dialogs/map_station.rs:245 |  |
| label8 | Label | Callsign: |  | ok ui/dialogs/map_station.rs:222 |  |
| gm_Callsign | GMapControl |  | OnMarkerEnter: gm_Callsign_OnMarkerEnter, OnMapZoomChanged: gm_Callsign_OnMapZoomChanged, MouseDown: gm_Callsign_MouseDown, MouseMove: gm_Callsign_MouseMove, MouseUp: gm_Callsign_MouseUp | ok ui/dialogs/map_station.rs:301 |  |
| tb_Longitude | DoubleTextBox |  | TextChanged: tb_Longitude_TextChanged | ok ui/dialogs/map_station.rs:256 |  |
| tb_Latitude | DoubleTextBox |  | TextChanged: tb_Latitude_TextChanged | ok ui/dialogs/map_station.rs:247 |  |
| btn_Cancel | Button | Cancel | Click: btn_Cancel_Click | ok ui/dialogs/map_station.rs:233 |  |
| btn_OK | Button | OK |  | ok ui/dialogs/map_station.rs:230 |  |
| label1 | Label | Elevation: |  | missing | No elevation readout next to the station. |
| label2 | Label | m asl |  | missing | No elevation readout next to the station. |
| tsl_Status | ToolStripStatusLabel | Status |  | ok ui/dialogs/map_station.rs:227 |  |
| tb_Locator | LocatorTextBox |  | TextChanged: tb_Locator_TextChanged | ok ui/dialogs/map_station.rs:265 |  |
| cb_Options_StationsMap_OverlayElevation | CheckBox | Overlay Elevation Grid  (on zom levels >= 17) | CheckedChanged: cb_Options_StationsMap_OverlayElevation_CheckedChanged | ok ui/dialogs/map_station.rs:284 |  |

## OptionsDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| btn_Options_OK | Button | OK |  | ok ui/options/mod.rs:307 |  |
| btn_Options_Cancel | Button | Cancel |  | ok ui/options/mod.rs:316 |  |
| btn_Options_Import_Callsigns | Button | Select File | Click: btn_Options_Import_Callsigns_Click | missing | Callsign-database import from a file is not ported. |
| btn_Options_MyUpdate | Button | Send Update! | Click: btn_Options_MyUpdate_Click | not ported: upload | Sharing QRV data is a spec non-goal. |
| btn_Options_DXUpdate | Button | Send Update! | Click: btn_Options_DXUpdate_Click | not ported: upload | Sharing QRV data is a spec non-goal. |
| btn_Options_DeleteAllElevationPaths | Button | Delete All Elevation Paths | Click: btn_Options_DeleteAllElevationPaths_Click | ok ui/options/database.rs:92 |  |
| btn_Options_DeleteAllPropagationPaths | Button | Delete All Propagation Paths | Click: btn_Options_DeleteAllPropagationPaths_Click | ok ui/options/database.rs:92 |  |
| btn_Options_Open_TmpDirectory | Button | Open AirScout Tmp Directory in Explorer Window | Click: btn_Options_Open_TmpDirectory_Click | missing | Opening directories is not ported. |
| btn_Options_Open_LogDirectory | Button | Open AirScout Log Directory in Explorer Window | Click: btn_Options_Open_LogDirectory_Click | missing | Opening directories is not ported. |
| btn_DeleteAllMapTiles | Button | Delete All Map Tiles | Click: btn_DeleteAllMapTiles_Click | ok ui/options/database.rs:92 |  |
| btn_Options_Open_PluginDirectory | Button | Open AirScout Plugin Directory in Explorer Window | Click: btn_Options_Open_PluginDirectory_Click | not ported: plugin download | Plugin support is a spec non-goal. |
| btn_Options_Open_AirScoutDirectory | Button | Open AirScout Program Directory in Explorer Window | Click: btn_Options_Open_AirScoutDirectory_Click | missing | Opening directories is not ported. |
| pb_Donate | PictureBox |  | Click: pb_Donate_Click | missing | The donate button is not ported. |
| rb_Options_Track_UDP_AirScout | RadioButton | UDP Broadcast (AirScout) Az/El | CheckedChanged: gb_Options_Track_UDP_CheckedChanged | ok ui/options/track.rs:75 |  |
| tb_Options_Track_DialFreq | LongTextBox |  | TextChanged: tb_Options_CAT_DialFreq_TextChanged | ok ui/options/track.rs:128 |  |
| btn_DeleteSingleStation | Button | Delete Single Station | Click: btn_DeleteSingleStation_Click | ok ui/options/database.rs:92 |  |
| btn_Options_LocalObstructions | Button | Manage | Click: btn_Options_LocalObstructions_Click | ok ui/options/stations.rs:366 |  |
| btn_Options_DXHorizon | Button | Radio Horizon | Click: btn_Options_DXHorizon_Click | ok ui/options/stations.rs:363 |  |
| btn_DXCall_QRZ | Button | QRZ lookup | Click: btn_DXCall_QRZ_Click | missing | QRZ lookup is not ported. |
| btn_Options_MyHorizon | Button | Radio Horizon | Click: btn_Options_MyHorizon_Click | ok ui/options/stations.rs:363 |  |
| btn_MyCall_QRZ | Button | QRZ lookup | Click: btn_MyCall_QRZ_Click | missing | QRZ lookup is not ported. |
| tab_Options_Planes | TabPage | Planes | Enter: tab_Options_Planes_Enter, Validating: tab_Options_Planes_Validating | ok ui/options/planes.rs:23 |  |
| groupBox36 | GroupBox | Debug |  | missing | No Debug group on the Planes tab. |
| groupBox48 | GroupBox | General Plane Feed Settings |  | ok ui/options/planes.rs:26 |  |
| label33 | Label | Max. allowed distance between  reported and estimated position [km]: |  | missing | `planes_max_error_dist` has no UI. |
| label22 | Label | Interval [sec]: |  | missing | `planes_interval_s` has no UI. |
| groupBox40 | GroupBox | Database Maintenance |  | missing | Keeping positions for N days is not ported. |
| label55 | Label | Aircraft positions older than above entered value will be deleted on next startup (0 = forever). |  | missing | Keeping positions for N days is not ported. |
| label53 | Label | days. |  | missing | Keeping positions for N days is not ported. |
| label32 | Label | Keep aircraft positions for: |  | missing | Keeping positions for N days is not ported. |
| groupBox38 | GroupBox | Plane Position Database Filters |  | ok ui/options/planes.rs:31 |  |
| label2 | Label | mins. |  | ok ui/options/planes.rs:31 |  |
| label1 | Label | Discard plane when last position report is older than: |  | ok ui/options/planes.rs:31 |  |
| label9 | Label | m |  | ok ui/options/planes.rs:27 |  |
| label8 | Label | m |  | ok ui/options/planes.rs:28 |  |
| label7 | Label | Maximal Altitude: |  | ok ui/options/planes.rs:28 |  |
| label5 | Label | Minimal Altitude: |  | ok ui/options/planes.rs:27 |  |
| groupBox26 | GroupBox | Plane Live Position Filters |  | ok ui/options/planes.rs:26 |  |
| label96 | Label | You can change the values here or in the Main Window. |  | ok ui/options/planes.rs:23 |  |
| label94 | Label | km |  | missing | The max-circumcircle filter lives in the main window, not the Planes tab. |
| label95 | Label | Maximal Path Midpoint Circumcircle (Filter AUTO = 0) (Filter OFF = -1): |  | missing | The max-circumcircle filter lives in the main window, not the Planes tab. |
| label92 | Label | m |  | ok ui/options/planes.rs:27 |  |
| label220 | Label | m |  | ok ui/options/planes.rs:28 |  |
| label93 | Label | Minimal Altitude  (Filter OFF = 0: |  | ok ui/options/planes.rs:27 |  |
| label91 | Label | Minimal Category: |  | ok ui/options/planes.rs:43 |  |
| cb_Options_Planes_Filter_Min_Cat | ComboBox |  | SelectedIndexChanged: cb_Options_Planes_Filter_Min_Cat_SelectedIndexChanged | ok ui/options/planes.rs:43 |  |
| groupBox6 | GroupBox | Plane Feeds |  | ok ui/options/planes.rs:77 |  |
| btn_Options_PlaneFeed3_Default | Button | Default | Click: btn_Options_PlaneFeed3_Default_Click | missing | Feed defaults are restored inside the feed settings dialog. |
| btn_Options_PlaneFeed2_Default | Button | Default | Click: btn_Options_PlaneFeed2_Default_Click | missing | Feed defaults are restored inside the feed settings dialog. |
| btn_Options_PlaneFeed1_Default | Button | Default | Click: btn_Options_PlaneFeed1_Default_Click | missing | Feed defaults are restored inside the feed settings dialog. |
| btn_Options_PlaneFeed3_Export | Button | Export | Click: btn_Options_PlaneFeed3_Export_Click | missing | Feed file export is not ported. |
| btn_Options_PlaneFeed2_Export | Button | Export | Click: btn_Options_PlaneFeed2_Export_Click | missing | Feed file export is not ported. |
| btn_Options_PlaneFeed1_Export | Button | Export | Click: btn_Options_PlaneFeed1_Export_Click | missing | Feed file export is not ported. |
| btn_Options_PlaneFeed3_Import | Button | Import | Click: btn_Options_PlaneFeed3_Import_Click | missing | Feed file import is not ported. |
| btn_Options_PlaneFeed2_Import | Button | Import | Click: btn_Options_PlaneFeed2_Import_Click | missing | Feed file import is not ported. |
| btn_Options_PlaneFeed1_Import | Button | Import | Click: btn_Options_PlaneFeed1_Import_Click | missing | Feed file import is not ported. |
| label79 | Label | Plane Feed 3: |  | ok ui/options/planes.rs:116 |  |
| btn_Options_PlaneFeed3_Settings | Button | Settings | Click: btn_Options_PlaneFeed3_Settings_Click | ok ui/options/planes.rs:95 |  |
| cb_Options_PlaneFeed3 | ComboBox |  | SelectedIndexChanged: cb_Options_PlaneFeed3_SelectedIndexChanged | ok ui/options/planes.rs:116 |  |
| label78 | Label | Plane Feed 2: |  | ok ui/options/planes.rs:116 |  |
| btn_Options_PlaneFeed2_Settings | Button | Settings | Click: btn_Options_PlaneFeed2_Settings_Click | ok ui/options/planes.rs:95 |  |
| cb_Options_PlaneFeed2 | ComboBox |  | SelectedIndexChanged: cb_Options_PlaneFeed2_SelectedIndexChanged | ok ui/options/planes.rs:116 |  |
| label77 | Label | Plane Feed 1: |  | ok ui/options/planes.rs:116 |  |
| btn_Options_PlaneFeed1_Settings | Button | Settings | Click: btn_Options_PlaneFeed1_Settings_Click | ok ui/options/planes.rs:95 |  |
| cb_Options_PlaneFeed1 | ComboBox |  | SelectedIndexChanged: cb_Options_PlaneFeed1_SelectedIndexChanged | ok ui/options/planes.rs:116 |  |
| tab_Options_Path | TabPage | Path | Enter: tab_Options_Path_Enter, Leave: tab_Options_Path_Leave, Validating: tab_Options_Path_Validating | ok ui/options/path.rs:7 |  |
| groupBox1 | GroupBox | Path Options |  | ok ui/options/path.rs:7 |  |
| btn_Options_Path_Export | Button | Export | Click: btn_Options_Path_Export_Click | missing | Path CSV export is not ported from the Path tab. |
| label147 | Label | Export elevation path to CSV (do not change settings before as they may not have  effect): |  | missing | Path CSV export is not ported from the Path tab. |
| label131 | Label | km |  | ok ui/options/path.rs:46 |  |
| label130 | Label | Do not calculate path automatically when length is more than: |  | ok ui/options/path.rs:46 |  |
| tb_Options_Path_StepWidth | TextBox |  |  | missing | No path stepwidth field. |
| label129 | Label | Path calculation stepwidth according to Elevation Model: |  | missing | No path stepwidth field. |
| label57 | Label | m |  | ok ui/options/path.rs:53 |  |
| tab_Options_SRTM1 | TabPage | SRTM1 | Enter: tab_Options_SRTM1_Enter, Leave: tab_Options_SRTM1_Leave | ok ui/options/elevation.rs:18 |  |
| groupBox43 | GroupBox | (S)huttle (R)adar (T)opography (M)ission 1 arscec |  | missing | The per-model description group is not rendered. |
| label111 | Label | Best performanc. slow calculation, excellent precision |  | missing | The per-model description group is not rendered. |
| label112 | Label | A 1 arcsec (30m) grided elevation model. Coverage: 58°S ... 60°N; 180°W ... 180°E |  | missing | The per-model description group is not rendered. |
| btn_Options_SRTM1_Copyright | Button | Licence/Copyright  Information | Click: btn_Options_SRTM1_Copyright_Click | ok ui/options/elevation.rs:31 |  |
| groupBox13 | GroupBox | Info |  | missing | The per-model info group is not rendered. |
| groupBox12 | GroupBox | Use Elevation Model |  | ok ui/options/elevation.rs:30 |  |
| tab_Options_SRTM3 | TabPage | SRTM3 | Enter: tab_Options_SRTM3_Enter, Leave: tab_Options_SRTM3_Leave | ok ui/options/elevation.rs:18 |  |
| groupBox42 | GroupBox | (S)huttle (R)adar (T)opography (M)ission 3 arscec |  | missing | The per-model description group is not rendered. |
| label100 | Label | Needs more performanc. fast calculation, good precision |  | missing | The per-model description group is not rendered. |
| label110 | Label | A 3 arcsec (90m) grided elevation model. Coverage: 58°S ... 60°N; 180°W ... 180°E |  | missing | The per-model description group is not rendered. |
| btn_Options_SRTM3_Copyright | Button | Licence/Copyright  Information | Click: btn_Options_SRTM3_Copyright_Click | ok ui/options/elevation.rs:31 |  |
| groupBox9 | GroupBox | Info |  | missing | The per-model info group is not rendered. |
| groupBox8 | GroupBox | Use Elevation Model |  | ok ui/options/elevation.rs:30 |  |
| tab_Options_GLOBE | TabPage | GLOBE | Enter: tab_Options_GLOBE_Enter, Leave: tab_Options_GLOBE_Leave | ok ui/options/elevation.rs:18 |  |
| groupBox41 | GroupBox | (G)lobal (L)and (O)ne-km (B)ase (E)levation Project |  | missing | The per-model description group is not rendered. |
| label99 | Label | Recommed for basic usage: Fast calculation, low precision |  | missing | The per-model description group is not rendered. |
| label98 | Label | A 30 arcsec (1km) grided elevation model. Coverage: 90°S ... 90°N; 180°W ... 180°E |  | missing | The per-model description group is not rendered. |
| btn_Options_GLOBE_Copyright | Button | Licence/Copyright  Information | Click: btn_Options_GLOBE_Copyright_Click | ok ui/options/elevation.rs:31 |  |
| groupBox11 | GroupBox | Info |  | missing | The per-model info group is not rendered. |
| groupBox10 | GroupBox | Use Elevation Model |  | ok ui/options/elevation.rs:30 |  |
| tab_Options_Map | TabPage | Map | Enter: tab_Options_Map_Enter, Validating: tab_Options_Map_Validating | ok ui/options/map.rs:27 |  |
| groupBox22 | GroupBox | Tracking Gauges |  | ok ui/options/track.rs:108 |  |
| cb_Options_Gauges_ForeColor | ComboBox |  | SelectedIndexChanged: cb_Options_Gauges_ForeColor_SelectedIndexChanged | missing | Gauge forecolor has no UI. |
| label198 | Label | Gauge ForeColor: |  | missing | Gauge forecolor has no UI. |
| label188 | Label | Azimuth/Elevation Gauge Width: |  | missing | Gauge width has no UI. |
| groupBox49 | GroupBox | Multi-Path |  | missing | No multi-path group on the Map tab. |
| groupBox23 | GroupBox | General |  | ok ui/options/map.rs:27 |  |
| groupBox30 | GroupBox | Plane Icon Sizes per Category |  | ok ui/options/map.rs:43 |  |
| label84 | Label | Restart of AirScout required! |  | ok ui/options/map.rs:43 |  |
| label83 | Label | Medium: |  | ok ui/options/map.rs:43 |  |
| label82 | Label | Heavy: |  | ok ui/options/map.rs:43 |  |
| label81 | Label | Super: |  | ok ui/options/map.rs:43 |  |
| label80 | Label | Light: |  | ok ui/options/map.rs:43 |  |
| groupBox7 | GroupBox | Info Window Elements |  | ok ui/options/planes.rs:51 |  |
| label76 | Label | Visible only in detailed window and when tracking |  | missing | The visibility mode of info-window rows is not ported. |
| label75 | Label | Visible only in detailed window when crossing |  | missing | The visibility mode of info-window rows is not ported. |
| label72 | Label | Units: |  | ok ui/options/planes.rs:54 |  |
| btn_Options_SelectFont | Button | Select Font |  | missing | Selecting a font is not ported. |
| label62 | Label | Font: |  | missing | Selecting a font is not ported. |
| label74 | Label | Always visible |  | missing | The visibility mode of info-window rows is not ported. |
| groupBox2 | GroupBox | Map |  | ok ui/options/map.rs:27 |  |
| label97 | Label | second(s). |  | ok ui/options/map.rs:49 |  |
| label29 | Label | Update screen every |  | ok ui/options/map.rs:49 |  |
| label61 | Label | You can select different map provider from the list below. Please note that the selection of maps is taken from Great Maps regardless of their legal status. Some of them might be copyrighted or otherwise restricted in use.   You were asked to agree with the terms of use first. Open Street Map is recommended as a default. |  | ok ui/options/map.rs:34 |  |
| cb_Options_Map_Provider | ComboBox |  | DropDown: cb_Options_Map_Provider_DropDown | ok ui/options/map.rs:34 |  |
| label148 | Label | Map Opacity: |  | ok ui/options/map.rs:42 |  |
| label144 | Label | down to  Zoom Level: |  | ok ui/options/map.rs:63 |  |
| tab_Options_Stations | TabPage | Stations | Enter: tab_Options_Stations_Enter, Leave: tab_Options_Stations_Leave, Validating: tab_Options_Stations_Validating | ok ui/options/stations.rs:265 |  |
| groupBox18 | GroupBox | Local Obstructions for My Station |  | ok ui/options/stations.rs:366 |  |
| lbl_Options_LocalObstructions | Label | Status is unknown. |  | missing | No local-obstructions status label; the Manage button opens the dialog. |
| groupBox46 | GroupBox | DX QRV |  | ok ui/options/stations.rs:373 |  |
| lbl_Options_DXLastUpdated | Label | LastUpdated |  | missing | No QRV last-updated timestamp is shown. |
| label128 | Label | Last Updated: |  | missing | No QRV last-updated timestamp is shown. |
| label13 | Label | W |  | ok ui/options/stations.rs:402 |  |
| label58 | Label | Power: |  | ok ui/options/stations.rs:391 |  |
| label107 | Label | dbD |  | ok ui/options/stations.rs:390 |  |
| label124 | Label | Antenna Gain: |  | ok ui/options/stations.rs:390 |  |
| label125 | Label | m |  | ok ui/options/stations.rs:389 |  |
| label126 | Label | Antenna Height: |  | ok ui/options/stations.rs:389 |  |
| groupBox45 | GroupBox | Band |  | ok ui/options/stations.rs:386 |  |
| btn_Options_BandDown | Button | ▼ | Click: btn_Options_BandDown_Click | missing | All bands are listed in the QRV grid; there is no band stepper. |
| btn_Options_BandUp | Button | ▲ | Click: btn_Options_BandUp_Click | missing | All bands are listed in the QRV grid; there is no band stepper. |
| tb_Options_Band | TextBox |  |  | missing | All bands are listed in the QRV grid; there is no band field. |
| groupBox44 | GroupBox | My QRV |  | ok ui/options/stations.rs:373 |  |
| lbl_Options_MyLastUpdated | Label | LastUpdated |  | missing | No QRV last-updated timestamp is shown. |
| label127 | Label | Last Updated: |  | missing | No QRV last-updated timestamp is shown. |
| label122 | Label | W |  | ok ui/options/stations.rs:402 |  |
| label123 | Label | Power: |  | ok ui/options/stations.rs:391 |  |
| label106 | Label | dbD |  | ok ui/options/stations.rs:390 |  |
| label121 | Label | Antenna Gain: |  | ok ui/options/stations.rs:390 |  |
| label28 | Label | m |  | ok ui/options/stations.rs:389 |  |
| label41 | Label | Antenna Height: |  | ok ui/options/stations.rs:389 |  |
| groupBox14 | GroupBox | Location Settings |  | ok ui/options/stations.rs:126 |  |
| label48 | Label | Number of Locator digits (6 ..14): |  | ok ui/options/stations.rs:126 |  |
| groupBox16 | GroupBox | Stations Information |  | ok ui/options/stations.rs:265 |  |
| label52 | Label | Information from callsign database and other sources are used to prefill fields. You can overwrite and complete entries here. Your local database is updated. If you want to share the information with the AirScout community please use the "Send Update!" buttons. |  | ok ui/options/stations.rs:265 |  |
| groupBox5 | GroupBox | DX Station |  | ok ui/options/stations.rs:305 |  |
| btn_Options_DXMap | Button | Map | Click: btn_Options_DXMap_Click | ok ui/options/stations.rs:360 |  |
| tb_Options_DXLon | DoubleTextBox |  | TextChanged: tb_Options_DXLon_TextChanged | ok ui/options/stations.rs:333 |  |
| tb_Options_DXLat | DoubleTextBox |  | TextChanged: tb_Options_DXLat_TextChanged | ok ui/options/stations.rs:324 |  |
| tb_Options_DXCall | CallsignTextBox |  | TextChanged: tb_Options_DXCall_TextChanged | ok ui/options/stations.rs:309 |  |
| label21 | Label | m asl |  | ok ui/options/stations.rs:355 |  |
| label14 | Label | Lon: |  | ok ui/options/stations.rs:331 |  |
| label15 | Label | Lat: |  | ok ui/options/stations.rs:322 |  |
| label42 | Label | Call: |  | ok ui/options/stations.rs:307 |  |
| tb_Options_DXElevation | TextBox |  |  | ok ui/options/stations.rs:355 |  |
| label43 | Label | Elevation: |  | ok ui/options/stations.rs:354 |  |
| label44 | Label | Loc: |  | ok ui/options/stations.rs:342 |  |
| groupBox4 | GroupBox | My Station |  | ok ui/options/stations.rs:305 |  |
| btn_Options_MyMap | Button | Map | Click: btn_Options_MyMap_Click | ok ui/options/stations.rs:360 |  |
| tb_Options_MyLon | DoubleTextBox |  | TextChanged: tb_Options_MyLon_TextChanged | ok ui/options/stations.rs:333 |  |
| tb_Options_MyLat | DoubleTextBox |  | TextChanged: tb_Options_MyLat_TextChanged | ok ui/options/stations.rs:324 |  |
| tb_Options_MyCall | CallsignTextBox |  | TextChanged: tb_Options_MyCall_TextChanged | ok ui/options/stations.rs:309 |  |
| label18 | Label | m asl |  | ok ui/options/stations.rs:355 |  |
| label40 | Label | Lon: |  | ok ui/options/stations.rs:331 |  |
| label39 | Label | Lat: |  | ok ui/options/stations.rs:322 |  |
| label10 | Label | Call: |  | ok ui/options/stations.rs:307 |  |
| tb_Options_MyElevation | TextBox |  |  | ok ui/options/stations.rs:355 |  |
| label12 | Label | Elevation: |  | ok ui/options/stations.rs:354 |  |
| label11 | Label | Loc: |  | ok ui/options/stations.rs:342 |  |
| tab_Options_General | TabPage | General | Enter: tab_Options_General_Enter, Validating: tab_Options_General_Validating | ok ui/options/general.rs:7 |  |
| groupBox25 | GroupBox | Covered Area |  | ok ui/options/general.rs:8 |  |
| label35 | Label | Max. Latitude: |  | ok ui/options/general.rs:12 |  |
| label54 | Label | Min. Latitude: |  | ok ui/options/general.rs:11 |  |
| label59 | Label | Max. Longitude: |  | ok ui/options/general.rs:14 |  |
| label60 | Label | Min. Longitude: |  | ok ui/options/general.rs:13 |  |
| tab_Options_Database | TabPage | Database | Enter: tab_Options_Database_Enter, Validating: tab_Options_Database_Validating | ok ui/options/database.rs:28 |  |
| groupBox47 | GroupBox | Quick Maintenance Commands |  | ok ui/options/database.rs:79 |  |
| groupBox27 | GroupBox | Import |  | missing | Callsign-database import from a file is not ported. |
| label108 | Label | Import Callsign Database from previous versions: |  | missing | Callsign-database import from a file is not ported. |
| gb_Options_Database_Settings | GroupBox | Background Update and Pre-Calculation Settings |  | ok ui/options/database.rs:43 |  |
| label47 | Label | Update interval [min]: |  | ok ui/options/database.rs:51 |  |
| rb_Options_Database_Update_Periodically | RadioButton | Periodically | CheckedChanged: rb_Options_Database_Update_Periodically_CheckedChanged | ok ui/options/database.rs:46 |  |
| rb_Options_Database_Update_OnStartup | RadioButton | On Program Startup | CheckedChanged: rb_Options_Database_Update_OnStartup_CheckedChanged | ok ui/options/database.rs:45 |  |
| rb_Options_Database_Update_Never | RadioButton | Never | CheckedChanged: rb_Options_Database_Update_Never_CheckedChanged | ok ui/options/database.rs:44 |  |
| gb_Options_Database_Info | GroupBox | Info |  | ok ui/options/database.rs:98 |  |
| btn_Options_Elevation_ASTER1_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Elevation_ASTER1_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| label205 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Elevation_ASTER1_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label206 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label207 | Label | ASTER1 Elevation Data: |  | ok ui/options/database.rs:104 |  |
| btn_Options_Elevation_ASTER3_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Elevation_ASTER3_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| label201 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Elevation_ASTER3_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label202 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label203 | Label | ASTER3 Elevation Data: |  | ok ui/options/database.rs:104 |  |
| lbl_Options_ScoutBase_Database_FileName | Label |  | Click: lbl_Options_ScoutBase_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_AirScout_Database_FileName | Label |  | Click: lbl_Options_AirScout_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_Propagation_GLOBE_Database_FileName | Label |  | Click: lbl_Options_Propagation_GLOBE_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_Propagation_SRTM3_Database_FileName | Label |  | Click: lbl_Options_Propagation_SRTM3_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_Propagation_SRTM1_Database_FileName | Label |  | Click: lbl_Options_Propagation_SRTM1_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_Elevation_GLOBE_Database_FileName | Label |  | Click: lbl_Options_Elevation_GLOBE_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_Elevation_SRTM3_Database_FileName | Label |  | Click: lbl_Options_Elevation_SRTM3_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_Elevation_SRTM1_Database_FileName | Label |  | Click: lbl_Options_Elevation_SRTM1_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| lbl_Options_Map_Database_FileName | Label |  | Click: lbl_Options_Map_Database_FileName_Click | ok ui/options/database.rs:105 |  |
| btn_Options_Map_Database_Maintenance | Button | Database Maintenance |  | ok ui/options/database.rs:108 |  |
| label38 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Map_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label140 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label142 | Label | Map Data: |  | ok ui/options/database.rs:104 |  |
| btn_Options_Elevation_SRTM1_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Elevation_SRTM1_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| btn_Options_Elevation_SRTM3_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Elevation_SRTM3_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| btn_Options_Elevation_GLOBE_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Elevation_GLOBE_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| btn_Options_Propagation_SRTM1_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Propagation_SRTM1_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| btn_Options_Propagation_SRTM3_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Propagation_SRTM3_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| btn_Options_Propagation_GLOBE_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_Propagation_GLOBE_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| btn_Options_AirScout_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_AirScout_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| btn_Options_ScoutBase_Database_Maintenance | Button | Database Maintenance | Click: btn_Options_ScoutBase_Database_Maintenance_Click | ok ui/options/database.rs:108 |  |
| label141 | Label | MB |  | ok ui/options/database.rs:106 |  |
| lbl_Options_Database_TotalSize | Label | xxx |  | ok ui/options/database.rs:118 |  |
| label139 | Label | total: |  | ok ui/options/database.rs:118 |  |
| label136 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Propagation_SRTM1_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label137 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label138 | Label | SRTM1 Propagation Data: |  | ok ui/options/database.rs:104 |  |
| label46 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Propagation_SRTM3_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label134 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label135 | Label | SRTM3 Propagation Data: |  | ok ui/options/database.rs:104 |  |
| label132 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Propagation_GLOBE_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label133 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label109 | Label | GLOBE Propagation Data: |  | ok ui/options/database.rs:104 |  |
| label118 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Elevation_SRTM1_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label119 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label120 | Label | SRTM1 Elevation Data: |  | ok ui/options/database.rs:104 |  |
| label115 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Elevation_SRTM3_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label116 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label117 | Label | SRTM3 Elevation Data: |  | ok ui/options/database.rs:104 |  |
| label16 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_Elevation_GLOBE_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label113 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label114 | Label | GLOBE Elevation Data; |  | ok ui/options/database.rs:104 |  |
| label104 | Label | This box is showing general info about the SQLite databases used by AirScout. |  | ok ui/options/database.rs:98 |  |
| label101 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_AirScout_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label102 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label103 | Label | Aircraft Data: |  | ok ui/options/database.rs:104 |  |
| label73 | Label | MB |  | ok ui/options/database.rs:106 |  |
| tb_Options_ScoutBase_Database_FileSize | TextBox |  |  | ok ui/options/database.rs:106 |  |
| label50 | Label | Size: |  | ok ui/options/database.rs:106 |  |
| label49 | Label | Station Data: |  | ok ui/options/database.rs:104 |  |
| tab_Options_ASTER3 | TabPage | ASTER3 | Enter: tab_Options_ASTER3_Enter, Leave: tab_Options_ASTER3_Leave | ok ui/options/elevation.rs:18 |  |
| groupBox51 | GroupBox | (A)dvanced (S)paceborn (Thermal (E)mission and (R)eflection Radiometer 3 arscec |  | missing | The per-model description group is not rendered. |
| label146 | Label | A 3 arcsec (90m) grided elevation model. Coverage: 83°S ... 83°N; 180°W ... 180°E |  | missing | The per-model description group is not rendered. |
| btn_Options_ASTER3_Copyright | Button | Licence/Copyright  Information | Click: btn_Options_ASTER3_Copyright_Click | ok ui/options/elevation.rs:31 |  |
| groupBox52 | GroupBox | Info |  | missing | The per-model info group is not rendered. |
| groupBox53 | GroupBox | Use Elevation Model |  | ok ui/options/elevation.rs:30 |  |
| tba_Option_ASTER1 | TabPage | ASTER1 | Enter: tba_Option_ASTER1_Enter, Leave: tba_Option_ASTER1_Leave | ok ui/options/elevation.rs:18 |  |
| groupBox54 | GroupBox | (A)dvanced (S)paceborn (Thermal (E)mission and (R)eflection Radiometer 1 arscec |  | missing | The per-model description group is not rendered. |
| label145 | Label | A 1 arcsec (30m) grided elevation model. Coverage: 83°S ... 83°N; 180°W ... 180°E |  | missing | The per-model description group is not rendered. |
| btn_Options_ASTER1_Copyright | Button | Licence/Copyright  Information | Click: btn_Options_ASTER1_Copyright_Click | ok ui/options/elevation.rs:31 |  |
| groupBox55 | GroupBox | Info |  | missing | The per-model info group is not rendered. |
| groupBox56 | GroupBox | Use Elevation Model |  | ok ui/options/elevation.rs:30 |  |
| tab_Options_Alarm | TabPage | Alarm | Enter: tab_Options_Alarm_Enter | ok ui/options/alarm.rs:7 |  |
| groupBox21 | GroupBox | Activate Alarm |  | ok ui/options/alarm.rs:9 |  |
| groupBox19 | GroupBox | Alarm Settings |  | ok ui/options/alarm.rs:10 |  |
| cb_Options_Alarm_BringWindowToFront | CheckBox | Bring window to front |  | ok ui/options/alarm.rs:18 |  |
| groupBox20 | GroupBox | Generate Alarm |  | ok ui/options/alarm.rs:10 |  |
| label36 | Label | km |  | ok ui/options/alarm.rs:13 |  |
| label56 | Label | If a plane is heading towards the path with suitable altitude, generate an alarm at distance of: |  | ok ui/options/alarm.rs:13 |  |
| tab_Options_Network | TabPage | Network | Enter: tab_Options_Network_Enter, Validating: tab_Options_Network_Validating | ok ui/options/network.rs:9 |  |
| label86 | Label | CAUTION: Running a web service is a potential security hole! You should activate this function only inside a private network.  Depending on your user profile you will prompted several times by the Operating System on first run. Please accept all security queries with "Yes" or "Accept".  Otherwise the web service will not run properly. |  | ok ui/options/network.rs:10 |  |
| groupBox32 | GroupBox | HTTP Server Settings |  | ok ui/options/network.rs:31 |  |
| label85 | Label | AirScout HTTP Server Port: |  | ok ui/options/network.rs:34 |  |
| groupBox31 | GroupBox | Activate Server |  | ok ui/options/network.rs:10 |  |
| label3 | Label | AirScout can work as a server in a network.   UDP Server:  You can ask for a path calculation between two stations and AirScout will return the planes near path and their portential for a reflection.   HTTP Server:  You can ask for latest plane positions via simple http-request (e.g. from a web browser. The result is delivered as a JSON file which can used to run own services and calculations.  See documentation for further details. |  | ok ui/options/network.rs:10 |  |
| groupBox24 | GroupBox | UDP Server Settings |  | ok ui/options/network.rs:15 |  |
| label6 | Label | AirScout UDP Server Port: |  | ok ui/options/network.rs:20 |  |
| label4 | Label | AirScout UDP Server Name: |  | ok ui/options/network.rs:24 |  |
| tab_Options_SpecLab | TabPage | Spectrum | Enter: tab_Options_SpecLab_Enter, Validating: tab_Options_SpecLab_Validating | ok ui/options/speclab.rs:7 |  |
| groupBox3 | GroupBox | Spectrum Lab Settings |  | ok ui/options/speclab.rs:7 |  |
| label70 | Label | sec |  | ok ui/options/speclab.rs:15 |  |
| label71 | Label | Update Interval: |  | ok ui/options/speclab.rs:15 |  |
| label68 | Label | Hz |  | ok ui/options/speclab.rs:14 |  |
| label69 | Label | F2: |  | ok ui/options/speclab.rs:14 |  |
| label67 | Label | Hz |  | ok ui/options/speclab.rs:13 |  |
| label66 | Label | F1: |  | ok ui/options/speclab.rs:13 |  |
| label63 | Label | Server Filename: |  | ok ui/options/speclab.rs:12 |  |
| label64 | Label | Server URL |  | ok ui/options/speclab.rs:11 |  |
| label65 | Label | AirScout can get FFT data from Spectrum Lab software via Network. You must have Spectrum Lab software installed with default settings. Do not forget to activate http - server functionaltiy in Spectrum Lab. |  | ok ui/options/speclab.rs:9 |  |
| tab_Options_Track | TabPage | Track | Enter: tab_Options_Track_Enter, Validating: tc_Track_Validating | ok ui/options/track.rs:49 |  |
| gb_Options_Doppler | GroupBox | Doppler Compensation (requires activated CAT and Rig connected) |  | ok ui/options/track.rs:116 |  |
| label150 | Label | Hz |  | ok ui/options/track.rs:128 |  |
| label180 | Label | no |  | ok ui/options/track.rs:135 |  |
| label181 | Label | no |  | ok ui/options/track.rs:135 |  |
| label182 | Label | no |  | ok ui/options/track.rs:135 |  |
| label183 | Label | no |  | ok ui/options/track.rs:135 |  |
| label184 | Label | None |  | ok ui/options/track.rs:135 |  |
| rb_Options_Doppler_Strategy_None | RadioButton |  | CheckedChanged: gb_Options_Doppler_CheckedChanged | ok ui/options/track.rs:135 |  |
| label178 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label179 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label176 | Label | no |  | ok ui/options/track.rs:135 |  |
| label177 | Label | no |  | ok ui/options/track.rs:135 |  |
| label174 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label175 | Label | no |  | ok ui/options/track.rs:135 |  |
| label172 | Label | no |  | ok ui/options/track.rs:135 |  |
| label173 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label170 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label171 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label168 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label169 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label166 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label167 | Label | no |  | ok ui/options/track.rs:135 |  |
| label165 | Label | no |  | ok ui/options/track.rs:135 |  |
| label163 | Label | RX |  | ok ui/options/track.rs:135 |  |
| label164 | Label | TX |  | ok ui/options/track.rs:135 |  |
| label162 | Label | RX |  | ok ui/options/track.rs:135 |  |
| label161 | Label | TX |  | ok ui/options/track.rs:135 |  |
| label160 | Label | yes |  | ok ui/options/track.rs:135 |  |
| label159 | Label | Constant frequency at aircraft: Both stations have to compensate RX + TX . |  | ok ui/options/track.rs:135 |  |
| label158 | Label | Compensate both RX + TX at my station: DX station does nothing at all. |  | ok ui/options/track.rs:135 |  |
| label157 | Label | Constant RX: Both stations compensate their own TX frequency only. |  | ok ui/options/track.rs:135 |  |
| label156 | Label | Constant TX: Both stations compensate their own RX frequency only. |  | ok ui/options/track.rs:135 |  |
| label155 | Label | You can see the dial frequency from rig here and choose how to compensate Doppler shift on your site and on DX site. |  | ok ui/options/track.rs:118 |  |
| rb_Options_Doppler_Strategy_D | RadioButton |  | CheckedChanged: gb_Options_Doppler_CheckedChanged | ok ui/options/track.rs:135 |  |
| rb_Options_Doppler_Strategy_C | RadioButton |  | CheckedChanged: gb_Options_Doppler_CheckedChanged | ok ui/options/track.rs:135 |  |
| rb_Options_Doppler_Strategy_B | RadioButton |  | CheckedChanged: gb_Options_Doppler_CheckedChanged | ok ui/options/track.rs:135 |  |
| rb_Options_Doppler_Strategy_A | RadioButton |  | CheckedChanged: gb_Options_Doppler_CheckedChanged | ok ui/options/track.rs:135 |  |
| label154 | Label | Doppler Compensation |  | ok ui/options/track.rs:117 |  |
| label153 | Label | DX Station |  | ok ui/options/track.rs:135 |  |
| label152 | Label | My Station |  | ok ui/options/track.rs:135 |  |
| gb_Options_Track_File | GroupBox | Rotator Control File Output |  | ok ui/options/track.rs:79 |  |
| rb_Options_Track_File_None | RadioButton | None | CheckedChanged: gb_Options_Track_File_CheckedChanged | ok ui/options/track.rs:81 |  |
| rb_Options_Track_File_WSJT | RadioButton | WSJT Az/El | CheckedChanged: gb_Options_Track_File_CheckedChanged | ok ui/options/track.rs:83 |  |
| rb_Options_Track_File_Native | RadioButton | Native Az/El | CheckedChanged: gb_Options_Track_File_CheckedChanged | ok ui/options/track.rs:82 |  |
| gb_Options_Track_DDE | GroupBox | Rotator Control DDE Output |  | missing | DDE is Windows-only and is not shown on this platform. |
| rb_Options_Track_DDE_None | RadioButton | None | CheckedChanged: gb_Options_Track_DDE_CheckedChanged | missing | DDE is Windows-only and is not shown on this platform. |
| rb_Options_Track_DDE_HRD | RadioButton | Ham Radio Deluxe (HRDRotator.exe) | CheckedChanged: gb_Options_Track_DDE_CheckedChanged | missing | DDE is Windows-only and is not shown on this platform. |
| gb_Options_Track_UDP | GroupBox | Rotator Control Network Output |  | ok ui/options/track.rs:61 |  |
| label90 | Label | Port: |  | ok ui/options/track.rs:75 |  |
| label89 | Label | Port: |  | ok ui/options/track.rs:68 |  |
| rb_Options_Track_UDP_None | RadioButton | None | CheckedChanged: gb_Options_Track_UDP_CheckedChanged | ok ui/options/track.rs:63 |  |
| rb_Options_Track_UDP_WinTest | RadioButton | UDP Broadcast (Win-Test) Az only | CheckedChanged: gb_Options_Track_UDP_CheckedChanged | ok ui/options/track.rs:67 |  |
| gb_Options_Track_Serial | GroupBox | Rotator Control Serial Output |  | ok ui/options/track.rs:86 |  |
| rb_Options_Track_Serial_None | RadioButton | None | CheckedChanged: gb_Options_Track_Serial_CheckedChanged | ok ui/options/track.rs:88 |  |
| label88 | Label | Baudrate: |  | ok ui/options/track.rs:94 |  |
| label87 | Label | Port: |  | ok ui/options/track.rs:92 |  |
| rb_Options_Track_Serial_GS232_AZEL | RadioButton | GS-232A Az/El | CheckedChanged: gb_Options_Track_Serial_CheckedChanged | ok ui/options/track.rs:90 |  |
| rb_Options_Track_Serial_GS232_AZ | RadioButton | GS-232A Az only | CheckedChanged: gb_Options_Track_Serial_CheckedChanged | ok ui/options/track.rs:89 |  |
| groupBox28 | GroupBox | Activate Tracking | Enter: tab_Options_Track_Enter | ok ui/options/track.rs:50 |  |
| label186 | Label | s |  | ok ui/options/track.rs:54 |  |
| label187 | Label | Tracking Time Offset: |  | ok ui/options/track.rs:54 |  |
| label185 | Label | ms |  | missing | No track refresh-rate field. |
| label151 | Label | Track Refresh Rate: |  | missing | No track refresh-rate field. |
| tab_Options_CAT | TabPage | CAT | Enter: tab_Options_CAT_Enter | ok ui/options/cat.rs:41 |  |
| gb_Options_CAT_PortSettings | GroupBox | Serial Port Settings |  | missing | Serial port settings are shown only for the native hamlib engine. |
| ud_Options_CAT_Timeout | NumericUpDown |  | ValueChanged: ud_Options_CAT_Timeout_ValueChanged | ok ui/options/cat.rs:176 |  |
| label149 | Label | Timeout [ms]: |  | ok ui/options/cat.rs:176 |  |
| ud_Options_CAT_Poll | NumericUpDown |  | ValueChanged: ud_Options_CAT_Poll_ValueChanged | ok ui/options/cat.rs:49 |  |
| label190 | Label | Poll Int [ms]: |  | ok ui/options/cat.rs:49 |  |
| label191 | Label | DTR: |  | missing | Serial flow-control settings are shown only for the native hamlib engine. |
| cb_Options_CAT_DTR | ComboBox |  | SelectedIndexChanged: cb_Options_CAT_DTR_SelectedIndexChanged | missing | Serial flow-control settings are shown only for the native hamlib engine. |
| label192 | Label | RTS: |  | missing | Serial flow-control settings are shown only for the native hamlib engine. |
| cb_Options_CAT_RTS | ComboBox |  | SelectedIndexChanged: cb_Options_CAT_RTS_SelectedIndexChanged | missing | Serial flow-control settings are shown only for the native hamlib engine. |
| label193 | Label | Stopbits: |  | missing | Serial port settings are shown only for the native hamlib engine. |
| cb_Options_CAT_StopBits | ComboBox |  | SelectedIndexChanged: cb_Options_CAT_Stopbits_SelectedIndexChanged | missing | Serial port settings are shown only for the native hamlib engine. |
| label194 | Label | Parity: |  | missing | Serial port settings are shown only for the native hamlib engine. |
| cb_Options_CAT_Parity | ComboBox |  | SelectedIndexChanged: cb_Parity_SelectedIndexChanged | missing | Serial port settings are shown only for the native hamlib engine. |
| label195 | Label | Databits: |  | missing | Serial port settings are shown only for the native hamlib engine. |
| cb_Options_CAT_DataBits | ComboBox |  | SelectedIndexChanged: cb_Options_CAT_DataBits_SelectedIndexChanged | missing | Serial port settings are shown only for the native hamlib engine. |
| label196 | Label | Baudrate: |  | missing | Serial port settings are shown only for the native hamlib engine. |
| cb_Options_CAT_Baudrate | ComboBox |  | SelectedIndexChanged: cb_Options_CAT_Baudrate_SelectedIndexChanged | missing | Serial port settings are shown only for the native hamlib engine. |
| label197 | Label | PortName: |  | missing | Serial port settings are shown only for the native hamlib engine. |
| cb_Options_CAT_PortName | ComboBox |  | SelectedIndexChanged: cb_Options_CAT_PortName_SelectedIndexChanged | missing | Serial port settings are shown only for the native hamlib engine. |
| gb_Options_CAT_RigType | GroupBox | Rig Type |  | ok ui/options/cat.rs:88 |  |
| label199 | Label | Select CAT-Engine  Rig-Type: |  | ok ui/options/cat.rs:92 |  |
| cb_Options_CAT_Rig | ComboBox |  | SelectedIndexChanged: cb_Options_CAT_Rig_SelectedIndexChanged | ok ui/options/cat.rs:97 |  |
| gb_Options_CAT_OperatingInstructions | GroupBox | Operating Instructions |  | ok ui/options/cat.rs:70 |  |
| groupBox57 | GroupBox | Activate CAT Interface |  | ok ui/options/cat.rs:42 |  |
| ud_Options_CAT_Update | NumericUpDown |  |  | ok ui/options/cat.rs:49 |  |
| label189 | Label | CAT Refresh [ms]: |  | ok ui/options/cat.rs:49 |  |
| cb_Options_CAT_Activate | CheckBox | Activate CAT Interface | CheckedChanged: cb_Options_CAT_Activate_CheckedChanged | ok ui/options/cat.rs:44 |  |
| tab_Options_Watchlist | TabPage | Watchlist | Enter: tab_Options_Watchlist_Enter | ok ui/options/watchlist.rs:8 |  |
| groupBox50 | GroupBox | Info |  | ok ui/options/watchlist.rs:8 |  |
| label37 | Label | Watchlist allows you to pick a number of stations out of your database.  This is very helpful especially when working in Multi-Path Mode.   All stations on watchlist can be shown on map with small markers and callsign labels for quick showing/hiding path to them.  If a station is selected from watchlist by checking the assigned checkbox, path is shown on map and entry is coloured according to aircraft scatter potential.  Watchlist can be synchronized with ON4KST chat user list instead of a local managed list to show all stations currently logged in in chat (Needs wtKST software and network functions enabbled). |  | ok ui/options/watchlist.rs:8 |  |
| btn_Options_Watchlist_Clear | Button | Clear Watchlist | Click: btn_Options_Watchlist_Clear_Click | ok ui/options/watchlist.rs:20 |  |
| groupBox17 | GroupBox | General |  | ok ui/options/watchlist.rs:9 |  |
| label31 | Label | Number of entrys to keep in list: |  | ok ui/options/watchlist.rs:10 |  |
| btn_Options_Watchlist_Manage | Button | Manage Watchlist | Click: btn_Options_Watchlist_Manage_Click | ok ui/multi.rs:58 |  |
| tab_Options_Misc | TabPage | Misc. |  | missing | There is no Misc tab; its actions are not ported. |
| groupBox35 | GroupBox | News Feed |  | missing | The news feed is not ported. |
| label143 | Label | You can change various settings here or do miscellaneous actions here. Most of them were not directly related to one of the Options categories. |  | missing | The Misc tab is not ported. |
| groupBox15 | GroupBox | Open Directories |  | missing | Opening directories is not ported. |
| tab_Options_Info | TabPage | Info | Enter: tab_Options_Info_Enter | ok ui/options/info.rs:10 |  |
| label105 | Label | ASTER GDEM is a product of METI and NASA |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label30 | Label | ASTER GDEM is a product of METI and NASA |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label45 | Label | USGS-authored or produced data and information are in the public domain |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| lbl_Options_Elevation_SRTM1 | LinkLabel | 1arsec (30m x 30m) Elevation Data from  SRTM - Project and ASTER |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label17 | Label | USGS-authored or produced data and information are in the public domain |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| lbl_Options_Elevation_SRTM3 | LinkLabel | 3arsec (90m x 90m) Elevation Data from  SRTM - Project and ASTER |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| lbl_Options_Elevation_GLOBE | LinkLabel | 1km based Elevation Data from GLOBE - Project |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| lbl_Options_Spherical | LinkLabel | http://www.movable-type.co.uk/scripts/latlong.html |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label51 | Label | Spherical: |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label25 | Label | special tnx to DF9IC and DL8AAU for extensive discussions and testing |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| lbl_Options_Map | LinkLabel | GMap.NET Copyright (c) 2008 - 2011 Universe |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label27 | Label | Maps library: |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label26 | Label | Idea: |  | ok ui/options/info.rs:18 | Attribution lives in the NOTICE text. |
| label24 | Label | DL2ALF |  | ok ui/options/info.rs:15 |  |
| label23 | Label | Free for personal ham radio related use. |  | ok ui/options/info.rs:13 |  |
| lbl_Options_Version | Label | Version: x.x.x.x |  | ok ui/options/info.rs:12 |  |
| label20 | Label | Aircraft scatter prediction (c) 2014 - 2020 DL2ALF |  | ok ui/options/info.rs:15 |  |
| label19 | Label | AirScout |  | ok ui/options/info.rs:11 |  |
| btn_Options_License | Button | Click here to see the complete license information. | Click: btn_Options_License_Click | ok ui/options/info.rs:14 |  |
| tsl_Options_Status | ToolStripStatusLabel | Status |  | ok ui/options/mod.rs:297 | Validation errors are shown above OK/Cancel. |
| groupBox39 | GroupBox | groupBox39 |  | missing | Empty placeholder group under General; not rendered. |
| groupBox58 | GroupBox | User supplied Overlay |  | missing | The user-supplied overlay image is not ported. |
| label34 | Label | Max. Lat: |  | missing | The user-supplied overlay is not ported. |
| label200 | Label | Min. Lat: |  | missing | The user-supplied overlay is not ported. |
| label204 | Label | Max. Lon: |  | missing | The user-supplied overlay is not ported. |
| label208 | Label | Min. Lon: |  | missing | The user-supplied overlay is not ported. |
| tb_Coverage_MaxLat | DoubleTextBox |  | TextChanged: tab_Options_General_Update | ok ui/options/general.rs:12 |  |
| tb_Coverage_MinLat | DoubleTextBox |  | TextChanged: tab_Options_General_Update | ok ui/options/general.rs:11 |  |
| tb_Coverage_MaxLon | DoubleTextBox |  | TextChanged: tab_Options_General_Update | ok ui/options/general.rs:14 |  |
| tb_Coverage_MinLon | DoubleTextBox |  | TextChanged: tab_Options_General_Update | ok ui/options/general.rs:13 |  |
| cb_Options_Background_Calculations_Enable | CheckBox | Enable Background Pre-Calculations for paths/horizons etc. |  | ok ui/options/database.rs:55 |  |
| ud_Options_Database_Update_Period | NumericUpDown |  |  | ok ui/options/database.rs:51 |  |
| cb_Options_Locations_RestrictToAreaOfInterest | CheckBox | Restrict locations to current Area of Interest |  | missing | No Options field; the setting is applied to background location updates. |
| cb_Options_SmallLettersForSubSquares | CheckBox | Small letters for subsquares: | CheckedChanged: cb_Options_SmallLettersForSubSquares_CheckedChanged | missing | No checkbox UI; the setting is fixed at its default. |
| cb_Options_Locator_AutoLength | CheckBox | Cut locator to significant digits automatically (Minimum 6 digits): |  | missing | Fixed at its C# default; no Options field. |
| ud_Options_Locator_MaxLength | NumericUpDown |  |  | missing | Fixed at its C# default (10); no Options field. |
| tb_Options_DXLoc | LocatorTextBox |  | TextChanged: tb_Options_DXLoc_TextChanged | ok ui/options/stations.rs:342 |  |
| tb_Options_MyLoc | LocatorTextBox |  | TextChanged: tb_Options_MyLoc_TextChanged | ok ui/options/stations.rs:342 |  |
| cb_Options_Gauges_Show | CheckBox | Show large Tracking Display on map |  | ok ui/options/track.rs:108 |  |
| ud_Options_Gauges_Width | NumericUpDown |  |  | missing | Gauge width has no UI. |
| cb_Options_Map_SmallMarkers | CheckBox | Use Small Markers for all Path Marks |  | missing | Small-marker toggle is not ported. |
| cb_Options_Map_LabelCalls | CheckBox | Show Labels with Callsign at Locations |  | ok ui/options/map.rs:52 |  |
| cb_Options_ShowUserSuppliedOverlay | CheckBox | User Supplied |  | missing | The user-supplied overlay is not ported. |
| cb_Options_Beamwidths_Activate | CheckBox | Beamwidths |  | ok ui/options/map.rs:51 |  |
| cb_Options_Distances_Activated | CheckBox | Distance Circles |  | missing | Distance-circles toggle is not ported. |
| cb_Options_Locators_Activate | CheckBox | Locator Grid |  | missing | Locator-grid toggle is not ported. |
| cb_Options_Watchlist_Activate | CheckBox | Watchlist Locations |  | missing | Watchlist-locations toggle is not ported. |
| cb_Options_Airports_Activate | CheckBox | Airports |  | missing | Airports toggle is not ported. |
| cb_Options_InfoWin_MyElevation | CheckBox | My Elevation |  | ok ui/options/planes.rs:67 |  |
| cb_Options_InfoWin_MyAzimuth | CheckBox | My Azimuth |  | ok ui/options/planes.rs:66 |  |
| cb_Options_InfoWin_MyDoppler | CheckBox | My Doppler |  | ok ui/options/planes.rs:68 |  |
| cb_Options_InfoWin_DXDoppler | CheckBox | DX Doppler |  | ok ui/options/planes.rs:71 |  |
| cb_Options_InfoWin_DXAzimuth | CheckBox | DX Azimuth |  | ok ui/options/planes.rs:69 |  |
| cb_Options_InfoWin_DXElevation | CheckBox | DX Elevation |  | ok ui/options/planes.rs:70 |  |
| cb_Options_InfoWin_Dist | CheckBox | Crossing Distance |  | ok ui/options/planes.rs:63 |  |
| cb_Options_InfoWin_Time | CheckBox | Crossing Time |  | ok ui/options/planes.rs:57 |  |
| cb_Options_InfoWin_Epsilon | CheckBox | Elevation Angles |  | ok ui/options/planes.rs:58 |  |
| cb_Options_InfoWin_Squint | CheckBox | Squint Angle |  | ok ui/options/planes.rs:59 |  |
| cb_Options_InfoWin_Angle | CheckBox | Crossing Angle |  | ok ui/options/planes.rs:65 |  |
| rb_Options_InfoWin_Imperial | RadioButton | Imperial |  | ok ui/options/planes.rs:55 |  |
| rb_Options_InfoWin_Metric | RadioButton | Metric |  | ok ui/options/planes.rs:54 |  |
| tb_Options_Map_ToolTipFont | TextBox |  |  | missing | Tooltip font is not ported. |
| cb_Options_InfoWin_Position | CheckBox | Plane Position |  | ok ui/options/planes.rs:56 |  |
| cb_Options_InfoWin_Alt | CheckBox | Plane Altitude |  | ok ui/options/planes.rs:62 |  |
| cb_Options_InfoWin_Track | CheckBox | Plane Track |  | ok ui/options/planes.rs:60 |  |
| cb_Options_InfoWin_Type | CheckBox | Plane Type |  | ok ui/options/planes.rs:61 |  |
| cb_Options_InfoWin_Speed | CheckBox | Plane Speed |  | ok ui/options/planes.rs:64 |  |
| cb_Options_Map_Preloader_Enabled | CheckBox | Enable Map Preloading |  | ok ui/options/map.rs:56 |  |
| ud_Options_Map_Preloader_MaxZoom | NumericUpDown |  |  | ok ui/options/map.rs:63 |  |
| cb_Options_Elevation_GLOBE_EnableCache | CheckBox | Keep downloaded elevation tiles |  | missing | Per-model tile cache not ported; one global download toggle. |
| cb_Options_Elevation_GLOBE | CheckBox | Use GLOBE elevation data |  | ok ui/options/elevation.rs:30 |  |
| cb_Options_Elevation_SRTM3_EnableCache | CheckBox | Keep downloaded elevation tiles |  | missing | Per-model tile cache not ported; one global download toggle. |
| cb_Options_Elevation_SRTM3 | CheckBox | Use SRTM3 elevation data |  | ok ui/options/elevation.rs:30 |  |
| cb_Options_Elevation_SRTM1_EnableCache | CheckBox | Keep downloaded elevation tiles |  | missing | Per-model tile cache not ported; one global download toggle. |
| cb_Options_Elevation_SRTM1 | CheckBox | Use SRTM1 elevation data |  | ok ui/options/elevation.rs:30 |  |
| cb_Options_Elevation_ASTER3_EnableCache | CheckBox | Keep downloaded elevation tiles |  | missing | Per-model tile cache not ported; one global download toggle. |
| cb_Options_Elevation_ASTER3 | CheckBox | Use ASTER3 elevation data |  | ok ui/options/elevation.rs:30 |  |
| cb_Options_Elevation_ASTER1_EnableCache | CheckBox | Keep downloaded elevation tiles |  | missing | Per-model tile cache not ported; one global download toggle. |
| cb_Options_Elevation_ASTER1 | CheckBox | Use ASTER1 elevation data |  | ok ui/options/elevation.rs:30 |  |
| cb_Options_Path_BestCaseElevation | CheckBox | Use best case elevation from grid square for both stations if precise location is unknown |  | ok ui/options/path.rs:42 |  |
| cb_Options_TracePositions | CheckBox | Trace aircraft positions to files (Caution: this will cause a lot of files in <AirScout's Temp directory>/PlanePositions!) |  | missing | Tracing positions to files is not ported. |
| cb_Options_Planes_ExtendedPlausibilityCheck_Enabled | CheckBox | Enable extended plausibility check of aircraft info |  | missing | No Options field; the setting feeds the feed worker only. |
| cb_Options_Planes_LogErrors | CheckBox | Log errors as warnings to AirScout's main logfile |  | missing | Feed log-errors toggle is not ported. |
| cb_Options_Planes_KeepHistory | CheckBox | Keep Plane Position History |  | missing | No Options field; the setting feeds the feed worker only. |
| ud_Options_Planes_Position_DatabaseLifetime | NumericUpDown |  |  | missing | Keeping positions for N days is not ported. |
| cb_Options_Alarm_Activate | CheckBox | Activate Alarm |  | ok ui/options/alarm.rs:9 |  |
| cb_Options_Alarm_PlaySound | CheckBox | Play sound |  | ok ui/options/alarm.rs:19 |  |
| cb_Options_Server_Activate | CheckBox | Activate Network Server |  | ok ui/options/network.rs:12 |  |
| tb_Options_Server_Name | TextBox |  |  | ok ui/options/network.rs:24 |  |
| tb_SpecLab_FileName | TextBox |  |  | ok ui/options/speclab.rs:12 |  |
| tb_SpecLab_URL | TextBox |  |  | ok ui/options/speclab.rs:11 |  |
| cb_SpecLab_Enabled | CheckBox | Activate Connection |  | ok ui/options/speclab.rs:9 |  |
| tb_Options_Track_Serial_Port | TextBox |  |  | ok ui/options/track.rs:92 |  |
| cb_Options_Track_Activate | CheckBox | Activate Tracking | CheckedChanged: cb_Options_Track_Activate_CheckedChanged | ok ui/options/track.rs:50 |  |
| cb_Options_Watchlist_SyncWithKST | CheckBox | Keep in sync with KST user list (needs wtKST > V3.1 and network functions activated): | CheckedChanged: cb_Options_Watchlist_SyncWithKST_CheckedChanged | ok ui/options/watchlist.rs:15 |  |
| cb_Options_NewsFeed | CheckBox | Notify me on news published by www.airscout.eu |  | missing | The news feed is not ported. |
| label209 | Label | Filename: |  | missing | The user-supplied overlay is not ported. |
| label210 | Label | Opacity: |  | missing | The user-supplied overlay is not ported. |
| tb_Options_UserSupplied_Filename | TextBox |  |  | missing | The user-supplied overlay is not ported. |
| btn_Options_UserSupplied_Filename | Button | Select | Click: btn_Options_UserSupplied_Filename_Click | missing | The user-supplied overlay is not ported. |
| labelMaxDelta | Label | Max. el. delta path to plane |  | ok ui/options/planes.rs:38 |  |

## PercentageControl

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|

## PlaneFeedDisclaimerDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| btn_DeepLinkDlg_Accept | Button | Accept |  | ok ui/dialogs/disclaimer.rs:35 |  |
| btn_DeepLinkDlg_Decline | Button | Decline |  | ok ui/dialogs/disclaimer.rs:38 |  |

## PlaneFeedSettingsDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| gb_Info | GroupBox | Info |  | missing | No Info group; the editor is a generic table. |
| lbl_Info | Label | PlaneFeed Info |  | missing | No feed info text; the generic editor shows the key/value table. |
| gb_Properties | GroupBox | Settings |  | ok ui/dialogs/feed_settings.rs:103 |  |
| gb_Version | GroupBox | Version |  | missing | No version group; the editor is a generic table. |
| lbl_Version | Label | PlaneFeed Version |  | missing | No version label; the editor is a generic table. |

## PlaneHistoryDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| label1 | Label | Hex: |  | ok ui/dialogs/plane_history.rs:102 |  |
| label2 | Label | Call: |  | ok ui/dialogs/plane_history.rs:106 |  |
| label3 | Label | or |  | ok ui/dialogs/plane_history.rs:105 |  |
| cb_Hex | ComboBox |  | TextChanged: cb_Hex_TextChanged | ok ui/dialogs/plane_history.rs:102 |  |
| cb_Call | ComboBox |  | TextChanged: cb_Call_TextChanged | ok ui/dialogs/plane_history.rs:106 |  |
| btn_Export | Button | Export | Click: btn_Export_Click | ok ui/dialogs/plane_history.rs:111 |  |
| tsl_Status | ToolStripStatusLabel | Status |  | ok ui/dialogs/plane_history.rs:120 | Status is shown as a label. |
| label4 | Label | Use this dialog to export a single aircraft's position history.  You can select the aircraft by Hex or by Call. |  | ok ui/dialogs/plane_history.rs:85 |  |
| btn_Close | Button | Close |  | ok ui/dialogs/plane_history.rs:116 |  |

## ScoutBaseDatabaseMaintenanceDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| tsl_Status | ToolStripStatusLabel | Status |  | ok ui/dialogs/db_maintenance.rs:133 | Progress and errors are inline labels. |
| groupBox1 | GroupBox | Tables |  | ok ui/dialogs/db_maintenance.rs:136 |  |
| ch_TableName | ColumnHeader | TableName |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Description | ColumnHeader | Description |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Entries | ColumnHeader | Entries |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| ch_Action | ColumnHeader | Action |  | ok ui/dialogs/db_maintenance.rs:137 |  |
| lbl_Info | Label | The Scoutbase database is containing location and QRV infos. Each information has a unique database key:  callsign and 6-digit Maidenhead locator square.   The database is periodically updated from a global web database. The operator can modify/add entries. |  | ok ui/dialogs/db_maintenance.rs:131 |  |

## SetTimeDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| label1 | Label | You can set online/offline mode here and set a start time for offline mode. |  | ok ui/dialogs/set_time.rs:40 |  |
| btn_SetTimeDlg_OK | Button | OK |  | ok ui/dialogs/set_time.rs:50 |  |
| btn_SetTimeDlg_Cancel | Button | Cancel |  | ok ui/dialogs/set_time.rs:58 |  |
| cb_Time_Online | CheckBox | Online mode: use current PC time (Default). | CheckedChanged: cb_Time_Online_CheckedChanged | ok ui/dialogs/set_time.rs:40 |  |

## Splash

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| pb_Main | PictureBox |  | Click: pb_Main_Click, Paint: pb_Main_Paint | missing | The splash screen is not ported. |

## TrafficDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| groupBox1 | GroupBox | Traftic Map |  | ok ui/dialogs/traffic.rs:205 |  |
| gm_Options_Traffic | GMapControl |  | OnRouteEnter: gm_Options_Traffic_OnRouteEnter | ok ui/dialogs/traffic.rs:205 |  |
| groupBox2 | GroupBox | Traffic Data |  | ok ui/dialogs/traffic.rs:173 |  |
| btn_Options_Traffic_Show | Button | Show | Click: btn_Options_Traffic_Show_Click | ok ui/dialogs/traffic.rs:183 |  |
| btn_Options_Traffic_Close | Button | Close | Click: btn_Options_Traffic_Close_Click | ok ui/dialogs/traffic.rs:186 |  |
| tsl_Main | ToolStripStatusLabel | Press Show to start calculation. |  | ok ui/dialogs/traffic.rs:197 | Status is shown as a label. |
| label1 | Label | From: |  | ok ui/dialogs/traffic.rs:174 |  |
| label2 | Label | To: |  | ok ui/dialogs/traffic.rs:174 |  |
| label3 | Label | Opacity[%]: |  | ok ui/dialogs/traffic.rs:179 |  |
| ud_Opacity | NumericUpDown |  |  | ok ui/dialogs/traffic.rs:180 |  |

## WatchlistDlg

| Control | Kind | Text | C# handlers | Rust | Notes |
|---|---|---|---|---|---|
| tsl_Watchlist_Main | ToolStripStatusLabel | tsl_Main |  | missing | No status strip; the lists load inline. |
| btn_Watchlist_Add | Button | > | Click: btn_Watchlist_Add_Click | ok ui/dialogs/watchlist.rs:151 |  |
| btn_Watchlist_Remove | Button | < | Click: btn_Watchlist_Remove_Click | ok ui/dialogs/watchlist.rs:155 |  |
| btn_Watchlist_RemoveAll | Button | << | Click: btn_Watchlist_RemoveAll_Click | ok ui/dialogs/watchlist.rs:158 |  |
| btn_Watchlist_Cancel | Button | Cancel |  | ok ui/dialogs/watchlist.rs:182 |  |
| btn_Watchlist_OK | Button | OK |  | ok ui/dialogs/watchlist.rs:179 |  |
| label1 | Label | You can select callsigns from database and set them on a watchlist. |  | ok ui/dialogs/watchlist.rs:130 |  |
| tb_Watchlist_Callsigns | TextBox |  | TextChanged: tb_Watchlist_Callsigns_TextChanged | ok ui/dialogs/watchlist.rs:93 |  |
| tb_Watchlist_Selected | TextBox |  | TextChanged: tb_Watchlist_Selected_TextChanged | ok ui/dialogs/watchlist.rs:93 |  |
| dgv_Watchlist_Callsigns | DataGridView |  | CellContentDoubleClick: dgv_Watchlist_Callsigns_CellContentDoubleClick, CellPainting: dgv_Watchlist_Callsigns_CellPainting | ok ui/dialogs/watchlist.rs:104 |  |
| dgv_Watchlist_Selected | DataGridView |  | CellContentDoubleClick: dgv_Watchlist_Selected_CellContentDoubleClick, CellPainting: dgv_Watchlist_Selected_CellPainting | ok ui/dialogs/watchlist.rs:104 |  |
| label2 | Label | Available Callsigns |  | ok ui/dialogs/watchlist.rs:135 |  |
| label3 | Label | Selected Callsigns |  | ok ui/dialogs/watchlist.rs:163 |  |

## Summary

| Window | rows | ok | missing | not ported |
|---|---|---|---|---|
| AircraftDatabaseMaintenanceDlg | 7 | 7 | 0 | 0 |
| CleanupDlg | 10 | 7 | 2 | 1 |
| CrossingHistoryDlg | 8 | 8 | 0 | 0 |
| DatabaseEditDlg | 5 | 4 | 1 | 0 |
| DatabaseMaintenanceDlg | 8 | 8 | 0 | 0 |
| DeleteSingleStationDlg | 7 | 7 | 0 | 0 |
| ElevationCopyrightDlg | 0 | 0 | 0 | 0 |
| FirstRunWizard | 67 | 36 | 31 | 0 |
| HistoryFromURLDlg | 12 | 11 | 1 | 0 |
| HorizonDlg | 28 | 26 | 2 | 0 |
| LicenseDlg | 1 | 1 | 0 | 0 |
| LocalObstructionsDlg | 4 | 4 | 0 | 0 |
| MapDlg | 113 | 108 | 5 | 0 |
| MapProviderDlg | 2 | 0 | 2 | 0 |
| MapStationDlg | 19 | 13 | 6 | 0 |
| OptionsDlg | 503 | 377 | 123 | 3 |
| PercentageControl | 0 | 0 | 0 | 0 |
| PlaneFeedDisclaimerDlg | 2 | 2 | 0 | 0 |
| PlaneFeedSettingsDlg | 5 | 1 | 4 | 0 |
| PlaneHistoryDlg | 9 | 9 | 0 | 0 |
| ScoutBaseDatabaseMaintenanceDlg | 7 | 7 | 0 | 0 |
| SetTimeDlg | 4 | 4 | 0 | 0 |
| Splash | 1 | 0 | 1 | 0 |
| TrafficDlg | 10 | 10 | 0 | 0 |
| WatchlistDlg | 13 | 12 | 1 | 0 |

