# Caravel Burn Wallet

> **Testnet only. Work in progress.** Not yet published to any network.

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
| Template address | _to be filled at publish_ |
| Component address | _to be filled at publish_ |

Read the component from any indexer (`GET /substates/<component address>`) and check:

1. **Owner rule is `None`.**
2. **Method rules:** `deposit`, `balance` and `total_deposited` are `AllowAll`, and the default for
   every other method is `DenyAll`. Nothing else is listed.
3. **Template address** is the one above.
4. The template's code is the source in this repository at the published commit
   (`template/src/lib.rs`).

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
