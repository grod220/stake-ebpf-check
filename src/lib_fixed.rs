use core::{cmp::max, ops::{DivAssign, MulAssign}, panic::PanicInfo};

use fixed_bigint::fixeduint::FixedUInt;
use fixed_bigint::num_traits::ToPrimitive;

type U256x16 = FixedUInt<u16, 16>;

#[inline]
fn u64_to_u256(x: u64) -> U256x16 {
    U256x16::from(x)
}

#[inline]
fn u256_floor_to_u64(x: &U256x16) -> u64 {
    match x.to_u64() {
        Some(v) => v,
        None => u64::MAX,
    }
}

pub type Epoch = u64;

pub mod stake_history {
    #[derive(Clone, Copy)]
    pub struct StakeHistoryEntry {
        pub activating: u64,
        pub deactivating: u64,
        pub effective: u64,
    }
}
use stake_history::StakeHistoryEntry;

const BASIS_POINTS_PER_UNIT: u64 = 10_000;
pub const ORIGINAL_WARMUP_COOLDOWN_RATE_BPS: u64 = 2_500;
pub const TOWER_WARMUP_COOLDOWN_RATE_BPS: u64 = 900;

#[inline]
pub fn warmup_cooldown_rate_bps(epoch: Epoch, new_rate_activation_epoch: Option<Epoch>) -> u64 {
    if epoch < new_rate_activation_epoch.unwrap_or(u64::MAX) {
        ORIGINAL_WARMUP_COOLDOWN_RATE_BPS
    } else {
        TOWER_WARMUP_COOLDOWN_RATE_BPS
    }
}

pub fn calculate_activation_allowance(
    current_epoch: Epoch,
    account_activating_stake: u64,
    prev_epoch_cluster_state: &StakeHistoryEntry,
    new_rate_activation_epoch: Option<Epoch>,
) -> u64 {
    rate_limited_stake_change(
        current_epoch,
        account_activating_stake,
        prev_epoch_cluster_state.activating,
        prev_epoch_cluster_state.effective,
        new_rate_activation_epoch,
    )
}

pub fn calculate_deactivation_allowance(
    current_epoch: Epoch,
    account_deactivating_stake: u64,
    prev_epoch_cluster_state: &StakeHistoryEntry,
    new_rate_activation_epoch: Option<Epoch>,
) -> u64 {
    rate_limited_stake_change(
        current_epoch,
        account_deactivating_stake,
        prev_epoch_cluster_state.deactivating,
        prev_epoch_cluster_state.effective,
        new_rate_activation_epoch,
    )
}

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

    let mut num = u64_to_u256(account_portion);
    let ce = u64_to_u256(cluster_effective);
    let r = u64_to_u256(rate_bps);

    num.mul_assign(&ce);
    num.mul_assign(&r);

    let mut den = u64_to_u256(cluster_portion);
    let tenk = u64_to_u256(BASIS_POINTS_PER_UNIT);
    den.mul_assign(&tenk);

    num.div_assign(&den);

    let delta = u256_floor_to_u64(&num);
    if delta > account_portion {
        account_portion
    } else {
        delta
    }
}

#[no_mangle]
pub extern "C" fn entrypoint(arg: u64) -> u64 {
    let account_stake = (arg & 0xffff) + 1;
    let cluster_share = ((arg >> 16) & 0xffff) + 1;
    let effective = max(cluster_share << 1, 1);

    let cluster_state = StakeHistoryEntry {
        activating: cluster_share,
        deactivating: (cluster_share / 2) + 1,
        effective,
    };

    let activation =
        calculate_activation_allowance(arg, account_stake, &cluster_state, Some(arg / 3));
    let deactivation = calculate_deactivation_allowance(
        arg,
        (account_stake / 2) + 1,
        &cluster_state,
        Some(arg / 5),
    );

    activation ^ deactivation
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
