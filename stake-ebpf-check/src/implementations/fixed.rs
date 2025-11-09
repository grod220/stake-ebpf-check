use crate::{StakeCalculator, warmup_cooldown_rate_bps, Epoch, BASIS_POINTS_PER_UNIT};
use core::ops::{DivAssign, MulAssign};
use fixed_bigint::fixeduint::FixedUInt;
use fixed_bigint::num_traits::ToPrimitive;

type U256x16 = FixedUInt<u16, 16>;

pub struct FixedCalculator;

#[inline]
fn u256_floor_to_u64(x: &U256x16) -> u64 {
    match x.to_u64() {
        Some(v) => v,
        None => u64::MAX,
    }
}

impl StakeCalculator for FixedCalculator {
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

        let mut num = U256x16::from(account_portion);
        let ce = U256x16::from(cluster_effective);
        let r = U256x16::from(rate_bps);

        num.mul_assign(&ce);
        num.mul_assign(&r);

        let mut den = U256x16::from(cluster_portion);
        let tenk = U256x16::from(BASIS_POINTS_PER_UNIT);
        den.mul_assign(&tenk);

        num.div_assign(&den);

        let delta = u256_floor_to_u64(&num);
        if delta > account_portion {
            account_portion
        } else {
            delta
        }
    }
}
