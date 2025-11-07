#![no_std]
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

pub const BASIS_POINTS_PER_UNIT: u64 = 10_000;
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

pub trait StakeCalculator {
    fn rate_limited_stake_change(
        epoch: Epoch,
        account_portion: u64,
        cluster_portion: u64,
        cluster_effective: u64,
        new_rate_activation_epoch: Option<Epoch>,
    ) -> u64;
}

pub fn calculate_activation_allowance<T: StakeCalculator>(
    current_epoch: Epoch,
    account_activating_stake: u64,
    prev_epoch_cluster_state: &StakeHistoryEntry,
    new_rate_activation_epoch: Option<Epoch>,
) -> u64 {
    T::rate_limited_stake_change(
        current_epoch,
        account_activating_stake,
        prev_epoch_cluster_state.activating,
        prev_epoch_cluster_state.effective,
        new_rate_activation_epoch,
    )
}

pub fn calculate_deactivation_allowance<T: StakeCalculator>(
    current_epoch: Epoch,
    account_deactivating_stake: u64,
    prev_epoch_cluster_state: &StakeHistoryEntry,
    new_rate_activation_epoch: Option<Epoch>,
) -> u64 {
    T::rate_limited_stake_change(
        current_epoch,
        account_deactivating_stake,
        prev_epoch_cluster_state.deactivating,
        prev_epoch_cluster_state.effective,
        new_rate_activation_epoch,
    )
}

mod implementations;

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

    #[cfg(feature = "bnum")]
    type Calculator = implementations::bnum::BnumCalculator;
    
    #[cfg(feature = "crypto")]
    type Calculator = implementations::crypto::CryptoCalculator;
    
    #[cfg(feature = "fixed")]
    type Calculator = implementations::fixed::FixedCalculator;
    
    #[cfg(feature = "plain")]
    type Calculator = implementations::plain::PlainCalculator;

    let activation =
        calculate_activation_allowance::<Calculator>(arg, account_stake, &cluster_state, Some(arg / 3));
    let deactivation = calculate_deactivation_allowance::<Calculator>(
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
