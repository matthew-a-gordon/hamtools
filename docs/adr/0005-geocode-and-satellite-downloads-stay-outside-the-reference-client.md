# Address lookup and satellite downloads stay outside the reference client, behind `--offline`

ADR 0003 says every external request goes through the one `ReferenceClient`. Two requests do not, by choice. The Nominatim address lookup in the config dialog and the TLE and transmitter downloads for Satellite keep their own small blocking clients (`ureq` on a short-lived thread). We chose this over routing them through `ReferenceClient` because the client is built for cached, keyed feeds with a disk snapshot and conditional requests, and neither fits: an address lookup must never be written to disk (only the derived Locator is kept, and the typed address and its answer live in memory for one dialog session), and the satellite downloads already have their own cache, staleness rule and merge logic carried over from `sat-tui`. Forcing them in would mean a second kind of feed in the client for the sake of one call site each.

The rules of ADR 0003 still hold for them in spirit. Neither makes a request at startup: the lookup runs only when the operator asks for one, and the satellite download runs on entering Satellite when its data is stale. The lookup keeps Nominatim's one-request-per-second spacing and a descriptive User-Agent. `--offline` stops both, along with every fetch the reference data would make (plan save, first lookup, explicit refresh), so with it nothing in the app touches the network. Config's `[reference]` intervals slow only the `ReferenceClient`; these two have their own fixed limits.

Reopen this if a third caller appears that wants an uncached, rate-limited GET, at which point a client method for it is worth its keep.

Amends ADR 0003; specified in #54.
