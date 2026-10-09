# Caravel Burn Wallet

A TARI wallet you can only deposit into: nothing can ever be withdrawn, and it has no owner, not even Caravel.

> **Testnet only.** Live on the Tari Ootle esmeralda testnet.

## Live on Esmeralda

The official burn wallet is
[`component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029`](https://explorer.tari.mw/substate/component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029).
Pin this address: anyone can create look-alikes from the same template.

## Burn from your browser

Open [caravellabs.net](https://caravellabs.net), go to **Burn** in the left menu, and burn from your private or public balance.

## Use it in your app

From JS ([`examples/js/burn.ts`](examples/js/burn.ts), `@tari-project/ootle` 0.8):

```ts
const provider = await connect()
await burnFromPublic(provider, wallet, account, 5_000_000n)   // 5 TARI, exact fee
await burnFromPrivate(provider, wallet, utxoIds, 5_000_000n)  // signed only by one-time keys
```

From the terminal ([`examples/walletd/burn.sh`](examples/walletd/burn.sh), wallet daemon JSON-RPC):

```sh
examples/walletd/burn.sh 1000000 --submit
```

From another contract:

```rust
ComponentManager::get(burn_wallet).invoke("deposit", args![bucket]);
```

The full guide, with reading the balance and the deposit history, is in [docs/USING.md](docs/USING.md).

## Verify it yourself

Read the component from any indexer (`GET /substates/<component>`) and check:

1. The owner rule is `None`.
2. Only `deposit`, `balance` and `total_deposited` are allowed, and every other method is `DenyAll`.
3. The template is `template_f6bb3aa6…0282`, built from commit [`6009072`](https://github.com/okansaglam016161-pixel/caravel-burn-wallet/commit/6009072531bfb7a94c6d7e719e5c03f5e7f94a98).

The full checklist is in [SECURITY.md](SECURITY.md#verify-it-yourself).

## Security

It was red-teamed before and after launch, and nobody found a way to take, freeze or recall the TARI, or to become the owner.
How it holds, what was tried, and how to report a problem: [SECURITY.md](SECURITY.md).

How it was published and how to build and test it: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

Part of [Caravel](https://github.com/okansaglam016161-pixel/caravel).

## License

MIT, see [LICENSE](LICENSE).
