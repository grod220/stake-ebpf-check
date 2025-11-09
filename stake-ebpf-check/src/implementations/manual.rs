use crate::{StakeCalculator, warmup_cooldown_rate_bps, Epoch, BASIS_POINTS_PER_UNIT};

pub struct ManualCalculator;

impl StakeCalculator for ManualCalculator {
    #[inline(never)]
    fn rate_limited_stake_change(
        epoch: Epoch,
        account_portion: u64,
        cluster_portion: u64,
        cluster_effective: u64,
        new_rate_activation_epoch: Option<Epoch>,
    ) -> u64 {
        if account_portion == 0 || cluster_portion == 0 || cluster_effective == 0 {
            return 0;
        }

        let rate_bps = warmup_cooldown_rate_bps(epoch, new_rate_activation_epoch);
        let numerator = (account_portion as u128)
            .checked_mul(cluster_effective as u128)
            .and_then(|x| x.checked_mul(rate_bps as u128));
        let denominator = (cluster_portion as u128).saturating_mul(BASIS_POINTS_PER_UNIT as u128);

        match numerator {
            Some(n) => {
                let delta = n.checked_div(denominator).unwrap();
                delta.min(account_portion as u128) as u64
            }
            None => account_portion,
        }
    }
}
