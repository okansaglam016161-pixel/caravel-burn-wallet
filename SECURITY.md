# Security

How the Caravel Burn Wallet keeps TARI locked, how to check it yourself, and everything that was
tried against it.

See also: [docs/USING.md](docs/USING.md) for using it from an app, and
[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for the published addresses and how to build and test.

## Report a problem, and the rug bounty

**10,000 $XTM** to anyone who moves any tTARI out of the burn wallet, or takes control of it.
Please DM us first, before you post anything: details in [the bounty post on X](LINK_TO_X_POST).

## Rules, and what enforces them

* **No owner.** The component is created with `OwnerRule::None`. The engine's default would make
  the deployer the owner, so it is replaced explicitly. An owner rule of `None` is satisfied by no
  caller, so nobody (the deployer included) can bypass the method rules, change the access rules,
  or set an owner later.
* **Deny by default.** Only `deposit`, `balance` and `total_deposited` are callable.
* **Nothing can leave.** The template has no code path that moves TARI out. A vault can only be
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
[DEPLOYMENT.md](docs/DEPLOYMENT.md#test-against-the-on-chain-binary)).

And read TARI itself (`GET /substates/resource_0101010101010101010101010101010101010101010101010101010101010101`):
recall, freeze, burn and mint are `DenyAll` and their updaters are `Locked`.

## Red team

The template was red-teamed before publish: by its author, and by an independent reviewer that read
only the contract and the Ootle 0.45 engine source. Neither found any way, for the deployer or
anyone else, to take, freeze or recall the TARI, to become the owner, or to change the rules. Every
path ends at one of three engine guarantees: only a component's own code can touch its vault, an
owner rule of `None` is satisfied by nobody, and TARI's recall, freeze, burn and mint rules are
`DenyAll` and locked.

Two limits are inherent rather than fixable: anyone can create more burn wallets from the same
template or publish a look-alike (so pin the address above), and, like any shared component,
concurrent deposits contend for it (which can delay a deposit but cannot take or lock funds).

After the first burn, the live component was red-teamed again by three independent reviewers working
from the Ootle 0.45 and `development` engine source, with live dry runs against the deployed
component (never a real attack transaction). They found no new way to take, freeze, mint or
double-count the TARI, or to change the code, owner or rules.

### The attacks are tests

All of them are in [`template/tests`](template/tests).

Before publish:

* `burn_wallet.rs`: deposits and their events; non-TARI and empty deposits refused; the template's
  exact surface; no owner and deny-by-default; the deployer and a stranger trying withdraw-like and
  admin-like methods.
* `red_team.rs`: the code swap (refused here, and shown to work against an owned vault); withdrawing,
  paying fees, recalling or freezing from another template; storing the vault in another component;
  changing TARI's rules; look-alike tokens and NFTs; spoofed `Deposit` events.

After the first burn:

* `red_team_auth.rs`: `UpdateComponentTemplate` with a migration body, swapping to the same template,
  seizing the component from outside via engine ops, method-name tricks, and nested cross-template
  calls. None can escalate against an `OwnerRule::None` component.
* `red_team_vault.rs`: naming the burn wallet as a fee payer, debiting its vault with a native
  `pay_fee` or a proof-lock from another component, a confidential withdraw from outside, minting or
  burning TARI, and changing TARI's (locked) access rules.
* `red_team_runtime.rs`: that a dry run cannot bypass ownership, a deposit rolls back cleanly on a
  later abort, a workspace bucket cannot be deposited twice, bucket splits conserve value, an
  instruction flood does not corrupt state, and the vault cannot be driven as a foreign input.

## Live pentest (dry runs only, nothing spent)

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
any transaction without a fee, so a second account, also not the deployer, paid the fee. The
deployer's key was in none of the stranger transactions. Each run included a control call
(`balance()`) that was accepted, so the refusals are the burn wallet's, not a broken transaction.
TARI's own rules were re-read on both indexers the same day: no owner, no auth hook; recall,
freeze, burn and mint `DenyAll`, deposit `AllowAll`, every one of those updaters `Locked`.
