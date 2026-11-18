# Turbine AMM

A Constant Product Automated Market Maker (CPMM) built on Solana using the Anchor framework, featuring protocol fees, a dedicated treasury account, slippage protection, and an in-memory integration test suite powered by [LiteSVM](https://github.com/LiteSVM/litesvm).

---

## Table of Contents

- [Overview](#overview)
- [Architecture & Program Accounts](#architecture--program-accounts)
  - [State Design (`Config`)](#state-design-config)
  - [PDA Derivations](#pda-derivations)
- [Instructions](#instructions)
  - [1. Initialize](#1-initialize)
  - [2. Deposit Liquidity](#2-deposit-liquidity)
  - [3. Remove Liquidity](#3-remove-liquidity)
  - [4. Swap](#4-swap)
- [Fee Structure & Treasury Collection](#fee-structure--treasury-collection)
- [Mitigating Downtime in DeFi Applications](#mitigating-downtime-in-defi-applications)
- [Testing Suite (LiteSVM)](#testing-suite-litesvm)
- [Building & Running Tests](#building--running-tests)

---

## Overview

Turbine AMM implements the classic constant product curve formula ($x \cdot y = k$) on Solana. It enables permissionless decentralized trading between pairs of SPL tokens, dynamic liquidity provisioning with proportional LP token shares, slippage tolerance boundaries, and protocol fee extraction into a designated treasury.

---

## Architecture & Program Accounts

### State Design (`Config`)

The core configuration account holds pool metadata and bump seeds for PDA signing:

```rust
#[account]
#[derive(InitSpace)]
pub struct Config {
    pub seed: u64,                 // Unique seed identifying the pool instance
    pub authority: Option<Pubkey>, // Optional authority for admin controls (e.g., locking)
    pub mint_x: Pubkey,            // Mint address of Token X
    pub mint_y: Pubkey,            // Mint address of Token Y
    pub fee: u16,                  // Total swap fee in basis points (e.g., 30 = 0.30%)
    pub locked: bool,              // Emergency pool lock state
    pub config_bump: u8,           // PDA bump for Config
    pub lp_bump: u8,               // PDA bump for LP Mint
    pub treasury: Pubkey,          // Protocol treasury address receiving fees
}
```

### PDA Derivations

The protocol uses Program Derived Addresses (PDAs) to ensure deterministic authority and trustless custody:

1. **Config Account PDA**:
   - Seeds: `[b"config", seed.to_le_bytes().as_ref()]`
   - Role: Stores pool configuration and acts as the signing authority for token vaults and LP minting.
2. **LP Token Mint PDA**:
   - Seeds: `[b"lp", config.key().as_ref()]`
   - Authority: `config` PDA (allows minting on deposit and burning on withdrawal).
3. **Vault X & Vault Y**:
   - Associated Token Accounts (ATAs) of `mint_x` and `mint_y` owned by the `config` PDA.

---

## Instructions

### 1. `initialize`
Initializes a new AMM liquidity pool:
- Creates the `config` PDA account.
- Creates the `mint_lp` token mint with 6 decimals, with `config` set as the mint authority.
- Initializes `vault_x` and `vault_y` ATAs owned by `config`.
- Sets swap fee in basis points and registers the `treasury` address.

### 2. `deposit`
Allows Liquidity Providers (LPs) to deposit Token X and Token Y into the pool vaults:
- **First Deposit**: Deposits `max_x` and `max_y` and mints initial LP tokens equal to `amount`.
- **Subsequent Deposits**: Uses the constant product curve invariant to calculate exact proportional amounts of X and Y required for the desired LP tokens.
- **Slippage Protection**: Reverts with `AmmError::SlippageExceeded` if required deposits exceed `max_x` or `max_y`.

### 3. `remove_liquidity`
Allows LPs to redeem their LP tokens for their proportionate share of the pool reserves:
- Calculates proportional amounts of Token X and Token Y from current reserves and total LP supply.
- **Slippage Protection**: Enforces that received amounts satisfy `x >= min_x` and `y >= min_y`.
- Burns the user's LP tokens via SPL Token Program CPI.
- Transfers Token X and Token Y from the vaults to the user using the `config` PDA signer seeds.

### 4. `swap`
Swaps Token X for Token Y (or Token Y for Token X):
- Enforces invariant preservation: $(x + \Delta x)(y - \Delta y) \ge k$.
- Supports bidirectional trading via the `is_x` boolean flag (`true` = X $\rightarrow$ Y, `false` = Y $\rightarrow$ X).
- Calculates output amount and fee using the constant product formula.
- Enforces slippage bounds: received output tokens must meet or exceed `min_out`.
- Transfers input tokens from the user to the vault.
- Transfers output tokens from the vault to the user via PDA signer CPI.
- Splits fee: routes 50% directly to the protocol `treasury_token` account and retains 50% in the pool to reward LPs.

---

## Fee Structure & Treasury Collection

The protocol implements a balanced fee-splitting mechanism:
1. **Total Fee**: Defined in basis points on the `Config` account (e.g., 30 bps = 0.30%).
2. **50% Protocol Treasury Cut**: 50% of the calculated swap fee is transferred directly into the treasury's Associated Token Account (`treasury_token`) during the swap.
3. **50% Liquidity Provider Cut**: The remaining 50% stays inside the pool reserves, permanently increasing the invariant $k$. This increases the redemption value of all circulating LP tokens.

---

## Mitigating Downtime in DeFi Applications

High volatility events and network congestion can severely impact DeFi protocols. Below is an architectural blueprint for mitigating downtime and operational risk on Solana:

### 1. RPC Redundancy & Intelligent Routing
- **Multiple RPC Endpoints**: Frontends and bots should never rely on a single public RPC node. Implement client-side round-robin, health-checking, and automatic failover across dedicated providers (e.g., Helius, Triton, QuickNode).
- **Websocket Resiliency**: Auto-reconnecting websocket listeners with exponential backoff and fallback to periodic polling.

### 2. Congestion Management & Priority Fees
- **Dynamic Priority Fees**: Implement dynamic fee estimation based on recent prioritization fee percentiles (`getRecentPrioritizationFees`) to ensure transactions land during periods of network contention.
- **Stake-Weighted Quality of Service (SWQoS)**: Route critical protocol operations (e.g., rebalancing, keeper sweeps) through RPC nodes with staked validator connections to bypass congestion queues.

### 3. Oracle Freshness & Staleness Circuit Breakers
- For AMMs with external price feeds or lending markets:
  - Enforce strict slot/timestamp staleness bounds (e.g., reject oracle updates older than 30–60 seconds).
  - Implement circuit breakers that automatically halt trading if price divergence between external oracles and internal pool spot price exceeds safe thresholds.

### 4. Admin Circuit Breakers & Emergency Locks
- The `Config` account includes a `locked: bool` flag. The designated `authority` can immediately pause deposits, withdrawals, or swaps in the event of an exploit, abnormal market desynchronization, or underlying token vulnerability.

### 5. Idempotent & Fault-Tolerant Keeper Systems
- Liquidator and keeper bots should be distributed across multiple distinct cloud regions and autonomous keypairs to ensure protocol maintenance continues uninterrupted even if local infrastructure goes down.

---

## Testing Suite (LiteSVM)

The test suite is built on **LiteSVM**, an in-process, high-performance Solana virtual machine test environment that executes compiled SBF bytecode directly without requiring a local validator process.

### Test Coverage

| Test Name | Type | Description |
| :--- | :--- | :--- |
| `test_initialize` | Happy Path | Initializes pool, derives PDAs, creates vaults and LP mint. |
| `test_deposit` | Happy Path | Deposits initial reserves into pool and mints LP tokens to user. |
| `test_remove_liquidity` | Happy Path | Burns LP tokens and returns proportional Token X and Token Y reserves. |
| `test_swap` | Happy Path | Swaps Token X for Token Y, verifies slippage tolerance, and routes protocol fee to treasury. |
| `test_swap_slippage_exceeded` | Edge / Security | Verifies swap is rejected when `min_out` exceeds available output. |
| `test_swap_zero_amount` | Edge / Security | Verifies swap rejects zero input amounts (`InvalidAmount`). |
| `test_remove_liquidity_slippage_exceeded` | Edge / Security | Verifies withdrawal fails when `min_x` or `min_y` exceeds proportional share. |
| `test_remove_liquidity_zero_amount` | Edge / Security | Verifies withdrawal rejects zero LP burn amounts. |
| `test_remove_liquidity_insufficient_lp` | Edge / Security | Verifies user cannot burn more LP tokens than they hold. |
| `test_deposit_zero_amount` | Edge / Security | Verifies deposit rejects zero LP mint amounts. |
| `test_deposit_slippage_exceeded` | Edge / Security | Verifies deposit fails when required deposit exceeds `max_x` / `max_y`. |

---

## Building & Running Tests

### Prerequisites
- [Rust](https://www.rust-lang.org/) (v1.80+)
- [Solana CLI](https://docs.solanalabs.com/cli/install) (v2.0+)
- [Anchor CLI](https://www.anchor-lang.com/docs/installation) (v0.30+)

### Build Program
```bash
anchor build
```

### Run Test Suite
```bash
cargo test
```
All tests execute via LiteSVM against the compiled SBF binary (`target/deploy/ammq32026.so`).
