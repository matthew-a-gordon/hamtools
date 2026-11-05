# Missing data: HF band plan, privilege re-check, POTA/SOTA shapes, export rules

Researched 2026-11-04 (local; server `Date` headers read 2026-11-05 01:23 UTC). Convention: `docs/research/<topic>.md`, as on the `research/*` branches (`research/us-band-plan` has `docs/research/us-band-plan.md`); this directory does not yet exist on `integration/spec-54`, so this creates it. No source files were changed.

> Not a compliance tool. Privilege data must be checked against the operator's licence and the current rules. Anything not read from a primary source is marked **UNVERIFIED**.

Method notes: eCFR was read through its own versioner API (`https://www.ecfr.gov/api/versioner/v1/full/2026-11-03/title-47.xml?part=97&section=97.30X`, needs `Accept-Encoding`), and the section text matches what the web page shows. `api.pota.app` answered HTTP 200 to `curl -A hamtools-research` (the earlier POTA note got 403 from a different fetcher). The SOTA site is a JavaScript app, so the CSV V2 spec was read from the Angular bundle that sotadata.org.uk itself serves (`https://www.sotadata.org.uk/en/main.140dc1ea7ec661f3.js`): the "CSV info" page text and the uploader's own parsing code. That is SOTA's own text and code, but I did not find a standalone HTML spec page.

## 1. ARRL voluntary HF band plan

Sources: ARRL band plan, <https://www.arrl.org/band-plan> (retrieved 2026-11-04), and for the legal CW/data versus phone split, 47 CFR 97.305, <https://www.ecfr.gov/current/title-47/section-97.305> (text read as of 2026-11-03; history ends `91 FR 1431, Jan. 14, 2026`).

Findings that change the plan:

- **The ARRL page does not give a full CW / digital / phone map for HF.** For 160, 80, 40, 30, 20, 17, 15 and 12 m it lists only a few sub-segments and calling points (RTTY/data, packet, SSTV, AM, DX windows). Only 10 m and 6 m have a near-complete map. The CW stretch at the bottom of each band is not stated anywhere on that page. The graphical chart (`https://www.arrl.org/graphical-frequency-allocations`, PDF `.../Hambands4_Color_11x8_5.pdf`) was downloaded but I had no PDF text extractor, so I did not read it: **UNVERIFIED**.
- The legal split comes from 97.305(c): the lower part of each band is "RTTY, data" (CW is allowed anywhere under 97.305(a)), the upper part "Phone, image". Those edges are primary and are the `fcc-97.305` rows below.
- The ARRL band-plan page says it is a voluntary division and that locally coordinated plans take precedence. It states no "as of" date; the sibling page <https://www.arrl.org/frequency-allocations> says "Updated - 14 April 2022", so it is **stale against the current rules** (see section 2) and I did not use it for edges.
- All 17 rows now in `arrl.toml` match this page (160 m digital, 80 m, 40 m, 30 m, 20 m RTTY and packet, 10 m CW and phone, 6 m, 2 m, 70 cm). The 20 m packet row `14.1005-14.112` matches. Missing from `arrl.toml`: everything below marked "new".

Rows (Hz). "Usual mode" is the rig mode a user would want; `phone` means LSB below 10 MHz and USB above. `arrl` = ARRL page; `fcc-97.305` = eCFR; a point entry has start = end.

