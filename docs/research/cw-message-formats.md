# CW message formats and the prosign SK

Researched 2026-11-05 (local machine clock read 2026-11-04 evening; the header date follows the task). Convention: `docs/research/<topic>.md`, as for `missing-data.md`. No source files were changed.

> Not an operating authority. Anything not read from a primary source is marked **UNVERIFIED**. "Primary" here means the document's own publisher or author, fetched in this session; where a document is hosted by one organisation but written by another, both are named.

Method notes:

- Fetched with `curl -A "Mozilla/5.0"` or `-A hamtools-research`, PDFs converted with `pdftotext -layout` (poppler). The built-in WebFetch tool could not read PDFs and got 403 on several pages, so every quotation below was checked against text I extracted myself, not against a model summary. Two web-search summaries made claims I could not reproduce from the page; those are listed under "Not verified".
- **Pages that could not be fetched or had no usable text:** `https://www.arrl.org/morse-code` (404 page); `https://www.arrl.org/arrl-morse-code-qso-practice`, `/morse-code-abbreviations`, `/q-signals`, `/cw-operating-procedures`, `/prosigns`, `/cw-operating`, `/operating-specialties`, `/contest-operating-tips` (HTTP 200 but only site-wide navigation text; no page-specific content); `http://www.arrl.org/quick-reference-operating-aids` (200, empty body text); `http://itdevel.arrl.org/ham-radio-history` (connection failed); `https://www.arrl.org/files/file/Morse/Morse%20Code%20Abbreviations.pdf` (returned a GIF, not a PDF); the "ARRLWeb A Quick Refresher Course on CW Operating Procedures" mirror at `ftp.unpad.ac.id` (Cloudflare challenge, 403); `https://docs.pota.app/docs/activator_reference/activator_guide.html` (403, but the same guide at `activator_guide-english.html` was read); `https://www.sota.org.uk/*` (a JavaScript app that serves the same 4.5 kB shell for every path, so no SOTA page text was readable there).
- **Not reachable at all:** The ARRL Operating Manual, the ARRL Handbook CW chapters, ITU Radio Regulations, and ACP 131 (not found; see section 5). I did not read them. Nothing below is attributed to them.

## 1. What SK means, source by source

Short answer: as a **prosign** (letters run together, `...-.-`), SK is the "end of work" / "end of contact" signal. "Silent key" is a **separate expansion** of the same two letters used in text (QST's obituary column; the Ethics manual's glossary lists both). They are not the same use: the on-air prosign does not say anything about the operator's death.

| Source | Owner | What it says about SK |
|---|---|---|
| ITU-R M.1677-1 (10/2009) | ITU | Lists "End of work" as `...-.-` in 1.1.3, and in Part II rule 7: "The end of work shall be indicated by the station that transmitted the last telegram. The correct indication is the end of work signal (. . . - . -)." The Recommendation never writes the letters "SK"; it gives the dot-dash pattern only. That `...-.-` is the letters S and K run together is my reading, not ITU text (**UNVERIFIED** as ITU wording). |
| ARRL Silent Key Submission Guidelines | ARRL | "Silent Key" means a deceased radio amateur: "QST Magazine regularly features a listing of Silent Keys as a way to honor our departed Radio Amateurs." Reports go to `sk@arrl.org` with an obituary, death certificate or estate letter. The page says nothing about the prosign. |
| ARRL NTS Public Service Communications Manual, App. B, Ch. 5 "Net Control" (W3YVQ v5.04, 5/02), hosted at arrl.org | ARRL (NTS volunteers) | "The prosign <SK> evolved from the old Morse 30, (...-. = 3, long dash = 0; together in current Morse = <SK>), indicating 'Out; clear (end of communications, no reply expected.)', the end of the telegrapher's shift. Some operators prefer no ending prosign, others <AR> or CL." Its CW net closing is `[QNC] (net name) [QRU] QNF 73 DE (call sign) [<SK>]`. |
| Same manual, Ch. 3 "Sending Messages on CW" (W3YVQ v3.01, 5/02), glossary | ARRL (NTS volunteers) | "<SK> PROSIGN (...-.-) Out, end of transmissions, no reply expected, from the old telegraph (...-.):3, (---, long dash): 0 = 30, end of shift." |
| *Ethics and Operating Procedures for the Radio Amateur*, Edition 3 (June 2010), by John Devoldere ON4UN and Mark Demeuleneere ON4WW, hosted at `arrl.org/files/file/DXCC/Eth-operating-EN-ARRL-CORR-JAN-2011.pdf` | **Authors are IARU Region 1 / UBA operators, not ARRL**; ARRL hosts a corrected copy for DXCC | See below. Two glossary entries for SK, both given. |
| ACP 130(A), July 2005 (visual signalling), hosted by commsmuseum.co.uk | Allied (NATO) | Its authorised prosign list (para. 3-1) has AR "End of transmission", AS "Wait", K "Invitation to transmit"; **no SK and no VA entry**. This is the visual-medium publication, not ACP 131. |
| POTA CW Guide, Activator Guide, Hunter Guide | Parks on the Air | Do not use or define SK at all (searched all three pages). |

