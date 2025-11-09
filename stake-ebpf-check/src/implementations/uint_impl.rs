use crate::{
    StakeCalculator,
    warmup_cooldown_rate_bps,
    Epoch,
    BASIS_POINTS_PER_UNIT,
};
use core::alloc::{GlobalAlloc, Layout};
use uint::construct_uint;

struct NoAlloc;

unsafe impl GlobalAlloc for NoAlloc {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static GLOBAL: NoAlloc = NoAlloc;

construct_uint! {
    /// 256-bit unsigned integer used for stake math.
    pub struct U256(4);
}

pub struct UintCalculator;

impl StakeCalculator for UintCalculator {
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

        let a = U256::from(account_portion);
        let ce = U256::from(cluster_effective);
        let rate = U256::from(rate_bps);
        let cp = U256::from(cluster_portion);
        let tenk = U256::from(BASIS_POINTS_PER_UNIT);

        let num = a * ce * rate;
        let den = cp * tenk;

        if den.is_zero() {
            return account_portion;
        }

        let q = num / den;
        let max_delta = U256::from(account_portion);
        let capped = if q > max_delta { max_delta } else { q };

        capped.low_u64()
    }
}
