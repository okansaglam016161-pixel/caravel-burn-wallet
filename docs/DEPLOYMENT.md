# Deployment

How the Caravel Burn Wallet was published on esmeralda, the first burn, and how to build and test
it, including against the exact on-chain binary.

See also: [USING.md](USING.md) for using it from an app, and [SECURITY.md](../SECURITY.md) for why
nothing can leave and how to verify it.

Published on **esmeralda** on 2026-10-08, on Ootle 0.45.

## The template

| | |
|---|---|
| Template | `template_f6bb3aa676b41c9d5748509dda66406e759d1b7fba5a83017cda88006b610282` |
| Transaction | `55ec0b7852546e34b7f588ea9021e47489126eb889d1469fd3adc3874dc1bdfd`, Commit / Accept, epoch 12007 |
| Fee | 728,683 µT (0.728683 tTARI) |
| Author | `20db90bffb62905d14b75369de9de8523d859bdec5f75e36929fbf9099781661` (the deployer) |
| Source | commit [`6009072`](https://github.com/okansaglam016161-pixel/caravel-burn-wallet/commit/6009072531bfb7a94c6d7e719e5c03f5e7f94a98) |
| Metadata hash | `1220d0bf4a3adc0ae9b1b92dfbb60d9a5c3cc555224ac78621a29a89c403fe83f47e` |
| On-chain binary | 98,367 bytes, sha256 `75aed53ca643fd55e1e2f25bc4c5fb84c4f5f19d2aec1ee546f6e6cc7af86a5e` (identical on both public indexers) |

## The burn wallet

| | |
|---|---|
| Component | `component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029` ([explorer](https://explorer.tari.mw/substate/component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029)) |
| Transaction | `578c07030139ce6c46c9521e257eec7accebc3a7345f59ae921254bfd25eced5`, Commit / Accept, epoch 12008 |
| Fee | 1,775 µT |
| Vault | `vault_2ea68bbb2339d1d930e35f011692c9e5dcdbf26a65008414ad3901c84d6055d0` (TARI) |

Created with [`scripts/instantiate.py`](../scripts/instantiate.py), which dry-runs `new()` before
submitting it and requires a final Accept. Read live on both indexers after creation: owner rule
`None`; method rules `deposit`, `balance`, `total_deposited` = `AllowAll`, default `DenyAll`;
template address as above; vault empty and unfrozen.

The live pentest run against it the same day is in
[SECURITY.md](../SECURITY.md#live-pentest-dry-runs-only-nothing-spent).

## First burn

On 2026-10-08 the deployer locked **1,000 tTARI** in the burn wallet, in two transactions:

| | Transaction | What | Fee |
|---|---|---|---|
| 1 | `3f06a9b6fe670956871581dd784237050c47d28ace0876fc61b347696b7b9f50` | The deployer made 1,000 tTARI of its confidential balance public, into its own account (a `StealthTransfer` revealing 1,000 tTARI, sent from the wallet daemon's UI) | 9,583 µT |
| 2 | `c528fef567a5a1f0fd678985f83a733eddffe48cbc85d89346f0683927300153` | `account.withdraw(TARI, 1,000 tTARI)` → `deposit(bucket)` on the burn wallet, in one transaction | 2,363 µT |

Both transactions are Commit / Accept on both public indexers, epoch 12015. Transaction 2 was
dry-run first. It emits exactly one `CaravelBurnWallet.Deposit` event, from this component, with
`amount` = `1000000000`. Read on both indexers afterwards:

* The vault holds 1,000,000,000 µT revealed (`balance()` = 1,000 tTARI), with 0 locked.
* The component state's `total_deposited` is 1,000,000,000 µT. The component is at version 1, so
  this deposit is the only change since `new()`.
* The deployer's balances add up. Confidential went down by 1,000,009,583 µT (the 1,000 tTARI plus
  transaction 1's fee). Public went from 2,684,087 µT, up 1,000,000,000 µT, then down
  1,000,002,363 µT, ending at 2,681,724 µT.

## Build and test

```sh
cd template
cargo test
cargo build --release --target wasm32-unknown-unknown
```

Tooling: `tari_template_lib` 0.34, `tari_ootle_template_build` 0.14, `tari_template_test_tooling` 0.45.

## Test against the on-chain binary

The on-chain binary is the release WASM after the optimisation pass `tari publish` applies, so it
is not byte-identical to `cargo build` output (112,785 bytes). Its ABI is exactly `new()`,
`deposit(&mut self, bucket)`, `balance(&self)` and `total_deposited(&self)`, and the full test suite
passes against it. Download the binary and run:

```sh
cd template
BURN_WALLET_WASM=/path/to/onchain.wasm cargo test -- --test-threads=1
```

The binary is the `binary` field of `GET /substates/template_f6bb…0282` on either indexer.

Part of [Caravel](https://github.com/okansaglam016161-pixel/caravel), alongside
[Caravel Lotto](https://github.com/okansaglam016161-pixel/caravel-lotto).
