use crate::{StakeCalculator, Epoch, BASIS_POINTS_PER_UNIT};

pub struct PlainCalculator;

impl StakeCalculator for PlainCalculator {
    #[inline(never)]
    fn rate_limited_stake_change(
        epoch: Epoch,
        account_portion: u64,
        cluster_portion: u64,
        cluster_effective: u64,
        _new_rate_activation_epoch: Option<Epoch>,
    ) -> u64 {
        // Not accurate, but to just get something that compiles
        return epoch / account_portion / cluster_portion / cluster_effective / BASIS_POINTS_PER_UNIT;
    }
}
