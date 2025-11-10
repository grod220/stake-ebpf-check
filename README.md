The repository builds with:

```toml
# .cargo/config.toml
[build]
target = "bpfel-unknown-none"
[unstable]
build-std = ["core", "alloc"]
```

This locks every crate into `#![no_std]` + `alloc` only, and forces LLVM to emit code for Solana’s `bpfel-unknown-none` backend. That target imposes sharp limits:

- **No `u128` or compiler-rt helpers** – LLVM lowers 64×64 multiplies to the `__multi3` builtin. BPF toolchains don’t provide it, so any code path that asks for a “double-width” product simply fails to link.
- **Tiny per-function stacks (~512 bytes)** – Returning large structs by value or keeping `struct { u64 limbs[16]; }` on the stack causes verifier errors like “Looks like the BPF stack limit is exceeded.”
- **No aggregate returns / stack arguments** – BPF’s calling convention insists everything fit in registers. If LLVM thinks a function returns a big struct or needs a temporary on the caller’s stack, compilation aborts.
- **`no_std` only** – We get `core` and `alloc`, nothing else. Heap allocation is technically possible via a `GlobalAlloc`, but Solana programs usually run with dummy allocators so the hot path must stay allocation-free.

The stake math itself is simple in pseudocode:

```
delta = min(account_portion,
            floor(account_portion * cluster_effective * rate_bps
                  / (cluster_portion * 10_000)))
```

…but the intermediate product needs ~192 bits of headroom, which is why the “manual” reference implementation uses `u128`.

---

## Why existing bigint crates fail

We surveyed a handful of popular options, and every one hit at least one BPF-specific wall:

| Crate | Limbs | Failure mode on BPF |
| --- | --- | --- |
| `crypto-bigint` | 32/64-bit limbs | Always promotes limb multiplies to a `WideWord` (`u128` on 64-bit), so LLVM emits `__multi3` calls. |
| `uint` (`construct_uint!`) | Fixed 64-bit limbs | Macro expands to `let (hi, lo) = split_u128(a as u128 * b as u128);` – same intrinsic failure. |
| `bnum`, `fixed-bigint` | Configurable limb widths | Use in-place mul/div, but the combination of big structs, by-value returns, and multiple scratch arrays blows the BPF stack and triggers “aggregate return” errors. |

This led to the key insight: **as long as the arithmetic is expressed in terms of fixed-width limb arrays, LLVM will end up introducing the very `__multi3` helpers we’re trying to avoid.** We needed an approach that:

1. Keeps all intermediates in native 64-bit registers.
2. Stores only a few scalars on the stack (no `[u16; 16]` temporaries).
3. Returns everything via scalars so BPF sees no aggregate ABI.
4. Still produces the exact same integers as the `u128` baseline.

### Learnings rolled in

From the earlier experiments we should adopt following guidelines:

- **No implicit widening:** Every multiply, add, and shift is spelled out in 64-bit arithmetic. This sidesteps `__multi3` and ensures LLVM doesn’t invent aggregate temporaries.
- **In-place state only:** Functions update `(hi, lo)` and `q` via `&mut u64` parameters. Nothing returns big structs, which keeps the BPF ABI happy and the stack footprint tiny.
- **Early capping:** Instead of letting overflowing multiplies wrap (as some bigints did), we carry explicit caps through each stage. If a partial quotient would exceed `account_portion`, we bail out early, returning `account_portion` exactly as the baseline would.
- **Host-side verification:** The repository now includes `#[cfg(test)]` suites that compare the streaming helpers against straightforward `u128` math on a CPU target. Running these outside the enforced BPF config gives us coverage without compromising the production build.

Build the streaming variant exactly like the Solana toolchain would:

```bash
cargo +nightly-2025-11-01 build --features XXXX
```

---

## Verifying the BPF Output

Once you've built the BPF program, you can verify it passes both upstream Linux kernel BPF verification and Solana's BPF verifier. This ensures the bytecode is valid, safe, and executable on target platforms.

**Platform Requirements:**
- **Building the BPF binary**: Mac or Linux (with Rust toolchain)
- **Method 1 (Kernel Verification)**: Linux only (requires actual Linux kernel)
- **Method 2 (Solana Verification)**: Mac or Linux (with Solana CLI)

### Prerequisites

**All platforms (Mac/Linux):**
- Rust toolchain with `nightly-2025-11-01` (for building)
- `llvm-objdump` (for inspecting binaries)