| band | start Hz | end Hz | usual mode | source |
|---|---|---|---|---|
| 160m | 1800000 | 1810000 | digital | arrl (in toml) |
| 160m | 1810000 | 1810000 | CW | arrl ("CW QRP" point, new) |
| 160m | 1810000 | 1843000 | CW | UNVERIFIED (the page says "1.800-2.000 CW" and starts SSB at 1.843; the CW edge between is inferred) |
| 160m | 1843000 | 2000000 | phone | arrl ("SSB, SSTV and other wideband modes", new) |
| 160m | 1910000 | 1910000 | phone | arrl (SSB QRP, new) |
| 160m | 1995000 | 2000000 | none | arrl (experimental; beacons 1999000-2000000, new) |
| 80m | 3500000 | 3570000 | CW | UNVERIFIED (not on the ARRL page); legal: data/CW per fcc-97.305 |
| 80m | 3570000 | 3600000 | RTTY | arrl (in toml; DX RTTY point 3590000, new) |
| 80m | 3600000 | 3790000 | phone | fcc-97.305 (75 m is phone) |
| 80m | 3790000 | 3800000 | phone | arrl (DX window, new) |
| 80m | 3800000 | 4000000 | phone | fcc-97.305; arrl points: SSTV 3845000, AM calling 3885000 (new) |
| 60m | 5330500 | 5330500 | phone (USB) | arrl channel; fcc-97.303(h)(3) |
| 60m | 5346500 | 5346500 | phone (USB) | same |
| 60m | 5351500 | 5366500 | phone (USB) | same (segment, 15 kHz) |
| 60m | 5371500 | 5371500 | phone (USB) | same |
| 60m | 5403500 | 5403500 | phone (USB) | same |
| 40m | 7000000 | 7040000 | CW | UNVERIFIED (not on the ARRL page); legal data/CW 7.000-7.125 |
| 40m | 7040000 | 7040000 | RTTY | arrl (RTTY/Data DX point, new) |
| 40m | 7080000 | 7125000 | RTTY | arrl (in toml) |
| 40m | 7125000 | 7300000 | phone | fcc-97.305(c)(3)(vii); arrl points: SSTV 7171000, AM calling 7290000 (new) |
| 30m | 10100000 | 10130000 | CW | UNVERIFIED (not on the ARRL page); the whole band is data/CW only per fcc-97.305 |
| 30m | 10130000 | 10140000 | RTTY | arrl (in toml) |
| 30m | 10140000 | 10150000 | digital (packet) | arrl (in toml) |
| 20m | 14000000 | 14070000 | CW | UNVERIFIED (not on the ARRL page); legal data/CW to 14150000 |
| 20m | 14070000 | 14095000 | RTTY | arrl (in toml) |
| 20m | 14095000 | 14099500 | digital (packet) | arrl ("14.095-14.0995", new) |
| 20m | 14100000 | 14100000 | none | arrl (NCDXF beacons point, new) |
| 20m | 14100500 | 14112000 | digital (packet) | arrl (in toml) |
| 20m | 14150000 | 14350000 | phone | fcc-97.305(c)(3)(x); arrl points: SSTV 14230000, AM calling 14286000 (new) |
| 17m | 18068000 | 18100000 | CW | UNVERIFIED (not on the ARRL page); legal data/CW to 18110000 |
| 17m | 18100000 | 18105000 | RTTY | arrl (new) |
| 17m | 18105000 | 18110000 | digital (packet) | arrl (new) |
| 17m | 18110000 | 18168000 | phone | fcc-97.305(c)(3)(xii) |
| 15m | 21000000 | 21070000 | CW | UNVERIFIED (not on the ARRL page); legal data/CW to 21200000 |
| 15m | 21070000 | 21110000 | RTTY | arrl ("RTTY/Data", new) |
| 15m | 21200000 | 21450000 | phone | fcc-97.305(c)(3)(xiv); arrl point: SSTV 21340000 (new) |
| 12m | 24890000 | 24920000 | CW | UNVERIFIED (not on the ARRL page); legal data/CW to 24930000 |
| 12m | 24920000 | 24925000 | RTTY | arrl (new) |
| 12m | 24925000 | 24930000 | digital (packet) | arrl (new) |
| 12m | 24930000 | 24990000 | phone | fcc-97.305(c)(3)(xvi) |
| 10m | 28000000 | 28070000 | CW | arrl (in toml) |
| 10m | 28070000 | 28150000 | RTTY | arrl (new) |
| 10m | 28150000 | 28190000 | CW | arrl (in toml) |
| 10m | 28200000 | 28300000 | CW | arrl ("Beacons"; new) |
| 10m | 28300000 | 29300000 | phone | arrl (in toml); points: SSTV 28680000; AM 29000000-29200000 (new) |
| 10m | 29300000 | 29510000 | none | arrl (satellite uplinks or downlinks, new) |
| 10m | 29520000 | 29590000 | FM | arrl (repeater inputs; toml has 29520000-29700000 as one FM row) |
| 10m | 29600000 | 29600000 | FM | arrl (FM simplex point, new) |
| 10m | 29610000 | 29700000 | FM | arrl (repeater outputs) |

