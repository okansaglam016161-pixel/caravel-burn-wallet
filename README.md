# Caravel Burn Wallet

> **Testnet only.** Live on the Tari Ootle **esmeralda** testnet (see [Live deployment](#live-deployment)).

A Tari Ootle template for a **deposit-only** TARI wallet with **no owner**. TARI sent here is
locked forever. The balance and every deposit are public.

## What it does

| Method | Who | What |
|---|---|---|
| `new()` | anyone, once per wallet | Creates an empty TARI vault with **no owner** |
| `deposit(bucket)` | anyone | Locks the TARI in the bucket. Refuses any other resource and empty buckets. Emits a `Deposit` event with the amount |
| `balance()` | anyone | TARI held, in µTARI |
| `total_deposited()` | anyone | Every µTARI ever deposited (always equal to `balance()`, since nothing leaves) |

That is the whole template. There is no withdraw, recall, burn, transfer, setter or admin method.

## Rules, and what enforces them

- **No owner.** The component is created with `OwnerRule::None`. The engine's default would make
  the deployer the owner; it is replaced explicitly. An owner rule of `None` is satisfied by no
  caller, so nobody (the deployer included) can bypass the method rules, change the access rules,
  or set an owner later.
- **Deny by default.** Only `deposit`, `balance` and `total_deposited` are callable.
- **Nothing can leave.** The template has no code path that moves TARI out. A vault can only be
  withdrawn from by its own component's code, and TARI's recall rule is `DenyAll` and locked, so no
  other template and no resource owner can pull funds out either.

### Why `OwnerRule::None` must never be removed

Ootle has a transaction instruction, `UpdateComponentTemplate`, that makes an existing component
run **different code**: the component keeps its state (including the vault) but a new template
runs it. The engine allows it only for the component's **owner**.

With the default owner rule, the deployer would be that owner, and could swap the burn wallet onto
a template with a `withdraw` method and empty it in a single transaction. The tests show exactly
this happening to an otherwise identical vault that keeps the default owner. With
`OwnerRule::None` there is no owner, so the swap is refused for everyone.

That one line (`.with_owner_rule(OwnerRule::None)` in `new()`) is what makes this a burn wallet.
Removing it, or replacing it with any other owner rule, turns it back into a wallet its owner can
drain.

## Rules for using it

- **Only `deposit()` counts.** TARI reaches the burn wallet only through `deposit(bucket)`. TARI
  sent any other way (for example, a stealth transfer to a key or address associated with it) is not
  in the vault and is not counted by `balance()` or `total_deposited()`.
- **Empty deposits are refused.** A deposit of 0 fails the whole transaction it is in, so a caller
  that might pass an empty bucket (a contract forwarding a share, for example) must skip the call
  when the amount is zero.
- **Filter `Deposit` events by component address, never by topic.** An event's topic is
  `CaravelBurnWallet.Deposit`, and any template named `CaravelBurnWallet` can emit that exact topic.
  What the engine sets, and nobody can forge, is the emitting **component address** and **template
  address**. A dashboard or deposit history must count only events whose component is the official
  burn wallet's address below.

## Verify it yourself

| | |
|---|---|
| Template address | `template_f6bb3aa676b41c9d5748509dda66406e759d1b7fba5a83017cda88006b610282` |
| Component address (the official burn wallet) | `component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029` |

Read the component from any indexer (`GET /substates/<component address>`) and check:

1. **Owner rule is `None`.**
2. **Method rules:** `deposit`, `balance` and `total_deposited` are `AllowAll`, and the default for
   every other method is `DenyAll`. Nothing else is listed.
3. **Template address** is the one above.
4. The template's code is the source in this repository at the published commit
   ([`6009072`](https://github.com/okansaglam016161-pixel/caravel-burn-wallet/commit/6009072531bfb7a94c6d7e719e5c03f5e7f94a98),
   `template/src/lib.rs`), which the template's metadata names as its `commit_hash`.

You can also run this repository's whole test suite against the exact on-chain binary (see
[Live deployment](#live-deployment)).

And read TARI itself (`GET /substates/resource_0101010101010101010101010101010101010101010101010101010101010101`):
recall, freeze, burn and mint are `DenyAll` and their updaters are `Locked`.

## Security

The template was red-teamed before publish: by its author, and by an independent reviewer that read
only the contract and the Ootle 0.45 engine source. Neither found any way, for the deployer or
anyone else, to take, freeze or recall the TARI, to become the owner, or to change the rules. Every
path ends at one of three engine guarantees: only a component's own code can touch its vault; an
owner rule of `None` is satisfied by nobody; and TARI's recall, freeze, burn and mint rules are
`DenyAll` and locked.

Two limits are inherent rather than fixable: anyone can create more burn wallets from the same
template or publish a look-alike (so pin the address above), and, like any shared component,
concurrent deposits contend for it (which can delay a deposit but cannot take or lock funds).

The attacks are tests, in [`template/tests`](template/tests):

- `burn_wallet.rs`: deposits and their events; non-TARI and empty deposits refused; the template's
  exact surface; no owner and deny-by-default; the deployer and a stranger trying withdraw-like and
  admin-like methods.
- `red_team.rs`: the code swap (refused here, and shown to work against an owned vault); withdrawing,
  paying fees, recalling or freezing from another template; storing the vault in another component;
  changing TARI's rules; look-alike tokens and NFTs; spoofed `Deposit` events.

After the first burn, the live component was red-teamed again — three independent reviewers working
from the Ootle 0.45 and `development` engine source, with live dry runs against the deployed
component (never a real attack transaction). They found no new way to take, freeze, mint or
double-count the TARI, or to change the code, owner or rules. The new attacks are also tests:

- `red_team_auth.rs`: `UpdateComponentTemplate` with a migration body, swapping to the same template,
  seizing the component from outside via engine ops, method-name tricks, and nested cross-template
  calls — none can escalate against an `OwnerRule::None` component.
- `red_team_vault.rs`: naming the burn wallet as a fee payer, debiting its vault with a native
  `pay_fee` or a proof-lock from another component, a confidential withdraw from outside, minting or
  burning TARI, and changing TARI's (locked) access rules.
- `red_team_runtime.rs`: that a dry run cannot bypass ownership, a deposit rolls back cleanly on a
  later abort, a workspace bucket cannot be deposited twice, bucket splits conserve value, an
  instruction flood does not corrupt state, and the vault cannot be driven as a foreign input.

## Live deployment

Published on **esmeralda** on 2026-10-08, on Ootle 0.45.

### The template

| | |
|---|---|
| Template | `template_f6bb3aa676b41c9d5748509dda66406e759d1b7fba5a83017cda88006b610282` |
| Transaction | `55ec0b7852546e34b7f588ea9021e47489126eb889d1469fd3adc3874dc1bdfd`, Commit / Accept, epoch 12007 |
| Fee | 728,683 µT (0.728683 tTARI) |
| Author | `20db90bffb62905d14b75369de9de8523d859bdec5f75e36929fbf9099781661` (the deployer) |
| Source | commit [`6009072`](https://github.com/okansaglam016161-pixel/caravel-burn-wallet/commit/6009072531bfb7a94c6d7e719e5c03f5e7f94a98) |
| Metadata hash | `1220d0bf4a3adc0ae9b1b92dfbb60d9a5c3cc555224ac78621a29a89c403fe83f47e` |
| On-chain binary | 98,367 bytes, sha256 `75aed53ca643fd55e1e2f25bc4c5fb84c4f5f19d2aec1ee546f6e6cc7af86a5e` (identical on both public indexers) |

The on-chain binary is the release WASM after the optimisation pass `tari publish` applies, so it
is not byte-identical to `cargo build` output (112,785 bytes). Its ABI is exactly `new()`,
`deposit(&mut self, bucket)`, `balance(&self)` and `total_deposited(&self)`, and the full test suite
passes against it: download the binary and run

```sh
cd template
BURN_WALLET_WASM=/path/to/onchain.wasm cargo test -- --test-threads=1
```

(the binary is the `binary` field of `GET /substates/template_f6bb…0282` on either indexer).

### The burn wallet

| | |
|---|---|
| Component | `component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029` |
| Transaction | `578c07030139ce6c46c9521e257eec7accebc3a7345f59ae921254bfd25eced5`, Commit / Accept, epoch 12008 |
| Fee | 1,775 µT |
| Vault | `vault_2ea68bbb2339d1d930e35f011692c9e5dcdbf26a65008414ad3901c84d6055d0` (TARI) |

Created with [`scripts/instantiate.py`](scripts/instantiate.py), which dry-runs `new()` before
submitting it and requires a final Accept. Read live on both indexers after creation: owner rule
`None`; method rules `deposit`, `balance`, `total_deposited` = `AllowAll`, default `DenyAll`;
template address as above; vault empty and unfrozen.

### Live pentest (dry runs only, nothing spent)

Run against the live component on 2026-10-08, every transaction a dry run:

| As | Attempt | Result |
|---|---|---|
| Deployer (the key that published and created it) | `UpdateComponentTemplate` onto another template | Refused: "You must be the owner to perform this action: native.component.update_template" |
| Deployer | 14 withdraw-like and admin-like methods (`withdraw`, `withdraw_all`, `take`, `take_all`, `recall`, `burn`, `transfer`, `send`, `pay_fee`, `set_access_rules`, `set_owner_rule`, `set_owner`, `add_owner`, `upgrade`), with and without an amount, keeping any output | All 28 refused: "Function … not found" |
| A fresh stranger key | The same code swap, and the same 28 calls | All refused, with the same reasons |
| A stranger | Deposit 1 µT; deposit 1 tTARI | Accepted, one `Deposit` event each, from this component |
| A stranger | Two deposits (2 µT, 3 µT) in one transaction | Accepted, two `Deposit` events |
| A stranger | Deposit 0 | Refused: "Deposit must be greater than zero" |
| A stranger | Deposit a non-TARI NFT (minted in the same transaction from the built-in NFT faucet) | Refused: "Only TARI can be deposited into the burn wallet" |

The fresh stranger key signed first, which makes it the transaction's signer. The network refuses
any transaction without a fee, so a second account, also not the deployer, paid the fee; the
deployer's key was in none of the stranger transactions. Each run included a control call
(`balance()`) that was accepted, so the refusals are the burn wallet's, not a broken transaction. TARI's own rules were re-read on both indexers the same day:
no owner, no auth hook; recall, freeze, burn and mint `DenyAll`, deposit `AllowAll`, every one of
those updaters `Locked`.

### First burn

On 2026-10-08 the deployer locked **1,000 tTARI** in the burn wallet, in two transactions:

| | Transaction | What | Fee |
|---|---|---|---|
| 1 | `3f06a9b6fe670956871581dd784237050c47d28ace0876fc61b347696b7b9f50` | The deployer made 1,000 tTARI of its confidential balance public, into its own account (a `StealthTransfer` revealing 1,000 tTARI, sent from the wallet daemon's UI) | 9,583 µT |
| 2 | `c528fef567a5a1f0fd678985f83a733eddffe48cbc85d89346f0683927300153` | `account.withdraw(TARI, 1,000 tTARI)` → `deposit(bucket)` on the burn wallet, in one transaction | 2,363 µT |

Both transactions are Commit / Accept on both public indexers, epoch 12015. Transaction 2 was
dry-run first. It emits exactly one `CaravelBurnWallet.Deposit` event, from this component, with
`amount` = `1000000000`. Read on both indexers afterwards:

- The vault holds 1,000,000,000 µT revealed (`balance()` = 1,000 tTARI), with 0 locked.
- The component state's `total_deposited` is 1,000,000,000 µT. The component is at version 1, so
  this deposit is the only change since `new()`.
- The deployer's balances add up. Confidential went down by 1,000,009,583 µT (the 1,000 tTARI plus
  transaction 1's fee). Public went from 2,684,087 µT, up 1,000,000,000 µT, then down
  1,000,002,363 µT, ending at 2,681,724 µT.

## Build and test

```sh
cd template
cargo test
cargo build --release --target wasm32-unknown-unknown
```

Tooling: `tari_template_lib` 0.34, `tari_ootle_template_build` 0.14, `tari_template_test_tooling` 0.45.

Part of [Caravel](https://github.com/okansaglam016161-pixel/caravel), alongside
[Caravel Lotto](https://github.com/okansaglam016161-pixel/caravel-lotto).

## License

MIT, see [LICENSE](LICENSE).
