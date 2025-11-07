use core::{cmp::max, panic::PanicInfo};
use crypto_bigint::U256;

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

#[inline]
fn u64_to_u256(x: u64) -> U256 {
    U256::from(x)
}

#[inline]
fn u256_floor_to_u64(x: U256) -> u64 {
    // Grab the low 64 bits without any formatting/panicking code.
    let le = x.to_le_bytes(); // [u8; 32]
    let mut out = 0u64;
    // Compose from the first 8 little-endian bytes.
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

    // Compute:
    // delta = floor( account_portion * cluster_effective * rate_bps
    //                / (cluster_portion * 10_000) )
    // Then clamp to <= account_portion (as in the original).
    //
    // NOTE: With inputs <= u64::MAX and rate_bps <= 10_000,
    // the 3-term product fits in < 2^192, so U256 is sufficient without overflow.
    let a   = u64_to_u256(account_portion);
    let ce  = u64_to_u256(cluster_effective);
    let r   = u64_to_u256(rate_bps);
    let cp  = u64_to_u256(cluster_portion);
    let tenk = U256::from(BASIS_POINTS_PER_UNIT);

    // All ops are infallible; no unwraps/panics involved.
    let num = a * ce * r;
    let den = cp * tenk;

    // Denominator is non-zero due to the early return above.
    let q = num / den;

    let delta = u256_floor_to_u64(q);
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
    let effective = max(cluster_share << 1, 1); // avoid 64-bit mul

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
