# Ham satellite operating

Terminal tools for a licensed amateur operator working amateur-radio satellites with a Yaesu FT-847 (and an Elecraft KX3 for CW). The language here is the operator's, not the orbital-mechanics textbook's.

## Language

### Passes

**Pass**:
One continuous period during which a satellite stays at or above the minimum elevation as seen from the operator's position.
_Avoid_: Window, visibility, flyover

**Minimum elevation**:
The lowest angle above the horizon at which a satellite counts as usable; it decides where a pass starts and ends.
_Avoid_: Horizon, mask, cutoff

**AOS** (acquisition of signal):
The moment a satellite rises to the minimum elevation, starting a pass.
_Avoid_: Rise, start time

**LOS** (loss of signal):
The moment a satellite falls below the minimum elevation, ending a pass.
_Avoid_: Set, end time

**TCA** (time of closest approach):
The moment within a pass when the satellite is highest in the sky; the elevation then is the pass's maximum elevation.
_Avoid_: Peak, culmination

**Pass duration**:
The time from a pass's AOS to its LOS: how long the satellite is usable. It is a property of the whole pass, so it stays the same however much of the pass has already gone by.
_Avoid_: Visible window, visibility time, time remaining

**Short pass**:
A pass whose duration is below the operator's threshold (three minutes by default): easy to miss and worth a warning.
_Avoid_: Brief pass, low pass

**Tracked pass**:
The current pass of the satellite being tracked, or its next pass if none is under way, whichever row the cursor is on.
_Avoid_: Selected pass, active pass

### Radio

**Transmitter**:
A signal source or relay on a satellite, as catalogued by SatNOGS: a repeater, a linear transponder or a beacon.
_Avoid_: Channel, frequency

**Downlink**:
The frequency (or passband) on which a transmitter sends to the ground, and so what the operator receives.
_Avoid_: RX frequency

**Uplink**:
The frequency (or passband) on which a transmitter listens, and so what the operator sends to. A beacon has none.
_Avoid_: TX frequency

**Tracking**:
Keeping the radio's receive and transmit frequencies on a transmitter: at its nominal frequencies before AOS, corrected for Doppler shift from AOS, and always moving with the operator's dial. Transmitting itself is always the operator's action.
_Avoid_: Auto-tune, rig control

### Logging

**Contact**:
One two-way exchange with another station through a satellite, as recorded by the operator.
_Avoid_: QSO (except in ADIF field names), call, log entry

**Log**:
The single append-only file holding every Contact, whatever the activity. Not a diagnostic log.
_Avoid_: logbook (that is the crate's name), database

### Station

**Locator**:
The 6-character Maidenhead square that says where the operator's station is. It is coarse by design (about 4.6 km by 3.5 km), so it never pins a home. Stored in the shared config; latitude and longitude are never stored.
_Avoid_: Grid, grid square (except in ADIF field names and existing flag names)

### Rig app

**Mode panel**:
The part of the rig app's screen that exists only while the rig is in a mode it serves, such as the CW transcript and input line in CW. Only one shows at a time; with no panel for the current mode, the space shows the session's recent Contacts.
_Avoid_: Plugin, tab, mode screen

**Soft key**:
One of the six labelled F-key slots under the CW panel, each bound to a canned message that is sent as CW when pressed. Where the message is stored (the config file today, the radio's own message buffers later) is not part of the term.
_Avoid_: Macro button, hotkey
