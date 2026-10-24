use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::{Elevation, Floor};

/// Proof that this process may change something here, which only [`gate`] makes: everything
/// that writes, removes, renews or starts a run takes one.
#[derive(Debug, Clone, Copy)]
pub struct Permit {
    _only_the_gate_makes_one: (),
}

#[cfg(test)]
impl Permit {
    // For a unit test below the gate. A test of the gate, or of a change through `Pitboard`,
    // asks the gate.
    pub(crate) fn for_a_test() -> Permit {
        Permit {
            _only_the_gate_makes_one: (),
        }
    }
}

// A file or keychain item written by root or an administrator might never be read, replaced
// or removed by the person's own runs again, and one under a relative home would land under
// whichever folder it ran from.
pub(crate) fn gate(ctx: &Context) -> Result<Permit> {
    crate::release::check()?;
    // Before elevation, which cannot change it.
    match ctx.host().floor() {
        Floor::Met => {}
        Floor::Below { build } => return Err(Error::SystemTooOld { build: Some(build) }),
        Floor::Unknown => return Err(Error::SystemTooOld { build: None }),
    }
    match ctx.host().elevation(ctx) {
        Elevation::Normal => {}
        Elevation::Elevated { why } => return Err(Error::Elevated { why: Some(why) }),
        Elevation::Unknown => return Err(Error::Elevated { why: None }),
    }
    // After elevation, so a run under `sudo` is told that first.
    crate::home::check_absolute(ctx)?;
    Ok(Permit {
        _only_the_gate_makes_one: (),
    })
}
