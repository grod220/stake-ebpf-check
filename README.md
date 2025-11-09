Given that:

crypto-bigint – uses 128-bit “double wide” digit for mul

uint – also uses 128-bit intermediates

bnum – stack blowup + likely 128-bit in mul

apint, num-bigint-dig, dashu-int, ibig – all use 64-bit limbs with 128-bit temporaries for mul/div (same fundamental problem)

awint – more bit-vector focused, but its mul also goes through wide temporaries

…I’m honestly not aware of any off-the-shelf bigint crate that:

is no_std (or core+alloc) friendly and

does not use u128 or __multi3 internally for mul, and

keeps stack usage within BPF’s tiny per-function budget.