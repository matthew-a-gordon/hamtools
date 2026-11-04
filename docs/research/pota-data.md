# POTA data sources and rules (research for #39, map #34)

Researched 2026-11-04. Read-only: public docs and a public client's source; no request was sent to api.pota.app, no authentication, nothing posted. Posting and self-spotting are out of scope.

## Bottom line

- POTA publishes **no official API documentation**. The JSON endpoints at `api.pota.app` are public, unauthenticated and used by the pota.app front end and by third-party tools. They are undocumented, so treat shapes as observed, not promised. Parse defensively and tolerate change.
- **No stated rate limit or terms of use** for the API (the POTA Privacy Policy is silent on API, scraping and reuse). Be a polite client: cache, poll spots at most about once a minute, honour any 429/Retry-After.
- The **ADIF rules are officially documented** and small. Required fields are few; the park goes in `MY_SIG`/`MY_SIG_INFO`.
- **Offline cache**: park metadata (24 h or longer) is cacheable; spots are not worth caching beyond about a minute. A bulk park file location is **unverified** (see Open questions).

## 1. APIs

Primary-source status: docs.pota.app (official) lists no API page; [privacy.html](https://docs.pota.app/docs/privacy.html) says nothing about API use and points to help@parksontheair.com for questions. `api.pota.app` and `docs.pota.app/docs/activator_reference/ADIF_for_POTA.html` returned HTTP 403 to my fetcher. The endpoint list below therefore comes from the source of a public third-party client, [qso-graph/pota-mcp `client.py`](https://github.com/qso-graph/pota-mcp/blob/main/src/pota_mcp/client.py) (secondary, but it states its field sets were checked against the live endpoints on 2026-10-10), plus a [Rig-Control-Web wiki page](https://github.com/jbdubbs/Rig-Control-Web/wiki/POTA-and-SOTA-Spots) and a [Log4OM forum thread](https://forum.log4om.com/viewtopic.php?p=41514) that use `https://api.pota.app/spot/` and question whether general use is approved.

Base `https://api.pota.app`, all GET, JSON, no auth:

| Endpoint | Returns |
|---|---|
| `/spot/activator` | Current activator spots (array). `/spot/` is also seen in other clients. |
| `/park/{ref}` | Park detail: `reference, name, latitude, longitude, grid4, grid6, parktypeDesc, locationDesc, locationName, entityName, agencies, website, accessMethods, activationMethods, firstActivator, firstActivationDate, active` |
| `/park/stats/{ref}` | `attempts, activations, contacts` |
| `/park/activations/{ref}?count=N` | Recent activations (`activeCallsign, qso_date, totalQSOs, qsosCW/DATA/PHONE, locationDesc`) |
| `/park/leaderboard/{ref}` | Top activators and hunters |
| `/location/parks/{loc}` | All parks in a location such as `US-IN`: `reference, name, latitude, longitude, grid, locationDesc, activations, attempts, qsos, notes, website` |
| `/activation` | Scheduled activations: `scheduledActivitiesId, activator, reference, name, locationDesc, startDate, endDate, startTime "HH:MM" (UTC), endTime, frequencies, comments` |
| `/stats/user/{call}` | Per-callsign stats (personal data; not needed) |

Spot object fields (observed): `spotId, activator, frequency` (string, kHz, e.g. `"14074.0"), `mode, reference, name, locationDesc, grid4, grid6, latitude, longitude, spotTime` (ISO, no zone, assumed UTC), `spotter, source, comments, count, expire` (seconds), `parkName, invalid` (often null).

Implications for rig-tui: a spot carries frequency and mode, so it can drive a tune action through rigctld. It carries lat/long, so distance from the Observer is computable offline. `/location/parks/US-IN` returns a whole-state park list in one call, which suits an offline park cache and lookup by name or proximity.

### Auth, rate limits, terms

- Auth: none for reads. Do not add credentials.
- Rate limit: none stated by POTA. pota-mcp's own policy (a judgement, not POTA's): spots TTL 60 s, park info 24 h, stats 24 h, scheduled 5 min, location list 24 h, 100 ms minimum gap, back off 60 s to 1 h on 429 with Retry-After. Rig-Control-Web suggests 3 to 5 minute polling for casual use. Adopt similar floors; spots no faster than 60 s.
- Terms: none found. The Privacy Policy covers only personal data collected through the site. For a definitive answer, email help@parksontheair.com (not done).
- Spots expire: [Rules](https://docs.pota.app/docs/rules.html) say a spot disappears about 30 minutes after the last spot or when the comment marks QRT. Anyone may spot an activator (we will not).

## 2. Park reference format

[Glossary](https://docs.pota.app/docs/glossary.html): program prefix, dash, number, e.g. `US-0001`, `CA-5082`. [ADIF reference](https://docs.pota.app/docs/activator_reference/ADIF_for_POTA_reference.html): "the POTA program prefix (K, VE, etc.) followed by a hyphen (-) and then a number". Practical regex from pota-mcp: `^[A-Z0-9]{1,8}-[0-9]{1,6}$` (prefixes can contain digits, e.g. `9A`, `3B8`; do not enumerate prefixes). A park can span regions; `locationDesc` may be comma-separated.

## 3. Activation rules relevant to the tool ([Rules](https://docs.pota.app/docs/rules.html))

- Activated = at least 10 QSOs from the park within one UTC day. Counted per UTC day; several activities at the same park on the same UTC day count as one if 10+ combined.
- Logs must be ADIF; "There are no time limits for log submission". "One log can cover multiple activity days but should be for only one park in a single location."
- Duplicate QSOs rejected; a station cannot work itself. All bands and modes allowed; no land repeaters; satellite repeater contacts allowed, with BAND being the activator's transmit band. Relayed contacts prohibited. No POTA power limit beyond law and minimum necessary.
- Activators need a registered pota.app account to upload.
- A useful live feature: QSO counter toward 10, per UTC day, per park.

## 4. ADIF requirements ([ADIF for POTA reference](https://docs.pota.app/docs/activator_reference/ADIF_for_POTA_reference.html))

Required (minimum to tally): `STATION_CALLSIGN` or `OPERATOR` (club activations must include `OPERATOR`), `CALL`, `QSO_DATE` (UTC YYYYMMDD, future dates rejected), `TIME_ON` (UTC HHMM or HHMMSS; QSOs are tallied by start time), `BAND` (e.g. `20M`; satellite: uplink band), `MODE` or `SUBMODE` (single mode stored; submode wins if both present).

Optional but needed for park processing: `MY_SIG=POTA`, `MY_SIG_INFO=<activated park>`, `SIG=POTA` and `SIG_INFO=<other park>` for park-to-park, `MY_STATE` (uploader prompts if missing or unrecognized).

Example from the doc:
```
<STATION_CALLSIGN:5>W8MSC <CALL:4>N0AW <QSO_DATE:8>20201002 <TIME_ON:6>134500 <BAND:3>40M <MODE:3>SSB <MY_SIG:4>POTA <MY_SIG_INFO:7>US-3315 <SIG:4>POTA <SIG_INFO:7>US-0008 <EOR>
```

Multi-park (n-fer): per the [P2P guide](https://docs.pota.app/docs/activator_reference/park_2_park.html), write the same QSO once per park, each line with a different park reference ("each line needs a unique park reference in SIG_INFO"). The ADIF reference page has no n-fer section. A user-forum report (secondary, from a search snippet) says the uploader rejects multiple references inside one `MY_SIG_INFO`; line-per-park is the safe choice. Duplicates need not be removed before upload.

P2P: matched when QSO times in both logs are within 15 minutes and required fields match; callsigns must match exactly (log what was sent on air, e.g. a `/M` suffix). `SIG_INFO` is strongly encouraged, not required. One P2P per ADIF line.

Unconfirmed: a Ham Radio Deluxe page mentions newer ADIF-standard `POTA_REF`/`MY_POTA_REF` fields; the official POTA docs still describe `SIG`/`SIG_INFO`. Use `SIG`/`MY_SIG`; verify against the ADIF spec before adding the others.

### File conventions ([Submitting Logs](https://docs.pota.app/docs/activator_reference/submitting_logs.html))

- Recommended name: `callsign@parkNumber-activationDdate.adi`, e.g. `KA8H@US-1515-20201127.adi`; if the park spans states, append the state: `W8MSC@US-4239-20181231-US-MI.adi`.
- One file per activation is recommended, **not required**; "A single ADIF file may contain multiple UTC days." Per Rules, one log should cover only one park (location). So: **one file per park per activation; per-UTC-day splitting is optional**. Whether an n-fer's lines for several parks may share one file is not stated.
- Upload: self-service at pota.app "My Log Uploads" (login required; the user does this manually), or email help@parksontheair.com. Fixing errors: "Request Support" on the bad log, then re-upload; volunteers do not edit QSOs.
- Tool implication: export is a plain file write; no network or auth.

## 5. Exchange conventions ([CW Guide](https://docs.pota.app/docs/cw_guide.html))

- A valid contact needs only callsigns exchanged (K0GUZ, quoted in the guide). Typical: activator calls "CQ POTA K7XYZ"; hunter sends callsign once; activator replies with the hunter's call, RST and state or park code; hunter returns RST and state. Guide example: `UR 599 5NN P2P K6876 K6876 MA MA BK`.
- P2P: hunter sends "P2P" in place of a callsign, then the park reference.
- Contact form prefill candidates: RST (599 on CW, 59 on phone), hunter state, and for P2P their park reference (maps to `SIG_INFO`). The activator's park is constant per session (`MY_SIG_INFO`), so it belongs on the Activation plan, not the Contact form.
- Spotting: RBN skimmers auto-update the spot page for registered CW activations when CQs are clean and keyer-sent; a message like "CQ CQ CQ POTA W0ZAP W0ZAP W0ZAP" is conventional (candidate CW Soft key: `CQ POTA <call>`). Hunters transmit 20 to 100 Hz off the activator.

## 6. What can be cached offline

| Data | Staleness tolerance | Source |
|---|---|---|
| Park list for a location (ref, name, lat/lon, grid) | days to weeks (pota-mcp notes a state list changes roughly weekly) | `/location/parks/{loc}` |
| Park detail | 24 h or longer | `/park/{ref}` |
| Scheduled activations | about 5 min | `/activation` |
| Spots | about 1 min; useless stale; each spot expires | `/spot/activator` |

Offline-first fit: a periodically refreshed local park file for the operator's state(s) powers lookup with no network; spots and schedule are best-effort overlays and must never block rig, logging or CW.

## Open questions / not verified

1. Whether pota.app offers an official bulk park CSV. Search found nothing confirming a file name or URL; I did not guess one. Use `/location/parks/{loc}` per state, or ask help@parksontheair.com.
2. Approval or terms for third-party API use (nothing published). Consider emailing the helpdesk before shipping.
3. The activator guide, the non-reference ADIF page and api.pota.app returned 403 to the fetcher, so endpoint shapes come from third-party code, not POTA.
4. Spot `spotTime` timezone (assumed UTC), 429 behaviour, shape stability.
5. Whether n-fer lines for different parks may share a file. Untested against the uploader.
6. Nothing here touches hardware; "untested on hardware" rules apply to the rig side.