The Ethics manual (hosted by ARRL, written by ON4UN/ON4WW), quoted from my own text extraction:

- Section II.9.6: "A typical way to gracefully end the QSO would be: '...TKS (thanks) FER QSO 73 ES (=and) CUL (see you later) W1ZZZ de G3ZZZ SK'. 'SK' is the prosign meaning 'end of contact'."
- Same section: "'DIT DIT DIT DAH DIT DAH' is the prosign 'SK' (from 'stop keying') and not 'VA' as published in some places (SK sent without inter letter spacing sounds the same as VA sent without inter letter spacing)."
- "Do not send '...AR SK'. It does not make sense. You are saying 'end of transmission' + 'end of contact'... the AR is redundant, so avoid using it."
- "If at the end of the QSO you also intend to close down your station, you should send: '...W1ZZZ DE G3ZZZ SK CL'".
- Closing-code table: `SK` = "end of contact (end of QSO)", use "at end of QSO"; `AR SK` = "do NOT use"; `SK CL` = "end of QSO + closing down station", use "when closing down".
- II.9.13: after two stations finish, "If they both sign with 'CL'... the frequency is now clear... If one or both ended with 'SK'... it may well be so that one or the other will remain on frequency for more QSOs." So SK means the contact is over, not that the operator is leaving the air.
- Abbreviation list (about p. 34): "SK: end of contact (prosign)" and, on the next line, "SK: silent key, a deceased ham". Summary: "SK: end of QSO: is used to end a QSO (SK = Stop Keying)."

Two different origin stories appear in primary documents and they disagree: the ARRL NTS manual derives SK from the landline number 30 ("end of shift"); the Ethics manual calls it "stop keying". I did not find a primary source that settles the etymology. Both agree on the meaning (end of the contact, no reply expected). The ITU text has no etymology.

How the user's belief fits: "silent key" for a deceased operator is a real, documented meaning of "SK" (ARRL guideline page; Ethics glossary). It is the second of two meanings the Ethics glossary lists, and the glossary labels the prosign meaning first. The prosign on the air is sent run together at the end of the QSO. In written form (a config file, a log note, a typed macro) the two uses look identical, which is the ambiguity; the on-air convention is the end-of-contact one. None of these sources says SK sent at the end of a QSO is read as an announcement of a death, and none says to avoid it. (That a given listener might read it so is **UNVERIFIED**.)

### How it is sent

- Prosign: two characters "combined into one without the inter-character space" (Ethics II.9.3). SK is `...-.-`.
- The Ethics manual lists AR, AS, CL, SK, HH as prosigns and says "BK and KN are not prosigns, as the two letters of these codes are sent with a space in between." (ITU lists K `-.-` and the wait signal `.-...`, which is AS, as single signals.)
- ITU spacing (M.1677-1 Part I 2): dash = 3 dots, gap inside a letter = 1 dot, between letters = 3 dots, between words = 7 dots. A prosign omits the 3-dot letter gap.
- Hamtools implication: the keyer path sends text through hamlib `b` / the KX3 `KY` buffer. Whether the radio's text keyer accepts a prosign notation (such as `<SK>` or `[SK]`) is **not** covered by any of these sources; typing "SK" in the text sends S and K with a letter gap, which is "SK" as two letters and not the prosign. Untested on hardware.

## 2. Conventional closing and calling signals (what the sources say)

