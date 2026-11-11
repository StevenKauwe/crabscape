use super::*;
use crate::team_vote::Purchase;
impl Adventure {
    pub fn team_built(&self) -> std::collections::BTreeSet<String> {
        std::iter::once(self.profile())
            .chain(
                self.others
                    .iter()
                    .take(if self.live_team { self.others.len() } else { 0 }),
            )
            .flat_map(|p| p["built"].as_array().into_iter().flatten())
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    }

    /// Recompute source eligibility and cost on a disposable copy, never a save.
    pub fn quote_purchase(&self, purchase: &Purchase) -> Result<u32, Error> {
        let mut p = self.profile().clone();
        let events = progression::apply(
            &mut p,
            &self.others,
            !self.live_team,
            purchase.command(),
            0.,
        )
        .map_err(Error::Progression)?;
        events
            .iter()
            .find_map(|e| {
                if let progression::Event::Spent { cost } = e {
                    Some(*cost as u32)
                } else {
                    None
                }
            })
            .ok_or(Error::InvalidState)
    }
    /// Invoked only after a serialized grant (or explicitly while alone).
    pub fn apply_purchase(
        &mut self,
        purchase: &Purchase,
        expected_cost: u32,
        clock: Clock,
    ) -> Result<Vec<Event>, Error> {
        if self.quote_purchase(purchase)? != expected_cost {
            return Err(Error::InvalidState);
        }
        self.roost_command(purchase.command(), clock)
    }
}
