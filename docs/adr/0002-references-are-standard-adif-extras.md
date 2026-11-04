# References are stored as standard ADIF extras, with no custom Activity field

A Contact made for SOTA or POTA records its references in standard ADIF fields (`MY_SIG`/`MY_SIG_INFO` and `SIG`/`SIG_INFO` for POTA, `MY_SOTA_REF`/`SOTA_REF` for SOTA), so any other logging program can read the Log. We did not add an `ACTIVITY` field to tag which Activity logged a Contact: the Activity tag exists only in the in-memory session list. One "Their ref" field makes a Contact park-to-park or summit-to-summit, with no separate flag, and an n-fer is one Contact listing its parks, fanned out into one file per park at export. The cost is that the Log cannot say which Activity made a Contact unless the references imply it.

Specified in #54; decided in #35 and "Contact form per Role: fields, prefill and quick entry" (#48).
