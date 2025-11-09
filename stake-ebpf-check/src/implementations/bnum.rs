use crate::{StakeCalculator, warmup_cooldown_rate_bps, Epoch, BASIS_POINTS_PER_UNIT};
use bnum::BUintD16;
use core::alloc::{GlobalAlloc, Layout};

struct NoAlloc;
unsafe impl GlobalAlloc for NoAlloc {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 { core::ptr::null_mut() }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
#[global_allocator]
static GLOBAL: NoAlloc = NoAlloc;

type U = BUintD16<16>;

pub struct BnumCalculator;

impl StakeCalculator for BnumCalculator {
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

        let a    = U::from(account_portion);
        let ce   = U::from(cluster_effective);
        let rate = U::from(rate_bps);
        let cp   = U::from(cluster_portion);
        let tenk = U::from(BASIS_POINTS_PER_UNIT);

        let num = a * ce * rate;
        let den = cp * tenk;

        let q = num / den;
        let delta = match <u64 as core::convert::TryFrom<U>>::try_from(q) {
            Ok(v) => v,
            Err(_) => u64::MAX,
        };
        
        if delta > account_portion { account_portion } else { delta }
    }
}
