use crate::{StakeCalculator, warmup_cooldown_rate_bps, Epoch};

// BPF-compatible implementation using streaming arithmetic from bpf-math.
// Computes: delta = floor((account_portion * cluster_effective * rate_bps) / (cluster_portion * 10_000))
// No u128, no aggregate returns, minimal stack usage.

pub struct EbpfStreamingCalculator;

impl StakeCalculator for EbpfStreamingCalculator {
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

        // Cap q1 early so q1*rate_bps <= account_portion
        let q_cap = account_portion / rate_bps;

        let (q1, rem_hi, rem_lo) = match bpf_math::mul_div_by_cp10k_capped(
            account_portion,
            cluster_effective,
            cluster_portion,
            q_cap,
        ) {
            Some(t) => t,
            None => return account_portion, // would exceed cap after *rate_bps
        };

        // total = q1 * rate_bps, computed with saturation
        let total = bpf_math::mul_cap(q1, rate_bps, account_portion);
        if total >= account_portion {
            return account_portion;
        }

        // Add floor(rem * rate_bps / M)
        let t2 = bpf_math::remainder_mul_div(rem_hi, rem_lo, rate_bps, cluster_portion);
        let room = account_portion - total;
        if t2 >= room { account_portion } else { total + t2 }
    }
}
