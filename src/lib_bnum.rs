
use core::{cmp::max, panic::PanicInfo};
use bnum::BUintD16;

// ---- minimal no-op allocator (required because `bnum` links `alloc`) ----
use core::alloc::{GlobalAlloc, Layout};

struct NoAlloc;
unsafe impl GlobalAlloc for NoAlloc {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 { core::ptr::null_mut() }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
#[global_allocator]
static GLOBAL: NoAlloc = NoAlloc;

// ---- bigint: 16-bit limbs so the backend never needs 64-bit muls ----------
// If you *know* 32-bit muls are OK in your environment, you can switch to:
//   type U = BUintD32<8>;  // 8 * 32 = 256 bits
type U = BUintD16<16>;      // 16 * 16 = 256 bits (most conservative)

#[inline]
fn u64_to_u(x: u64) -> U { U::from(x) }

#[inline]
fn u_to_u64_floor(x: U) -> u64 {
    // Prefer core-only conversion: TryFrom<BUint*> for u64 is implemented.
    match <u64 as core::convert::TryFrom<U>>::try_from(x) {
        Ok(v) => v,
        Err(_) => u64::MAX, // value > u64::MAX; we'll clamp to account_portion anyway
    }
}

// ---- your original types / constants ---------------------------------------
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

// ---- core math on U (256-bit), no panics, no fmt/alloc use -----------------
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

    // delta = floor( account * effective * rate_bps / (cluster_portion * 10_000) ), clamped.
    let a    = u64_to_u(account_portion);
    let ce   = u64_to_u(cluster_effective);
    let rate = u64_to_u(rate_bps);
    let cp   = u64_to_u(cluster_portion);
    let tenk = U::from(BASIS_POINTS_PER_UNIT);

    // All ops stay inside the 256-bit integer; no overflow for u64^3 (fits in < 2^192).
    let num = a * ce * rate;
    let den = cp * tenk; // non-zero due to early return

    let q = num / den;
    let delta = u_to_u64_floor(q);
    if delta > account_portion { account_portion } else { delta }
}

// ---- entrypoint / panic handler --------------------------------------------
#[no_mangle]
pub extern "C" fn entrypoint(arg: u64) -> u64 {
    let account_stake = (arg & 0xffff) + 1;
    let cluster_share = ((arg >> 16) & 0xffff) + 1;
    let effective = max(cluster_share << 1, 1); // avoid u64 mul

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
fn panic(_info: &PanicInfo) -> ! { loop {} }
