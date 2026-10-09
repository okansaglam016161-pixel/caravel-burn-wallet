# Using the Caravel Burn Wallet

Everything here targets the official burn wallet,
`component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029`, on esmeralda.
Amounts are in µTARI (1 TARI = 1,000,000).

See also: [SECURITY.md](../SECURITY.md) for why nothing can leave, and
[DEPLOYMENT.md](DEPLOYMENT.md) for the published addresses and how to build and test.

## What it does

| Method | Who | What |
|---|---|---|
| `new()` | anyone, once per wallet | Creates an empty TARI vault with **no owner** |
| `deposit(bucket)` | anyone | Locks the TARI in the bucket. Refuses any other resource and empty buckets. Emits a `Deposit` event with the amount |
| `balance()` | anyone | TARI held, in µTARI |
| `total_deposited()` | anyone | Every µTARI ever deposited (always equal to `balance()`, since nothing leaves) |

That is the whole template. There is no withdraw, recall, burn, transfer, setter or admin method.

## Rules for using it

* **Only `deposit()` counts.** TARI reaches the burn wallet only through `deposit(bucket)`. TARI
  sent any other way (for example, a stealth transfer to a key or address associated with it) is not
  in the vault and is not counted by `balance()` or `total_deposited()`.
* **Empty deposits are refused.** A deposit of 0 fails the whole transaction it is in, so a caller
  that might pass an empty bucket (a contract forwarding a share, for example) must skip the call
  when the amount is zero.
* **Filter `Deposit` events by component address, never by topic.** An event's topic is
  `CaravelBurnWallet.Deposit`, and any template named `CaravelBurnWallet` can emit that exact topic.
  What the engine sets, and nobody can forge, is the emitting **component address** and **template
  address**. A dashboard or deposit history must count only events whose component is the official
  burn wallet's address above.

Each burn below is dry-run first (free: nothing is spent) to learn its exact fee, and is submitted
only at that fee.

## From a web or JS app

[`examples/js/burn.ts`](../examples/js/burn.ts) is one file to copy into your app. It uses
`@tari-project/ootle`, `@tari-project/ootle-indexer` and `@tari-project/ootle-secret-key-wallet`,
all 0.8.0, and type-checks against them (`cd examples/js && npm install && npm run typecheck`).

```ts
import { burnFromPrivate, burnFromPublic, connect, listDeposits, readBurnWallet } from './burn.ts'

const provider = await connect() // https://ootle-indexer-a.tari.com

// From the public balance: account.pay_fee(fee), account.withdraw(TARI, amount) → deposit(bucket).
const { fee, transactionId } = await burnFromPublic(provider, wallet, account, 5_000_000n) // 5 TARI

// From private funds: spend these stealth UTXOs, reveal amount + fee into a bucket, deposit the
// amount, pay the fee from the rest. The change comes back to you privately. No account is needed,
// and the owner key neither signs nor receives: only the UTXOs' one-time keys sign.
await burnFromPrivate(provider, wallet, ['utxo_0101…_<commitment>'], 5_000_000n)

// { submit: false } stops after the dry runs and returns the exact fee: show it, then burn.
const quote = await burnFromPublic(provider, wallet, account, 5_000_000n, { submit: false })
```

`wallet` is a `SecretKeyWallet` holding the owner key, and `account` is its account component. Each
burn builds the transaction, dry-runs it to price it, rebuilds it paying exactly that fee, dry-runs
that, and only then submits it. A dry run must come back as a clean `Accept`. An
`AcceptFeeRejectRest` means the fee would be taken and the burn would not happen, so it is an
error. (The SDK's `sendDryRun` posts to `/transactions`, which the indexer refuses for dry runs, so
`burn.ts` posts to `/transactions/dry-run` itself.)

## From the terminal (wallet daemon JSON-RPC)

[`examples/walletd/burn.sh`](../examples/walletd/burn.sh) burns from the daemon's default account
with `transactions.submit_manifest`:

```sh
export TARI_WALLET_DAEMON_API_KEY=…   # never on the command line
examples/walletd/burn.sh 1000000            # dry run: prints the exact fee and the Deposit event
examples/walletd/burn.sh 1000000 --submit   # dry run, then submit at exactly that fee
```

The manifest it submits:

```rust
fn main() {
    let mut account = global!["account"];          // the default account's component
    let burn_wallet = global!["burn_wallet"];      // component_2e91…6029
    let bucket = account.withdraw(global!["tari"], global!["amount"]);
    burn_wallet.deposit(bucket);
}
```

## From another contract

Call `deposit(bucket)` on the burn wallet from your template. For example, a shop that burns 2% of
every sale:

```rust
pub fn buy(&mut self, mut payment: Bucket) {
    assert!(payment.resource_address() == TARI_TOKEN, "Pay in TARI");
    assert!(payment.amount() == self.price, "Pay exactly the price");

    let burn = Amount::new(self.price.to_u128() * 2 / 100);
    // The burn wallet refuses an empty deposit, which would fail the whole sale.
    if burn.is_positive() {
        // self.burn_wallet: ComponentAddress = component_2e91…6029
        ComponentManager::get(self.burn_wallet).invoke("deposit", args![payment.take(burn)]);
    }
    self.till.deposit(payment);
}
```

The whole template is [`template/tests/templates/shop`](../template/tests/templates/shop/src/lib.rs),
a test-only template that is never published. [`template/tests/use_from_contract.rs`](../template/tests/use_from_contract.rs)
runs it against the burn wallet: a 50 TARI sale puts 1 TARI in the burn wallet and 49 in the till,
the one `Deposit` event comes from the burn wallet's component, and a sale too small to burn still
succeeds. Declare the burn wallet and its vault (`vault_2ea68bbb…55d0`) as inputs of a transaction that
calls your template, as `burn.ts` does, or the deposit can fail with `SubstateNotFound`.

## Reading the balance and the deposits

Read the component and its vault from any indexer (`GET /substates/<id>`), and **only trust a read
with `verified: true`**. Just after an epoch change an indexer can serve a value no committee member
has proved yet, with `verified: false`. It may be stale, so retry or ask the other indexer rather
than show it. `readBurnWallet` in `burn.ts` does this, and also checks the template address and
that the owner rule is `None`:

```ts
const { balance, totalDeposited } = await readBurnWallet(provider) // throws on an unverified read
```

For the deposit history, query the event index **by the component address**:
`GET /transactions/events?substate_id=component_2e91…6029`, paging with `before_id`. Keep events
whose `substate_id` is the burn wallet and whose topic is `CaravelBurnWallet.Deposit`. The amount is
`payload.amount`. Never filter by topic alone: any template named `CaravelBurnWallet` can emit that
topic. `listDeposits` in `burn.ts` does this.

## Proof: dry runs against the live burn wallet

Run on 2026-10-09 against the live component. Every transaction was a dry run and nothing was spent.

| Example | Result |
|---|---|
| `burnFromPublic`, 1 TARI, test wallet's account | `Accept` at the exact fee, 2,370 µT |
| `burnFromPrivate`, 1 TARI, from two of the test wallet's UTXOs, signed only by their one-time keys | `Accept` at the exact fee, 10,302 µT |
| `burn.sh 1000000`, deployer's wallet daemon | `Accept` at the exact fee, 2,392 µT. One `Deposit` event from the burn wallet, `amount` = `1000000` |
| `readBurnWallet` | 1,000,000,000 µT, `verified: true`, total deposited 1,000,000,000 µT |
| `listDeposits` | 1 deposit, 1,000,000,000 µT in `c528fef5…` |

The burn wallet's balance was read again afterwards and had not moved. Run the TS proof yourself
with your own wallet (`examples/js/prove.ts` explains the variables; keys come from the environment
only).

A dry run checks a transaction's shape and fee, but not every authorization check, so a green dry
run alone does not show that someone else could do the same.

The shop is the one example not dry-run live: a template has to be published before a transaction
can call it, and publishing costs a fee. Instead, it passes against the exact on-chain burn wallet
binary (`BURN_WALLET_WASM=… cargo test --test use_from_contract`, see
[DEPLOYMENT.md](DEPLOYMENT.md#test-against-the-on-chain-binary)), and the method it calls is the
same `deposit(bucket)` the three live dry runs call. (A deposit made from inside another component
is also covered by `red_team_auth.rs`'s nested-call test, see [SECURITY.md](../SECURITY.md#the-attacks-are-tests).)
