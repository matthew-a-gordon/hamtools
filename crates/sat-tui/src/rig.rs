//! Applying [`RigCommand`]s to a rig through rigctld.
//!
//! Written for Hamlib's FT-847 backend (model 1001, read from the 4.6.5 source): split on/off
//! switches the radio's satellite mode, plain frequency/mode calls address the SAT RX VFO and
//! the split calls the SAT TX VFO. Nothing here can key the transmitter.

use rigctld_client::{Client, Result};

use crate::tracker::RigCommand;

/// Send `commands` in order, stopping at the first failure.
pub async fn apply(client: &mut Client, commands: &[RigCommand]) -> Result<()> {
    for command in commands {
        match *command {
            RigCommand::SatMode(true) => client.set_split(true, "VFOB").await?,
            RigCommand::SatMode(false) => client.set_split(false, "VFOA").await?,
            // Passband 0 leaves the choice to the radio.
            RigCommand::RxMode(mode) => client.set_mode(mode.name(), 0).await?,
            RigCommand::TxMode(mode) => client.set_split_mode(mode.name(), 0).await?,
            RigCommand::RxFreq(hz) => client.set_freq(hz).await?,
            RigCommand::TxFreq(hz) => client.set_split_freq(hz).await?,
        }
    }
    Ok(())
}