Legal edges (fcc-97.305(c)(3), eCFR, 2026-11-03), useful as a second layer or for checking the above: 160 m entire band data and phone; 80 m entire band RTTY/data; 75 m (3.600-4.000) phone; 60 m phone/RTTY/data/CW on the channels and segment; 40 m data 7.000-7.100 and 7.100-7.125, phone 7.125-7.300, and phone 7.075-7.100 only for stations in Regions 1/3 or Region 2 west of 130 W or south of 20 N (so not in the continental US: 97.305(f)(11)); 30 m data only; 20 m data 14.000-14.150, phone 14.150-14.350; 17 m data 18.068-18.110, phone 18.110-18.168; 15 m data 21.000-21.200, phone 21.200-21.450; 12 m data 24.890-24.930, phone 24.930-24.990; 10 m data 28.000-28.300, phone 28.300-28.500 (97.305(f)(10) limits Novice/Technician to CW and J3E/R3E), 28.500-29.000 phone, 29.000-29.700 phone (no RTTY/data).

## 2. Re-check of `privileges.toml` against the eCFR

Sources: 47 CFR 97.301 <https://www.ecfr.gov/current/title-47/section-97.301>, 97.303 <https://www.ecfr.gov/current/title-47/section-97.303>, 97.305, all read 2026-11-04 as the 2026-11-03 version. The eCFR versions API gives 2026-02-13 as the latest 97.301 version; the source note ends `91 FR 1430, Jan. 14, 2026`. So the file's `amended = "91 FR 1430"` is **confirmed** for 97.301 and 97.303.

Table reading: General's 75 m row has two values (`3.800-4.000`, `3.800-3.900`), the first column being blank for Region 1; Region 2 is `3.800-4.000`, which is consistent with the Extra and Advanced rows. 97.301(a) is Technician, General, Advanced and Extra (Novice is not in it). 97.301(e) is "Novice Class or Technician Class" and lists only 80, 40, 15, 10 m, 1.25 m 222-225 and 23 cm 1270-1295.

Discrepancies (rows and the class letters are as written in `privileges.toml`):

