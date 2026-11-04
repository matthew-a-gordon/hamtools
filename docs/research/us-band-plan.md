# US band plan and license privileges as embeddable data

Research for #40 (map #34). Researched 2026-11-04. Planning only; no source changes.

> **VERIFY BEFORE RELYING ON THIS.** Everything below is a research note, not a
> compliance tool. Privilege data (who may transmit where, with which emissions
> and power) must be verified against **the operator's actual license class** and
> the **current FCC rules** (47 CFR Part 97, eCFR) before anyone relies on it.
> Part 97 was amended as recently as 91 FR 1430 (Jan. 14, 2026), and the
> extraction used here was lossy (see "Confidence and discrepancies"). The app must
> never present itself as authoritative on what is legal to transmit.

## Sources, by trust

| Data | Owner | Source | Used for |
|---|---|---|---|
| Frequency bands per license class | FCC | 47 CFR 97.301 (eCFR) | privileges |
| Sharing conditions, 60 m channels | FCC | 97.303 (esp. (h)(3)) | 60 m, caveats |
| Emission types by segment | FCC | 97.305 | mode-by-segment privileges |
| Power limits | FCC | 97.313 | optional limit hints |
| Space station bands | FCC | 97.207(c) | satellite |
| Voluntary mode segments, calling freqs, satellite sub-bands | ARRL | https://www.arrl.org/band-plan | band plan |

Primary: eCFR, https://www.ecfr.gov/current/title-47/part-97 (sections above).
The ARRL plan is voluntary and says locally coordinated plans take precedence.

## Licensing and attribution

- Part 97 is a US federal regulation: public domain, free to embed with a citation
  and an "as of" date.
- The ARRL page states "Reproduction of material from any ARRL web page without
  written permission is strictly prohibited" (copyright 2026 ARRL). Facts
  (frequency edges) are not copyrightable per se, but the safe path is: do not copy
  ARRL wording or table layout; store only terse factual segment boundaries,
  cross-checked against Part 97 and the published plan, cite ARRL as the
  reference, and ask ARRL permission before shipping their text or a near copy.
  This is a judgement, not legal advice.

## Privileges by license class (97.301, Region 2 values)

The rule text still lists Novice and Advanced (no longer issued; holders keep
privileges).

- **All classes (97.301(a))**: 6 m 50-54 MHz; 2 m 144-148; 1.25 m 219-220 (fixed
  digital forwarding only) and 222-225; 70 cm 420-450; 33 cm 902-928; 23 cm
  1240-1300; 13 cm 2300-2310 and 2390-2450; 5 cm 5.650-5.925 GHz; 3 cm 10.0-10.5
  GHz; 1.2 cm 24.00-24.25 GHz; mm bands above.
- **Extra (b)**: the above plus 2200 m 135.7-137.8 kHz; 630 m 472-479 kHz; 160 m
  1800-2000 kHz; 80 m 3.500-3.600; 75 m 3.600-4.000; 60 m 5.3515-5.3665 (see
  60 m); 40 m 7.000-7.300; 30 m 10.100-10.150; 20 m 14.000-14.350; 17 m
  18.068-18.168; 15 m 21.000-21.450; 12 m 24.890-24.990; 10 m 28.000-29.700.
- **Advanced (c)**: 80 m 3.525-3.600; 75 m 3.700-4.000; 40 m 7.025-7.300; 20 m
  14.025-14.150 and 14.175-14.350; 15 m 21.025-21.200 and 21.225-21.450; 160, 60,
  30, 17, 12, 10 m as Extra.
- **General (d)**: 160 m full; 80 m 3.525-3.600; 75 m 3.800-4.000 (expected for
  R2; the extraction showed this row ambiguously, verify); 60 m as Extra; 40 m
  7.025-7.125 and 7.175-7.300; 30, 17, 12, 10 m as Extra; 20 m 14.025-14.150 and
  14.225-14.350; 15 m 21.025-21.200 and 21.275-21.450.
- **Novice/Technician (e)**: 80 m 3.525-3.600; 40 m 7.025-7.125; 15 m
  21.025-21.200; 10 m 28.0-28.5; Novice-only 1.25 m 222-225 and 23 cm 1270-1295.

97.301 gives band edges, not modes. Mode limits for Technician on HF come from
97.305 plus class and were not retrieved here (see discrepancies). Do not infer
mode privileges from band edges alone.

## Emission types by segment (97.305), summary

CW is allowed anywhere the operator has privileges. HF phone/data split: 80 m
RTTY/data only (phone is the 75 m part); 40 m 7.000-7.100 data, 7.075-7.100
voice (Region 1/3 note), 7.100-7.125 data, 7.125-7.300 voice; 30 m data only, no
phone; 20 m 14.00-14.15 data, 14.15-14.35 voice; 17 m 18.068-18.110 data, above
voice; 15 m 21.0-21.2 data, 21.2-21.45 voice; 12 m 24.89-24.93 data, above voice;
10 m 28.0-28.3 data, 28.3-29.7 voice. 6 m from 50.1 all modes; 2 m from 144.1
all modes. 70 cm and above broadly all emission types (SS, pulse on some).
1.25 m 219-220 data only. Caveat: this table looked older than the Jan 2026
amendment; re-read the live section before encoding.

