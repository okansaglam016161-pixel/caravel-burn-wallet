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

The tests in [`template/tests`](template/tests) check each of these, including attempts by the
deployer and by strangers to call withdraw-like and admin-like methods, and attacks on the vault
from another template.

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
