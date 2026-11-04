# Research #38: SOTA data sources and rules

Part of map #34. Read-only research; nothing was authenticated, posted, or called against a live endpoint. Only documentation pages and public forum threads were read. Several official sites (sota.org.uk, sotadata.org.uk) are JavaScript apps that returned no body text to the fetcher, so some primary pages could not be read directly; those gaps are marked **UNVERIFIED**.

## Headline finding (blocks design)

The SOTA API terms (https://api2.sota.org.uk/docs) say: "no AI-generated software may connect to the SOTA API without prior approval." Other terms on the same page:

- Provided as-is, no SLA or warranty; subject to change without notice (endpoints and schemas may be modified, deprecated or deleted).
- Subject to "reasonable usage limits"; users causing undue load or using deprecated endpoints may be blocked. No numeric rate limit is published.
- "The API cannot be used in any commercial product" without a licence and fee.
- Developers (including library authors) must join the SOTA Reflector and its "API-consumers" group before using the API (https://reflector.sota.org.uk/, contact: https://www.sota.org.uk/Contact).
- Check for an existing app first and consider contributing to it.

Enforcement is real: a third-party project was told by SOTA staff to stop misusing the API and dropped SOTA entirely because it was AI-assisted (https://github.com/mooxle/Muf_Muncher/issues/2; the "AI" reason is the project owner's own characterisation, not a SOTA quote).

Implication for rig-tui: this repo is developed with AI agents. Any live SOTA API client needs the owner to join API-consumers and obtain explicit approval first. Until then, design the SOTA Activity so the online feed is an optional, pluggable source, and everything else (summit lookup, log export) works offline from the downloadable summit list and a local file.

## API: what exists

Source of truth is https://api2.sota.org.uk/docs/index.html. The fetched page showed only the terms; the endpoint listing was not readable (**UNVERIFIED**; a guessed swagger.json URL returned 404). Endpoints below come from the SOTA Reflector thread https://reflector.sota.org.uk/t/sotadata-api/23731 (2020, may be stale) and search snippets of community code:

| Purpose | Path (on https://api2.sota.org.uk) | Notes |
|---|---|---|
| Recent spots | `GET /api/spots/{limit}[/{filter}]` | Positive limit = number of spots (max 200); negative = hours (max 72), e.g. `/api/spots/-1/all`. Community-reported. |
| Summit detail | `GET /api/summits/{association}/{code}` | Grid square, coordinates. Community-reported. |
| Associations | `GET /api/associations` | Suggested on the Reflector for listing. |
| Regions | `GET /api/regions[/{assoc}/{region}]` | Same. |
| Alerts | `GET /api/alerts` | Seen in an older crate only. |
| Post spot | `POST /api/spots` | Out of scope (no posting). Needs a login token. |

- Auth: reading reportedly needs none (a WordPress plugin doc says so); posting needs a SOTA login token. Reading without auth is still subject to the terms above.
- Rate limits: none numeric. Community guidance: one SOTA Slack spotter polls `/api/spots/-1/all` once per minute. Treat 60 s as the floor and back off on errors.
- A separate "database API" exists; SOTA staff (VK3ARR) say it "isn't considered stable" and its docs are not widely shared. Do not build on it.
- Stability: no deprecation notice period. Always check status codes; unchecked responses plus a deprecated endpoint is what got Muf_Muncher flagged.
- No push feed (websocket/MQTT) was found. **UNVERIFIED** whether one exists.

## Summit reference format

`{Association}/{Region}-{Number}`, e.g. `G/LD-008`, `VK2/NR-009`, `W0C/SP-085` (examples via https://sarc.org.au/sota/ and https://www.arrl.org/radio-operating-from-summits, search snippets).

- Association: a code that may be a country/state or, in the US, a call-area style prefix (W1, W2, W0C ...). Each association has an Association Reference Manual (ARM) listing regions and summits.
- Region: two characters, unique within the association.
- Number: 001-999, zero-padded to 3.
- Association part has variable length and can contain digits; parse on the last `/` then `-`. The association code for the owner's area was not verified; look it up in the ARM, and do not hard-code it.

## Offline cache

- Summit list: https://www.sotadata.org.uk/summitslist.csv, "updated daily" per the ShackKit README and a Log4OM forum post (via search). Whole-world table (ref, association, region, name, altitude, coordinates, points, bonus points, validity dates per a Parks'n'Peaks page's overlapping fields). Exact header row **UNVERIFIED** (file not fetched). Licence of the CSV **UNVERIFIED**; the API terms do not obviously govern it, so ask the Management Team before redistributing it. Cache locally, do not commit it.
- Spots are ephemeral (hours): cache only for a "last seen" display; never block on them.
- Association/region listings change rarely; cache alongside the summit list.

## Log upload: CSV V2

Primary spec is the sotadata.org.uk ActivatorCSVInfo page (not readable by the fetcher). Content below is from a project that quotes it (https://github.com/andrewquamme/SOTA-logger) and a user guide (https://vk1da.blog/2014/04/21/sota-logs-using-csv-upload/).

Up to ten fields, comma-separated (or tab), one QSO per line:

1. `V2` version tag
2. My callsign
3. My summit (full reference)
4. Date, `DD/MM/YY` recommended (`DD/MM/YYYY` also seen)
5. Time UTC, `HHMM` or `HH:MM`, 24 h
6. Band: frequency in MHz with unit (`7.0MHz`) or band name (`20m`)
7. Mode: CW, SSB, FM, Data, AM, Other
8. His callsign
9. His summit (optional; blank if none; this makes it S2S)
10. Notes (optional)

Rules:

- Callsigns have no spaces. A notes field containing a comma or tab must be double-quoted (older guides say just avoid commas).
- Records in chronological order, all records for one activation together; out-of-order is rejected.
- Keep leading zeros in times (`0005`); do not upload template sample rows; a final blank line is tolerated.
- Activator, chaser and S2S records may share a file; on activation import the chaser/S2S records are ignored.
- Example: `V2,G3WGV,G/LD-008,24/04/03,1204,7.0MHz,CW,G3NOH,,PSE QSL Direct`.
- Upload is by web form at sotadata.org.uk (login, a human step). Export is a file; no API upload is needed or wanted. The chaser CSV format was **not verified**.

Mapping to our logbook: own callsign, own summit ref (from the Activation plan), UTC date/time, frequency or band, mode (collapse to CW/SSB/FM/AM/Data/Other), their call, their summit ref (S2S), comment. ADIF `MY_SOTA_REF`/`SOTA_REF` are used by converters (https://on6zq.be/w/index.php/SOTA/ADIF2SOTA); SOTA's own ADIF-import behaviour is **UNVERIFIED**.

## Rules that affect the app

From Reflector rule threads, not the General Rules PDF itself (could not be fetched; verify against it):

- Activation scores with 4+ QSOs, each a different station, at least one made from the summit; bands/modes can be mixed (https://reflector.sota.org.uk/t/definition-of-a-valid-sota-qso/26912, https://reflector.sota.org.uk/t/do-you-need-4-qso-for-hf/25019).
- One activation per summit per UTC day; the database refuses duplicates. Activation points once per calendar year, winter bonus aside. Crossing UTC midnight is the activator's call; recorded date is the first QSO's (https://reflector.sota.org.uk/t/splitting-4-qso-across-midnight/17445).
- Chaser points once per UTC day per summit regardless of activator. S2S counts as activator plus chaser credit; no seasonal bonus on the activator side.

## Exchange conventions

Only callsigns and signal reports are required; sending the summit reference is strongly recommended, not required (quoted from the rules in a Reflector thread). No primary document on spot-driven chasing or band/frequency conventions was read. **UNVERIFIED.**

## Suggestions for the spec

1. Gate the live SOTA API on owner approval; spec the Activity to run fully offline (cached summit list, local log, CSV V2 export) with the spot feed as an optional module.
2. Contact-form prefill: own summit ref (Activation plan), their summit ref (S2S), mode, frequency, UTC. A "4 distinct stations" counter and a UTC-day-boundary warning are cheap and offline.
3. Export CSV V2 with strict ordering, quoted notes, UTC; a human uploads it.
4. No self-spotting or posting (already out of scope).

## Open items

- Ask SOTA (API-consumers group) whether approval is possible for AI-assisted software, and what the read limits are.
- Read the endpoint listing at api2.sota.org.uk docs in a real browser and record true paths, parameters and response shapes.
- Download summitslist.csv by hand once to confirm the header and any licence line.
- Confirm the chaser CSV format and the General Rules wording.