1. **6 m 50-54, 2 m 144-148, 70 cm 420-450, 33 cm 902-928 are marked `xagtn`; Novice is wrong.** They come from 97.301(a), which excludes Novice. Correct: `xagt`. (ARRL's own table says "All Amateurs except Novices".)
2. **23 cm 1240-1300 is marked `xagtn` as one row; Novice has only 1270-1295 (97.301(e)).** Correct split: 1240-1270 `xagt`, 1270-1295 `xagtn`, 1295-1300 `xagt`.
3. 1.25 m 222-225 `xagtn` is correct (97.301(a) and (e)). The 219-220 MHz segment (all except Novice, fixed point-to-point digital message forwarding only, 97.303(l)) is not in the file; omission, not an error.
4. Not in the file: 2200 m 135.7-137.8 kHz and 630 m 472-479 kHz (`xag`: 97.301(b)(c)(d), fixed locations only and UTC notification, 97.303(g)), 13 cm and above. Omissions only.
5. Every HF row is correct: 160 m `xag`; 80 m 3.500-3.525 `x`, 3.525-3.600 `xagtn`, 3.600-3.700 `x`, 3.700-3.800 `xa`, 3.800-4.000 `xag`; 40 m 7.000-7.025 `x`, 7.025-7.125 `xagtn`, 7.125-7.175 `xa`, 7.175-7.300 `xag`; 30 m `xag`; 20 m and 15 m splits; 17 m and 12 m `xag`; 10 m 28.0-28.5 `xagtn`, 28.5-29.7 `xag`.
6. **60 m**: the segment `5.3515-5.3665` `xag` is correct (97.301(b)(c)(d)). The channel rows are correct against 97.303(h)(3): amateur stations on 60 m may transmit "only in the 5351.5-5366.5 kHz band and on the four center frequencies" 5332.0, 5348.0, 5373.0 and 5405.0 kHz. Carriers 5330.5, 5346.5, 5371.5 and 5403.5 are right. The comment in the file ("the ARRL page disagrees on the channel count") is **stale**: the eCFR and the ARRL page now agree (four channels and one 15 kHz segment).
7. 60 m mode rule: 97.303(h)(3), phone, data and RTTY (designators 2K80J3E, 2K80J2D, 60H0J2B) may set the carrier 1.5 kHz below the centre frequency; CW (150HA1A) is on the centre frequency; no emission may occupy more than 2.8 kHz. 97.305(c)(3)(iii) and (f)(14): only phone, RTTY, data and CW; RTTY/data must meet 97.309; data and RTTY transmissions must be kept short to avoid interfering with US Government stations. The ARRL page adds a 100 W PEP ERP limit (9.15 W ERP for the 15 kHz segment); I did not locate that in the sections above (it would be in 97.313), so it is **UNVERIFIED** here.
8. Caveat the `x/a/g/t/n` letters hide: Novice and Technician on HF are CW only on 80, 40 and 15 m (97.305(f)(9)) and CW plus J3E/R3E on 10 m 28.3-28.5 (97.305(f)(10)). A band-edge-only file marks `t` on 3.525-3.600 etc., which is correct as a band edge but not as a mode.

## 3. Live shape of the endpoints versus the parsers

Fetched 2026-11-04 ~18:23 local (01:23 UTC on 2026-11-05) with `curl --compressed -A hamtools-research`.

**Spots: `https://api.pota.app/spot/activator`** (200, JSON array, 31 items, served by CloudFront with `age: 6`). Keys on every item: `spotId` (number), `activator`, `frequency` (string, kHz, for example `"7248"`), `mode` (`CW`, `FT8`, `SSB` seen), `reference`, `parkName`, `spotTime` (`"2026-11-05T01:07:38"`, no zone), `spotter`, `comments`, `source`, `invalid` (null), `name`, `locationDesc`, `grid4`, `grid6` (mixed case, `EL89xg`), `latitude`, `longitude` (numbers), `count`, `expire` (number).

- **Mismatch: `parkName` was `null` in all 31 spots; the park name is in `name`.** `spots.rs` reads only `parkName`, so `Spot.park_name` would be blank for every spot. Fix: read `name` first (or as fallback).
- Matches: `frequency` as a string is handled by `number()`; `invalid: null` is not treated as invalid; `comments`, `locationDesc`, `spotter`, `latitude`, `longitude` and `grid6` are as assumed. `spotTime` is UTC (it precedes the 01:23 UTC response by a plausible 15 minutes) and the format `%Y-%m-%dT%H:%M:%S` parses it.
- `expire` is **seconds remaining**: `spotTime` 01:07:38 plus 1800 s equals 01:37:38, and the response `Date` 01:23:05 plus `expire` 873 equals 01:37:38. That agrees with the parser (`fetched_at + expire`). It matches the POTA rule that spots disappear about 30 minutes after the last spot (rules page). Because the response is cached (`age` header), the Date header is the better anchor than the local fetch time; the difference here was about 6 s.
- The all-spots endpoint `/spot/` has no `latitude`, `longitude` or grid fields (11 keys only); `spots.rs` uses `/spot/activator`, which is right.

**Park detail: `https://api.pota.app/park/US-0001`** (200, a JSON object, not an array; the parser accepts both). Keys include `parkId`, `reference`, `name`, `latitude`, `longitude` (numbers), `grid4`, `grid6`, `locationDesc` (`"US-ME"`), `locationName`, `active` (1 or 0), `parkComments`, `website`. Matches the parser, with these edge cases:

- `GET /park/K-0001` (a legacy-style reference) returns HTTP 200 with the body `null`; `parse_park_detail` returns "expected a JSON object", which is acceptable but is really "not found".
- `active` is not read. `US-4503` returns `active: 0` and a comment saying it can no longer be activated. Inactive parks are not in the location list (`US-4503` is absent from `/location/parks/US-MT`), so only a direct detail lookup shows it.
- `locationDesc` can be a comma list: for example `US-ID,US-MT,US-OR,US-WA`, and `US-4572` (Lewis and Clark National Historic Trail) lists 16 states. `states_of` in `export.rs` handles a comma list.

**Park list by location: `https://api.pota.app/location/parks/US-IN`** (200, array of 229). Keys: `reference`, `name`, `latitude`, `longitude` (numbers, none null), `grid` (always 6 characters, mixed case), `locationDesc`, `attempts`, `activations`, `qsos`, `website`, `notes` (string or null). The parser's `grid6` then `grid` then `grid4` fallback finds `grid`. The list for a state includes multi-state parks (the US-IN list contains `US-4572`), so one park appears in several state lists with the same multi-state `locationDesc`. `https://api.pota.app/location/US-IN` is not an endpoint (`Missing Authentication Token`).

**Documentation status**: I found no API page on docs.pota.app (index, rules, ADIF reference, submitting logs, resources). The endpoints are the ones the pota.app front end uses, so shapes are observed, not promised (**UNVERIFIED** as a stable contract).

**SOTA: `https://www.sotadata.org.uk/summitslist.csv`**. This URL answers HTTP 302 to `https://storage.sota.org.uk/summitslist.csv` (a different host; the client must follow cross-host redirects). The body is 5,075,513 bytes, UTF-8 without a BOM, LF line ends, 182,284 lines.

- Line 1: `SOTA Summits List (Date=04/11/2026)`.
- Line 2: `SummitCode,AssociationName,RegionName,SummitName,AltM,AltFt,GridRef1,GridRef2,Longitude,Latitude,Points,BonusPoints,ValidFrom,ValidTo,ActivationCount,ActivationDate,ActivationCall`. **The file now has 17 columns; the parser's `SUMMIT_HEADER` is the first 14 and uses `starts_with`, so it still passes.** Columns 15 to 17 are extra.
- First data rows: `3B8/MU-001,Mauritius,Mauritius,"Piton de la Petite Rivière Noire (Black River Peak)",828,2716,57.40771,-20.40882,57.40771,-20.40882,10,0,01/08/2026,31/12/2099,0,,` and a US row: `W9/IL-001,"USA - W9",Illinois,"Williams Hill",324,1064,-88.4756,37.5823,-88.47560,37.58230,1,0,01/07/2013,31/12/2099,40,23/01/2026,K8ARE`.
- Checked across all 182,282 data rows: every row has exactly 17 fields under a quote-aware split; `AltM` is an integer in every row; `Points` is one of 1, 2, 4, 6, 8, 10; `ValidFrom` and `ValidTo` are always `dd/mm/yyyy` (parser `date_key` expects that); `ValidTo` is `31/12/2099` for current summits and a real past date for retired ones (for example `31/03/2023` for `CT/BA-002`).
- Longitude is column 9 and Latitude column 10 (indexes 8 and 9), as the parser assumes (`3B8/MU-001`: lon 57.4, lat -20.4 is correct for Mauritius). `GridRef1` and `GridRef2` repeat the same pair at lower precision. Names with commas or non-ASCII are double-quoted.
- The SOTA site itself links this URL for "a CSV file containing a list of all summits" (in the same bundle).
- No mismatches with `parse_summit_list`. Note `points` is the base points; `BonusPoints` is separate and not read.

## 4. What the uploaders require, and `export.rs`

### POTA

Sources: <https://docs.pota.app/docs/activator_reference/ADIF_for_POTA_reference.html> (page last modified Jul 8 2026), <https://docs.pota.app/docs/activator_reference/submitting_logs.html> (Jul 8 2026), <https://docs.pota.app/docs/rules.html> (v1.3, page last modified Oct 4 2026, "Logging Requirements"). All retrieved 2026-11-04. The `ADIF_for_POTA.html` URL the earlier note used returns 403; the `_reference.html` page above is the current one.

- Required: `STATION_CALLSIGN` or `OPERATOR` (club logs must have both, with the club call in `STATION_CALLSIGN`; only A-Z, 0-9 and `/`), `CALL`, `QSO_DATE` (UTC, `YYYYMMDD`, future dates rejected), `TIME_ON` (UTC, `HHMM` or `HHMMSS`; QSOs are tallied by start time), `BAND` (with the unit, `20M`; for satellite, the activator's transmit band), `MODE` or `SUBMODE` (only one mode is stored; if both are given the submode wins).
- Optional: `MY_SIG`=`POTA`, `MY_SIG_INFO` (the park, `US-0005`), `SIG`=`POTA` and `SIG_INFO` (the other park, for park-to-park), `MY_STATE` (when the park crosses administrative lines, the uploader tries this code to place the activator; if missing or invalid it prompts). If `MY_SIG`/`MY_SIG_INFO` are missing the uploader prompts.
- Submission: upload at My Log Uploads on pota.app (since July 2022), or email `help@parksontheair.com`. **File naming applies only to the email route**: `callsign@parkNumber-activationDdate.adi`, for example `KA8H@US-1515-20201127.adi`; if the park spans several states, include the activation state: `W8MSC@US-4239-20181231-US-MI.adi`. Same-park, same-callsign logs are best combined; one file may span several UTC days.
- One log is for one park in a single location. n-fer (two-fer and up): "A separate log must be submitted for each park of the multi-park simultaneous activation" (rules, Activation Location and Access).
- Multi-state parks: the activator and all equipment must be inside the park and, by the rules' own reading, a person straddling a state line is out of compliance, so the activator is in one state. The state is chosen at upload for a multi-state park.
- Duplicates: rejected by the uploader. A QSO's identity is the uploading account, `CALL`, `MODE`, `QSO_DATE`, `BAND`, `MY_SIG_INFO`, `SIG_INFO`, and the location chosen at upload. `CALL` cannot equal `STATION_CALLSIGN` or `OPERATOR`.
- An activation is 10 QSOs in one UTC day; several visits to the same park, state and UTC day count once.

Against `export.rs`:

| Item | `export.rs` | Verdict |
|---|---|---|
| Fields | `STATION_CALLSIGN`, `OPERATOR`, `CALL`, `QSO_DATE`, `TIME_ON`, `BAND`, `FREQ`, `MODE`, `SUBMODE`, `RST_*`, `MY_SIG`, `MY_SIG_INFO`, `MY_STATE`, `SIG`, `SIG_INFO` | Matches the documented set; extras (`FREQ`, `RST_*`) are ordinary ADIF and not rejected by anything I read |
| One file per park for an n-fer | yes | Matches the rule |
| File name | `{call}@{park}-{YYYYMMDD}.adi` | Matches the email-route format; `/` in a call becomes `_` (documentation silent, **UNVERIFIED**) |
| Multi-state name suffix | `-IN`, state letters only | **Differs**: the documented form is the full location, `-US-MI` |
| Multi-state handling | one file per state, **every Contact written into every state's file** | **Likely wrong**: duplicates include the location in their identity, so the same QSO filed under two states would count twice, but the activator was in only one state. Better: one file, `MY_STATE` set to where the operator was (ask the user for multi-state parks) |
| Duplicate key | call, band, mode | The upload key also includes date, park and `SIG_INFO`; and mode should use `SUBMODE` when present (the submode wins) |
| Activation threshold | 10 | Matches the rules |
| Combining several UTC days | optional | Allowed in one file |

### SOTA CSV V2

Source: SOTA's "SOTA Activator and Chaser TSV/CSV file format" page, in the bundle served by <https://www.sotadata.org.uk/en/upload> (`main.140dc1ea7ec661f3.js`, route `/upload/csv/info`), and the same bundle's `loadCSV` and `fixBandCSV` code, retrieved 2026-11-04. Forum posts (reflector.sota.org.uk) agree but are not primary and were not relied on.

- Layout: `V2,<my callsign>,<my summit>,<date>,<time>,<band>,<mode>,<his callsign>[,<his summit>[,<notes>]]`, up to ten fields (the uploader rejects more than ten; fewer than seven is "insufficient fields"). Comma (CSV) or tab (TSV); the page says one record per line, terminated `[CR][LF]`. A header line that starts with `CALL` or `SIGNATURE` is skipped; lines with a single field are skipped.
- Callsigns: no spaces. My Summit is the full reference (`G/LD-008`), blank for a chaser entry. Summit references must match `^[A-Z0-9]+/[A-Z][A-Z]-[0-9][0-9][0-9]$`.
- Date: "Most date formats are accepted ... International date format (DD/MM/YY) is recommended." The uploader accepts `dd/mm/yy`, `dd/mm/yyyy`, `dd.mm.yy`, `yyyymmdd` and `yyyy-mm-dd`. Time (UTC): `HHMM` or `HH:MM`, 24 hour clock. Records must be in date and time order or the file errors ("Dates not in sequential order").
- Band: "You can enter the frequency in MHz (14.285MHz) or pick from the following": `1.8MHz`, `3.5MHz`, `5MHz`, `7MHz`, `10MHz`, `14MHz`, `18MHz`, `21MHz`, `24MHz`, `28MHz`, `50MHz`, `144MHz`, `432MHz`, `1240MHz` (the page labels them 160m through 23cm). Modes: valid values are `CW, SSB, FM, Data, DV, AM, Other` (the code also maps a long list of digital names such as FT8 to `DATA`).
- Notes: optional tenth field; a comma or tab inside it requires double quotes around the whole field.
- Activations: "Each activation is deemed to end when the [My Summit] reference changes." Several days at the same summit "must upload each activation in a separate file" (otherwise the file is assumed to hold one activation). Activator, chaser and S2S records can be mixed (S2S = both summit fields filled).
- File naming: no requirement; the file is chosen in a browser upload form (human step, login).

Against `export.rs`:

| Item | `export.rs` | Verdict |
|---|---|---|
| Field order and count | `V2,station,summit,date,time,band,mode,call,hisSummit,"notes"` (10 fields) | Matches |
| Date | `DD/MM/YY`, zero padded | Accepted (recommended form). Resolves the `UNVERIFIED` item for the date |
| Time | `HHMM` | Accepted |
| Band | the record's `BAND` lowercased, for example `40m` | **Not in SOTA's documented list.** The uploader turns `40m` into `40M` and passes it on; whether the server accepts that is **UNVERIFIED**. The documented safe form is `7MHz` (or the actual frequency, `7.032MHz`) |
| Mode | `CW`, `SSB`, `FM`, `AM`, `Data`, `Other` | Valid; `DV` exists but is not produced |
| Notes quoted | always `"..."` | Fine; an empty quoted field parses |
| One summit per file; multi-day | one file; `include_other_dates` can merge several days | **Conflicts** with "same summit on several days: separate files" |
| Line terminator | `\n` | Spec says CR LF; the uploader's CSV parser autodetects, so probably fine (**UNVERIFIED**) |
| Spaces in callsigns | not checked | The uploader rejects spaces |

## Summary table

| Item | Finding | Confidence | Source |
|---|---|---|---|
| ARRL HF edges | Page gives only sub-segments for 160-12 m; CW edges and full map are not stated; all 17 current rows match | High | arrl.org/band-plan, 2026-11-04 |
| Legal CW/data vs phone edges | Listed above from 97.305(c)(3) | High | eCFR 97.305, 2026-11-03 |
| Novice on 6, 2, 70, 33 cm | Wrong in toml; (a) excludes Novice | High | eCFR 97.301(a), (e) |
| Novice on 23 cm | Only 1270-1295; toml gives 1240-1300 | High | eCFR 97.301(e) |
| HF privilege rows | All correct | High | eCFR 97.301(b)-(e) |
| 60 m | Segment plus four channels, carriers right; note about ARRL disagreeing is stale; 2.8 kHz; CW on centre | High | eCFR 97.303(h)(3), 97.305(f)(14) |
| 60 m ERP limits | Not located in 97.301/303/305 | Low (UNVERIFIED) | arrl.org/band-plan only |
| Spots `parkName` | Always null; name is in `name`; parser blank | High | api.pota.app/spot/activator |
| Spots `expire`, `spotTime` | Seconds remaining; UTC without zone; parser agrees | High | same, plus POTA rules (30 min) |
| Park detail and list | Shapes match; `/park/K-0001` is `200 null`; `active` unread; multi-state `locationDesc` | High | api.pota.app |
| POTA API stability | No official docs found | Medium | docs.pota.app |
| Summit list | 17 columns, parser's 14-column prefix still matches; 302 to another host; data clean | High | sotadata.org.uk/summitslist.csv |
| POTA ADIF | Export fields right; multi-state suffix and per-state duplication look wrong; n-fer files right | Medium to high | docs.pota.app (ADIF reference, submitting logs, rules) |
| SOTA CSV V2 | Layout and date right; band `40m` not in documented list; multi-day merge conflicts | High (layout), Medium (band acceptance) | sotadata.org.uk uploader bundle |