## 60 m (5 MHz)

97.303(h)(3) as read: amateurs may transmit only in 5351.5-5366.5 kHz and on four
center-frequency channels.

| Center (kHz) | Carrier for phone/data/RTTY (kHz) |
|---|---|
| 5332.0 | 5330.5 |
| 5348.0 | 5346.5 |
| 5373.0 | 5371.5 |
| 5405.0 | 5403.5 |

CW carrier sits on the center frequency. All 60 m emissions within 2.8 kHz.
Designators: phone 2K80J3E, data 2K80J2D, RTTY 60H0J2B, CW 150HA1A. Secondary
service; must not interfere with fixed-service stations and must accept
interference. Embed 60 m as a channel list plus one contiguous segment, not a
normal band edge.

## Satellite sub-bands

- FCC 97.207(c), space station transmit segments: 7.0-7.1, 14.00-14.25, 144-146,
  435-438 MHz, 2400-2450 MHz, 5.83-5.85 GHz, 10.45-10.50 GHz, 24.00-24.05 GHz,
  plus the 17/15/12/10 m bands. Any class may be control operator of a space
  station, within its own privileges.
- ARRL plan satellite hints: 2 m 144.30-144.50 and 145.80-146.00; 10 m
  29.300-29.510; 70 cm 435-438 (satellite only internationally); 3400-3410 MHz;
  5 cm 5650-5670 up, 5830-5850 down; 13 cm 2400-2410.
- Ground-station privileges still follow the operator's class on the uplink
  frequency; satellite sub-band membership says nothing about that.

## ARRL voluntary segments (selected, display hints only)

160 m digital 1.800-1.810; 80 m RTTY/data 3.570-3.600; 40 m RTTY/data
7.080-7.125; 30 m RTTY 10.130-10.140, packet 10.140-10.150; 20 m RTTY
14.070-14.095, packet 14.1005-14.112; 10 m CW 28.000-28.070 and 28.150-28.190,
phone 28.300-29.300, FM repeaters 29.520-29.700; 6 m CW/beacons 50.0-50.1, SSB
calling 50.125; 2 m SSB calling 144.200, FM calling 146.52; 70 cm calling
432.10, national simplex 446.00. Conventions, not rules; regional coordination
overrides.

## Suggested data shape (TOML)

```toml
[meta]
as_of = "2026-11-04"
fcc_amended = "91 FR 1430"
region = 2

[[segment]]              # privilege layer: FCC, class-gated
band = "20m"
lo_hz = 14_000_000
hi_hz = 14_150_000
emissions = ["cw", "data"]                     # from 97.305
classes = ["extra", "advanced", "general"]     # from 97.301
class_lo_hz = { general = 14_025_000 }         # per-class edge overrides

[[channel]]              # 60 m only
center_hz = 5_332_000
carrier_hz = 5_330_500
classes = ["extra", "advanced", "general"]

[[plan]]                 # ARRL, voluntary, display only
lo_hz = 14_070_000
hi_hz = 14_095_000
label = "RTTY"
source = "arrl"

[[satellite]]
lo_hz = 145_800_000
hi_hz = 146_000_000
label = "OSCAR"
source = "arrl"
```

Keep three layers separate: privilege (FCC, class-gated), plan (ARRL, advisory),
satellite. Integer Hz. Each row carries `source`; the file carries `as_of`. Data
file, not code, so corrections are data-only.

## Confidence and discrepancies

The fetch tool summarised pages with a small model; raw eCFR XML could not be
fetched (curl timed out; direct eCFR pages redirected to a bot gate). Treat all
numbers as unverified transcription.

- 97.301 printed 60 m as 5.3515-5.3665 MHz while 97.303(h)(3) read as
  5351.5-5366.5 kHz plus four channels (same range, different unit). The ARRL page
  still says "five channels plus one 15 kHz segment" with 100 W PEP, which
  conflicts with four channels.
- 97.313(i) read as 100 W ERP on channels and 9.15 W ERP in the band, while
  97.303(h)(3) mentioned no power limit; likely stale or mis-summarised. **Do not
  encode power limits** until read from the live text.
- 97.301 footnotes not retrieved; 75 m General R2 row ambiguous.
- 97.305 table looked pre-2026.
- Technician/Novice HF mode limits were not retrieved: verify.

## Recommendation for rig-tui

- Embed the table as a data file with `as_of`; show a persistent "advisory:
  verify with current FCC rules and your license" note in the bandplan modal.
- License class is runtime config (like observer position), never inferred.
- "Outside privileges" is a soft warning, never a transmit gate, and the UI
  never claims "legal". Nothing in this effort transmits.
- Before building: re-fetch 97.301, 97.303, 97.305, 97.313 from eCFR with a plain
  HTTP client, diff against this note, and ask ARRL whether their segment data may
  be embedded.
