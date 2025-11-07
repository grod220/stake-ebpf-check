use crate::{StakeCalculator, warmup_cooldown_rate_bps, Epoch, BASIS_POINTS_PER_UNIT};
use crypto_bigint::U256;

pub struct CryptoCalculator;

#[inline]
fn u256_floor_to_u64(x: U256) -> u64 {
    let le = x.to_le_bytes();
    let mut out = 0u64;
    out |= le[0] as u64;
    out |= (le[1] as u64) << 8;
    out |= (le[2] as u64) << 16;
    out |= (le[3] as u64) << 24;
    out |= (le[4] as u64) << 32;
    out |= (le[5] as u64) << 40;
    out |= (le[6] as u64) << 48;
    out |= (le[7] as u64) << 56;
    out
}

impl StakeCalculator for CryptoCalculator {
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

        let a   = U256::from(account_portion);
        let ce  = U256::from(cluster_effective);
        let r   = U256::from(rate_bps);
        let cp  = U256::from(cluster_portion);
        let tenk = U256::from(BASIS_POINTS_PER_UNIT);

        let num = a * ce * r;
        let den = cp * tenk;

        let q = num / den;

        let delta = u256_floor_to_u64(q);
        if delta > account_portion {
            account_portion
        } else {
            delta
        }
    }
}