**Method 1 only - Requires Linux machine with:**
- Kernel 5.10+ (for modern BPF support)
- `libbpf-dev` (Ubuntu/Debian) or `libbpf-devel` (RHEL/Fedora)
- `bpftool` (usually in `linux-tools-common` package)
- `readelf` (for ELF inspection)
- GCC (for compiling verification tools)

**Method 2 only - Works on Mac or Linux with:**
- Solana CLI tools (`solana-test-validator`, `solana program deploy`)

**Installation on Ubuntu/Debian (Method 1):**
```bash
sudo apt-get update
sudo apt-get install -y libbpf-dev bpftool linux-tools-common llvm gcc
```

### Upstream Linux Kernel BPF Verification

**⚠️ Requires Linux machine** - This method loads programs into the Linux kernel for verification.

This verifies your program against the **upstream Linux kernel BPF verifier** using `libbpf`, the standard BPF loading library. This is the most rigorous verification and ensures compatibility with mainline Linux.

#### Step 1: Build the Release Binary

**Can be done on Mac or Linux:**

```bash
# From the project root
cargo build --release --features streaming -p stake-ebpf-check

# Output will be at:
# target/bpfel-unknown-none/release/libstake_ebpf_check.so
```

**Important**: Use `--release` mode. Debug builds contain `.text.unlikely.` sections with static panic handlers that `libbpf` doesn't support. Release mode optimizes these away.

#### Step 2: Inspect the Binary (Optional)

Verify the binary has a clean structure:

```bash
# Check sections - should only see .text, .symtab, .strtab
llvm-objdump --section-headers target/bpfel-unknown-none/release/libstake_ebpf_check.so

# Check for relocations - should be empty or minimal
readelf -r target/bpfel-unknown-none/release/libstake_ebpf_check.so

# View disassembly
llvm-objdump -d target/bpfel-unknown-none/release/libstake_ebpf_check.so | less
```

Expected output:
```
Sections:
  .text        (5,632 bytes / 704 instructions)
  .symtab      (symbol table)
  .strtab      (string table)

# No .text.unlikely. sections
# No or minimal relocations
```

#### Step 3: Create Verification Tool

On your Linux machine, create `verify_with_libbpf.c`:

```c
#include <bpf/libbpf.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "Usage: %s <bpf_object.so>\n", argv[0]);
        return 1;
    }

    printf("=== Loading BPF object with libbpf ===\n");
    printf("File: %s\n\n", argv[1]);
    
    struct bpf_object *obj = bpf_object__open(argv[1]);
    if (!obj) {
        fprintf(stderr, "❌ Failed to open BPF object: %s\n", strerror(errno));
        return 1;
    }
    
    printf("✅ BPF object opened successfully\n");
    
    // Set all programs to socket filter type (most permissive)
    struct bpf_program *prog;
    bpf_object__for_each_program(prog, obj) {
        const char *name = bpf_program__name(prog);
        printf("  Found program: %s\n", name);
        bpf_program__set_type(prog, BPF_PROG_TYPE_SOCKET_FILTER);
    }
    
    printf("\n=== Loading into kernel (runs BPF verifier) ===\n");
    int err = bpf_object__load(obj);
    
    if (err) {
        fprintf(stderr, "\n❌ Kernel BPF verifier REJECTED\n");
        fprintf(stderr, "Error: %s\n", strerror(-err));
        bpf_object__close(obj);
        return 1;
    }
    
    printf("\n✅✅✅ Kernel BPF verifier PASSED! ✅✅✅\n\n");
    
    // Show loaded programs
    bpf_object__for_each_program(prog, obj) {
        if (bpf_program__fd(prog) >= 0) {
            printf("  Loaded: %s (FD=%d)\n", 
                   bpf_program__name(prog), 
                   bpf_program__fd(prog));
        }
    }
    
    bpf_object__close(obj);
    return 0;
}
```

#### Step 4: Compile and Run

```bash
# Compile the verifier
gcc -o verify_with_libbpf verify_with_libbpf.c -lbpf -lelf -lz

# Transfer your .so file to the Linux machine
scp target/bpfel-unknown-none/release/libstake_ebpf_check.so user@linux-host:~/

# Run verification (requires sudo for BPF syscall)
sudo ./verify_with_libbpf libstake_ebpf_check.so
```
