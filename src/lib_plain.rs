use core::{cmp::max, panic::PanicInfo};

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

#[no_mangle]
pub extern "C" fn entrypoint(arg: u64) -> u64 {
    let account_stake = (arg & 0xffff) + 1;
    let cluster_share = ((arg >> 16) & 0xffff) + 1;
    let effective = max(cluster_share * 2, 1);

    let cluster_state = StakeHistoryEntry {
        activating: cluster_share,
        deactivating: (cluster_share / 2) + 1,
        effective,
    };

    let activation =
        calculate_activation_allowance(arg, account_stake, &cluster_state, Some(arg / 3));
    let deactivation =
        calculate_deactivation_allowance(arg, (account_stake / 2) + 1, &cluster_state, Some(arg / 5));

    activation ^ deactivation
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