| Signal | Source | Statement |
|---|---|---|
| DE | ITU Part II 1.1, 2; Ethics | Calling form: "the call sign (not more than twice) of the station required, the word DE followed by its own call sign". The reply is the same shape. Ethics examples: `G4ZZZ DE ON6YYY ON6YYY AR`. |
| K | ITU Part I; Ethics; POTA | K = invitation to transmit, "over to you". |
| KN | Ethics II.9.10 | "Over to you only", for when you want one station and no breakers. Not a prosign (K and N spaced). |
| AR | ITU Part II 5-6 (the "cross" `.-.-.` ends every telegram and a transmission); Ethics; ACP 130(A) | End of transmission. Ethics: at the end of a CQ and when you call a station; "AR K" and "AR KN" are "do NOT use". |
| BK | Ethics II.9.7 | Break; fast hand-over without callsigns, "the CW equivalent of 'over'". |
| EE / "dits" | POTA CW Guide (K0GUZ's article) | "He may reply 'QSL' or maybe 'TNX' or perhaps a couple of dits. If he sends you a couple of dits, send two back to him." POTA's own text: activators use "TU CA 73 e e" (hunter's state code, 73, then two Es). |
| TU | POTA CW Guide; Ethics glossary | "TU: thank you." POTA: "TU CA 73 e e". |
| 73 | Ethics glossary | "best regards". Never "73s". POTA: "QSOs usually end with brief but proper 73s" (the guide itself writes "73s"; the Ethics manual says 73 is already plural). |
| R | Ethics glossary; ARRL NTS manual | Ethics: "roger, yes, I confirm, received". The NTS manual says R is "Roger, received and understood (Does NOT mean yes!)". The two ARRL-hosted documents disagree on "yes". |
| CL | Ethics; POTA Hunter Guide | Closing down. POTA: activators make a "last call (often 'LAST CL' in CW)". |
| QRL? | Ethics; POTA Activator Guide | "is the frequency in use?... you must always use it before calling CQ on a new frequency" (Ethics); "ask if the frequency is in use (QRL? in CW)" (POTA). |
| QRZ? | Ethics | "who called me?: QRZ has no other meaning". |

## 3. CQ formats

- **General:** Ethics II.9: "much better to send a number of short CQs ('CQ CQ de F9ZZZ F9ZZZ AR') than one long spun CQ". The example ends AR; the closing table says AR at the end of a CQ, K when addressing a station. Example in its beginner QSO: `CQ CQ G4ZZZ G4ZZZ CQ CQ G4ZZZ G4ZZZ AR`. DE in the CQ is optional by the same manual's own examples.
- **POTA (CW Guide, page last modified Jul 2026):** "The sequence most activators use is: CQ POTA K7XYZ. Sometimes, they repeat the call sign." Also "When calling CQs, activators often omit 'DE' and 'K'." For RBN spotting it suggests "CQ CQ CQ POTA W0ZAP W0ZAP W0ZAP" and shows "CQ POTA W0ZAP W0ZAP US1234 K" as a message that RBN reports. "TEST" is treated like "CQ" by skimmers; "V V V TEST TEST TEST W0ZAP..." marks a test. The CW Guide also says "use your transmitter's message keyer feature to call CQs in perfect code". POTA "does not require any specific exchange" (CW Guide and Rules, v1.3: "Parks on the Air does not require a specific exchange in a QSO").
- **SOTA:** I found no SOTA primary document on CW calling form. "CQ SOTA" is the widely used call (**UNVERIFIED**; seen only in third-party talks and blog posts I did not treat as primary). The SOTA General Rules text, as quoted by a SOTA Reflector post (reflector.sota.org.uk, forum on SOTA's own domain, quoting the rules; the rules page itself was not readable): "a minimum of four QSOs must be made, each of which must be with a different station. QSOs must comprise an exchange of callsigns and signal reports, it is strongly recommended that the summit identifier be given during each contact". The forum post is a user quoting the rule; I could not confirm it against the live General Rules (**UNVERIFIED** as current wording).

## 4. Exchange of RST and callsigns

- Ethics answer-a-CQ: "Do not send at a higher speed than the station you're calling. Do not send the call of the station you are calling more than once... You can use either 'K' or 'AR' to end your call... In many cases one sends only the callsign without any closing code (AR or K) at all. This is also common practice in contests."
- Ethics report line: `ON6YYY DE W4ZZZ GE TKS FER CALL UR RST 579 579 ... HW CPY? ON6YYY DE W1ZZZ K`.
- POTA CW Guide: "When responding to a CQ, hunters simply send their callsign once (without repeating the activator's callsign followed by a 'DE,' and also omitting 'K' at the end)". Reports: "UR 569 56N MA MA" versus the old "UR RST IS 569 56N IN QTH MA MA"; QSOs "usually include a realistic RST signal report, not a perfunctory 559 or 599"; prosigns BT and AR "are rarely used because the exchange is brief". Ending: "BK", "K" or "KN" is usually used.
- POTA minimum: K0GUZ's article in the guide says "For a successful POTA contact you need only exchange calls."
- Park-to-park example in the guide: "UR 599 5nn P2P K6876 K6876 MA MA BK".
- Contest/other: the guide says "TU CA 73 e e" is typical; "5NN" appears as the hunter's report in K0GUZ's example ("5NN MI").

## 5. Is "R R 73 TU DE" idiomatic?

Not seen in any source I read. Taking the parts:

- "R" alone as acknowledgement: documented (Ethics glossary, NTS manual), but POTA states "Most Parks on the Air QSOs omit words expected from the context", and its examples confirm with "QSL" or "TNX" or dits, not "R R".
- "73 TU" (that order): not found. The documented order is "TU ... 73" (POTA: "TU CA 73 e e").
- "DE" before your own call at the close is standard: Ethics `W1ZZZ de G3ZZZ SK`, `73 JOHN CUL DE G4ZZZ SK`.

So "R R 73 TU DE X" is not wrong, but it is not a form documented in these sources. **UNVERIFIED** as to whether operators in general would call it idiomatic (no source on frequency of use).

## 6. Assessment of the current hamtools examples

Defaults and examples found in the repo: `crates/rig-tui/src/softkeys.rs` (built-in F1-F3), `docs/user-guide.md`, config module docs.

| Text in repo | Verdict against the sources |
|---|---|
| `CQ CQ DE {MYCALL} {MYCALL} K` (softkeys.rs F1) | Idiomatic. The Ethics manual's example ends a CQ with AR, POTA's examples end with K or nothing; both documented. |
| `CQ CQ CQ DE {MYCALL} {MYCALL} K` (user guide) | Same. |
| `{THEIRCALL} DE {MYCALL} UR {MYRST} {MYRST} K` (F2) | Idiomatic: matches Ethics "UR RST 579 579 ... K" and POTA's "UR 569". The doubled report is Ethics style; POTA prefers a single, realistic report. |
| `{THEIRCALL} TU 73 DE {MYCALL} EE` (F3), `TU 73 DE {MYCALL} EE` (guide) | Idiomatic per POTA ("TU CA 73 e e"). The Ethics-style alternative is `... 73 DE {MYCALL} SK`. Whether the radio sends "EE" as the two-Es sequence POTA describes (letter gap between) is **UNVERIFIED** here. |
| `CQ POTA DE {MYCALL}` (config module doc) | Valid per POTA ("CQ POTA K7XYZ"); lacks the callsign repeat and K that POTA's RBN example uses. Fine for a documentation snippet; better as below for a real macro. |
| `73 TU` (tests/fixtures only) | Not idiomatic order; fine as test data, not for docs. |
| SK in any default | Valid prosign per ITU and ARRL, but the maintainer (see `.trail/learnings/2026-11-05-never-end-an-example-or-default-matthew.md`) ruled it out of examples. The sources do not require it: POTA ends with `73 e e`. |

## 7. Recommended F1-F6 sets

Placeholders as used in the repo (`{MYCALL} {THEIRCALL} {MYRST} {MYREF}`). Each line is a suggestion tied to the quoted source, not a standard. Text typed in the keyer sends letters with normal gaps; none of the sources define text-macro notation for prosigns, so this table avoids prosigns that need run-together sending (no `AR`, `SK`, `CL`).

### General CQ (no programme)

| Key | Text | Basis |
|---|---|---|
| F1 | `CQ CQ DE {MYCALL} {MYCALL} K` | Ethics II.9: short CQs; K per its closing table (AR also valid). |
| F2 | `{THEIRCALL} DE {MYCALL} UR {MYRST} {MYRST} K` | Ethics beginner QSO. |
| F3 | `{THEIRCALL} TU 73 DE {MYCALL} EE` | Ethics closes with 73 ... DE call; POTA's "TU ... 73 e e". |
| F4 | `QRZ? DE {MYCALL}` | Ethics: QRZ? means "who called me?". |
| F5 | `PSE UR CALL AGN K` | Ethics II.9.10: `... PSE UR CALL AGN K`. |
| F6 | `QRL?` | Ethics: "you must always use it before calling CQ on a new frequency". |

### POTA activation

| Key | Text | Basis |
|---|---|---|
| F1 | `CQ POTA {MYCALL} {MYCALL} K` | POTA CW Guide: "CQ POTA K7XYZ"; "CQ POTA W0ZAP W0ZAP US1234 K". Optional DE per same page. |
| F2 | `{THEIRCALL} UR {MYRST} K` (or `BK`) | POTA: "UR 569 ..."; "BK", "K" or "KN" usually used; prefer a single, realistic report. |
| F3 | `{THEIRCALL} TU 73 DE {MYCALL} EE` | POTA: "TU CA 73 e e". |
| F4 | `QRZ? POTA DE {MYCALL}` | Ethics QRZ?; POTA's own CQ words are "CQ POTA". **UNVERIFIED** as a POTA-specific phrase. |
| F5 | `UR {MYRST} P2P {MYREF} {MYREF} BK` | POTA park-to-park example (`UR 599 5nn P2P K6876 K6876 MA MA BK`). The guide's example is the hunting activator's call; this is the same shape sent by the activator being hunted (**UNVERIFIED** as POTA practice). |
| F6 | `QRL?` | POTA Activator Guide: "ask if the frequency is in use (QRL? in CW)". |

### SOTA activation

| Key | Text | Basis |
|---|---|---|
| F1 | `CQ SOTA {MYCALL} {MYCALL} K` | Pattern copied from POTA and Ethics; "CQ SOTA" itself **UNVERIFIED** (no SOTA primary found). |
| F2 | `{THEIRCALL} UR {MYRST} {MYREF} K` | SOTA General Rules, as quoted on the SOTA Reflector: exchange callsigns and signal reports, "strongly recommended that the summit identifier be given during each contact". The exact summit-ID format on air is not specified anywhere I read. |
| F3 | `{THEIRCALL} TU 73 DE {MYCALL} EE` | Same as POTA. |
| F4 | `QRZ? SOTA DE {MYCALL}` | **UNVERIFIED** phrase. |
| F5 | `S2S {MYREF} {MYREF} K` | "S2S" is the SOTA term in the SOTA data bundle (summit-to-summit is a separate field in the CSV import); the on-air phrase is **UNVERIFIED**. |
| F6 | `QRL?` | Ethics. |

## 8. Not verified

- The ARRL Operating Manual, ARRL Handbook, ITU Radio Regulations (which govern call-sign identification, not prosign use, to my knowledge, **UNVERIFIED**) and ACP 131 were not read.
- A web-search summary said an ARRL "quick reference operating aids" page says "SK is sent before the final identification". I could not get the page text, so that claim is **UNVERIFIED**.
- The same summary said the ARRL "Ham Radio History" page traces SK to "30". I could not fetch that page; the "30" origin above comes from the ARRL NTS manual, which I did read.
- The claim that other amateurs may read an end-of-QSO SK as a death notice is anecdotal and not in any source I read.
- No frequency-of-use data for any phrase (R R, 73 TU, EE).

## Citations

All retrieved 2026-11-04/05.

- ITU-R M.1677-1 (10/2009), International Morse code: <https://www.itu.int/dms_pubrec/itu-r/rec/m/R-REC-M.1677-1-200910-I!!PDF-E.pdf>
- ARRL, Silent Key Submission Guidelines: <https://www.arrl.org/silent-key-submission-guidelines>
- ARRL-hosted, authored by ON4UN/ON4WW, *Ethics and Operating Procedures for the Radio Amateur*, Edition 3, June 2010 (ARRL-corrected Jan 2011): <https://www.arrl.org/files/file/DXCC/Eth-operating-EN-ARRL-CORR-JAN-2011.pdf>
- ARRL NTS, Public Service Communications Manual App. B, Ch. 5 Net Control (W3YVQ v5.04 5/02): <https://www.arrl.org/files/file/Public%20Service/MPG504A.pdf>
- ARRL NTS, same manual, Ch. 3 Sending Messages on CW (W3YVQ v3.01 5/02): <https://www.arrl.org/files/file/Public%20Service/MPG304A.pdf>
- ACP 130(A), July 2005, Signalling Procedures in the Visual Medium (hosted copy): <https://www.commsmuseum.co.uk/publications/ACP130/acp130a2005.pdf>
- Parks on the Air, CW Guide (original author AB1WX; includes K0GUZ article): <https://docs.pota.app/docs/cw_guide.html>
- Parks on the Air, Rules v1.3: <https://docs.pota.app/docs/rules.html>
- Parks on the Air, Hunter Guide: <https://docs.pota.app/docs/hunter_guide-english.html>
- Parks on the Air, Activator Guide: <https://docs.pota.app/docs/activator_reference/activator_guide-english.html>
- SOTA Reflector (quotes the General Rules; not the rules page itself): <https://reflector.sota.org.uk/t/definition-of-a-valid-sota-qso/26912>
