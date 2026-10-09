// Burn TARI into the official Caravel Burn Wallet from a JS/TS app, and read what it holds.
//
// Copy this file into your app. It needs @tari-project/ootle, @tari-project/ootle-indexer and
// @tari-project/ootle-secret-key-wallet, all 0.8.0. Every amount is in µTARI (1 TARI = 1,000,000).
//
// Every burn goes the same way: build the transaction, dry-run it (free, nothing is spent) to learn
// its exact fee, rebuild it paying exactly that fee, dry-run that, and only then submit. Pass
// `{ submit: false }` to stop after the dry runs.

import {
  Network,
  OotleWallet,
  StealthInput,
  StealthTransferStatement,
  TARI_RESOURCE_ADDRESS,
  TransactionBuilder,
  WasmStealthCrypto,
  amountLiteral,
  createOutput,
  decryptOwnedUtxo,
  generateSealKeypair,
  getVaultIdsForAccount,
  iterVaultIdsInState,
  microTariString,
  parseSubstateUtxo,
  resolveMaxEpoch,
  resolveTransaction,
  resourceAddressLiteral,
  sealTransaction,
  serializeUnsignedTx,
  signBalanceProof,
  signTransaction,
  stealthTransferInstruction,
  stealthUtxoSubstateId,
  type Signer,
  type TransactionEnvelope,
  type UnsignedTransactionV1,
} from '@tari-project/ootle'
import { IndexerProvider } from '@tari-project/ootle-indexer'
import type { SecretKeyWallet } from '@tari-project/ootle-secret-key-wallet'

/** The official burn wallet. Pin this address: anyone can create look-alikes from the same template. */
export const BURN_WALLET = 'component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029'
export const BURN_WALLET_TEMPLATE = 'f6bb3aa676b41c9d5748509dda66406e759d1b7fba5a83017cda88006b610282'
export const INDEXER_URL = 'https://ootle-indexer-a.tari.com'

/** The fee the PRICING dry run offers. A dry run charges nothing; it only has to be enough to finish. */
const PROBE_FEE = 100_000n

export interface BurnResult {
  /** The exact fee, in µTARI, from the dry run (paid on top of the amount burned). */
  fee: bigint
  /** Set when the burn was submitted and committed. */
  transactionId?: string
}

export function connect(url = INDEXER_URL): Promise<IndexerProvider> {
  return IndexerProvider.connect({ url, network: Network.Esmeralda })
}

// ── 1. Burn from the public balance (the account vault) ──────────────────────

/**
 * Burn `amount` µTARI from `account`'s public balance. `wallet` holds the account's owner key.
 *
 *   account.pay_fee(fee)                    the fee, from the same vault
 *   account.withdraw(TARI, amount)          → bucket
 *   burn_wallet.deposit(bucket)             locked forever
 */
export async function burnFromPublic(
  provider: IndexerProvider,
  wallet: SecretKeyWallet,
  account: string,
  amount: bigint,
  { submit = true } = {},
): Promise<BurnResult> {
  // Everything the transaction touches must be declared: your account and its vaults, and the burn
  // wallet and its vault (a deposit into an undeclared vault fails with SubstateNotFound).
  const inputs = [
    account, ...await getVaultIdsForAccount(provider, account),
    BURN_WALLET, ...await getVaultIdsForAccount(provider, BURN_WALLET),
  ]
  const maxEpoch = await resolveMaxEpoch(provider)

  const build = (fee: bigint) =>
    new TransactionBuilder(Network.Esmeralda, maxEpoch)
      .withFeeInstructionsBuilder((b) =>
        b
          .callMethod({ componentAddress: account, methodName: 'pay_fee' }, [amountLiteral(fee)])
          .callMethod({ componentAddress: account, methodName: 'withdraw' }, [
            resourceAddressLiteral(TARI_RESOURCE_ADDRESS),
            amountLiteral(amount),
          ])
          .saveVar('burn')
          .callMethod({ componentAddress: BURN_WALLET, methodName: 'deposit' }, [{ Workspace: 'burn' }]),
      )
      .withInputs(inputs.map((substate_id) => ({ substate_id, version: null })))
      .buildUnsignedTransaction()

  const sign = async (tx: UnsignedTransactionV1, dryRun: boolean) => {
    const resolved = await resolveTransaction(provider, tx)
    return sealTransaction(await signTransaction([wallet], { ...resolved, dry_run: dryRun }))
  }

  return dryRunThenSubmit(provider, build, sign, submit)
}

// ── 2. Burn from private funds (stealth UTXOs) ───────────────────────────────

/**
 * Burn `amount` µTARI from private funds. No account is needed.
 *
 * `utxoIds` are stealth UTXOs this wallet owns (`utxo_…` substate ids, which your wallet already
 * tracks). They must add up to at least `amount` plus the fee; the rest comes back as a new private
 * output to `wallet`.
 *
 *   StealthTransfer: spend the UTXOs, reveal `amount + fee` → bucket, change → you, privately
 *   TakeFromBucket(bucket, amount)          → the burn
 *   burn_wallet.deposit(the burn)           locked forever
 *   PayFeeFromBucket(bucket)                what is left in the bucket, which is exactly the fee
 */
export async function burnFromPrivate(
  provider: IndexerProvider,
  wallet: SecretKeyWallet,
  utxoIds: string[],
  amount: bigint,
  { submit = true } = {},
): Promise<BurnResult> {
  const crypto = new WasmStealthCrypto(Network.Esmeralda)
  const myAddress = await wallet.getAddress()
  const myPublicKey = await wallet.getPublicKey()
  const viewSecret = await wallet.getViewSecret()

  // Open each UTXO: its value and mask (from the view key) and the sender nonce (to sign for it).
  const utxos = await Promise.all(utxoIds.map(async (id) => {
    const substate = await provider.getSubstate(id)
    const parsed = parseSubstateUtxo(substate, id)
    const opened = parsed && await decryptOwnedUtxo(crypto, viewSecret, substate, id)
    if (!parsed || !opened) throw new Error(`${id} is spent, frozen, or not this wallet's`)
    return { commitment: parsed.commitment, nonce: hexToBytes(parsed.body.public_nonce), mask: opened.mask, value: opened.value }
  }))
  const total = utxos.reduce((sum, u) => sum + u.value, 0n)

  const burnWalletInputs = [BURN_WALLET, ...await getVaultIdsForAccount(provider, BURN_WALLET)]
  const maxEpoch = await resolveMaxEpoch(provider)
  const sealKeypair = generateSealKeypair()

  // Built per fee: the statement commits to `amount + fee`, and the change is what is left.
  const build = async (fee: bigint) => {
    const revealed = amount + fee
    const change = total - revealed
    if (change < 0n) throw new Error(`These UTXOs hold ${total} µTARI; the burn needs ${revealed} with the fee`)

    const outputs = change > 0n
      ? [createOutput({ destination: myAddress, amount: change, resourceAddress: TARI_RESOURCE_ADDRESS })]
      : []
    // The revealed output's receiver must be a signer of this transaction: you.
    const { statement: outs, outputMask } = await crypto.generateOutputsStatement(outputs, { amount: revealed, receiver: myPublicKey })
    const ins = await crypto.buildInputsStatement(utxos.map((u) => new StealthInput(u.commitment)), 0n)
    const inputMask = await crypto.aggregateInputMasks(utxos.map((u) => u.mask))
    const proof = await signBalanceProof(crypto, inputMask, outputMask, ins, outs)
    const statement = new StealthTransferStatement(ins, outs, proof)

    return new TransactionBuilder(Network.Esmeralda, maxEpoch)
      .withFeeInstructionsBuilder((b) => {
        b.addInstruction(stealthTransferInstruction(
          { resourceAddress: TARI_RESOURCE_ADDRESS, revealedInputBucket: null, statement },
          (name) => b.resolveWorkspaceOffsetId(name),
        )).saveVar('bucket')
        // TakeFromBucket writes to a numbered slot, not a named one. 'bucket' is slot 0, so the
        // burn goes in slot 1.
        const bucket = b.resolveWorkspaceOffsetId('bucket')
        if (bucket.id !== 0) throw new Error('unexpected workspace layout')
        const burn = { id: 1, offset: null }
        b.addInstruction({ TakeFromBucket: { input_bucket: bucket, amount: microTariString(amount), output_bucket: burn.id } })
        b.callMethod({ componentAddress: BURN_WALLET, methodName: 'deposit' }, [{ Workspace: burn }])
        b.addInstruction({ PayFeeFromBucket: { bucket } })
        return b
      })
      .withInputs([
        ...burnWalletInputs.map((substate_id) => ({ substate_id, version: null })),
        ...utxos.map((u) => ({ substate_id: stealthUtxoSubstateId(TARI_RESOURCE_ADDRESS, u.commitment), version: null })),
      ])
      .buildUnsignedTransaction()
  }

  const sign = async (tx: UnsignedTransactionV1, dryRun: boolean) => {
    const resolved = await resolveTransaction(provider, tx)
    // Each UTXO is spent with a one-time key derived from its sender nonce, signed over the
    // transaction WITHOUT the dry-run flag, plus your own key for the revealed output.
    const unsignedJson = serializeUnsignedTx(resolved)
    const oneTime = []
    for (const u of utxos) {
      oneTime.push(await wallet.addStealthSignature(unsignedJson, u.nonce, sealKeypair.public_key, { crypto }))
    }
    const me = new OotleWallet().registerKeyProvider(myAddress, wallet).setDefaultSigner(myAddress)
    const signed = await signTransaction([me, fixedSignatures(oneTime)], { ...resolved, dry_run: dryRun }, sealKeypair)
    return sealTransaction(signed)
  }

  return dryRunThenSubmit(provider, build, sign, submit)
}

// ── 3. Read the burn wallet (verified reads only) ────────────────────────────

export interface BurnWalletState {
  /** TARI locked in the vault, µTARI. */
  balance: bigint
  /** Every µTARI ever deposited (equal to `balance`: nothing leaves). */
  totalDeposited: bigint
  vault: string
}

/**
 * Read the burn wallet's balance from an indexer, trusting only VERIFIED reads.
 *
 * Just after an epoch change an indexer can serve a value no committee member has proved yet, with
 * `verified: false`. That value may be stale, so it is refused here rather than shown: retry, or
 * ask the other indexer.
 */
export async function readBurnWallet(provider: IndexerProvider): Promise<BurnWalletState> {
  const component = await provider.getSubstate(BURN_WALLET)
  if (component.verified !== true) throw new Error('Unverified read of the burn wallet; try again')
  if (!('Component' in component.substate)) throw new Error(`${BURN_WALLET} is not a component`)
  const { header, body } = component.substate.Component
  // The checks that make it the burn wallet, not a look-alike: our template, and no owner.
  if (header.template_address !== BURN_WALLET_TEMPLATE) throw new Error('Not the Caravel Burn Wallet template')
  if (header.owner_rule !== 'None') throw new Error('The burn wallet must have no owner')

  const [vaultHex] = [...iterVaultIdsInState(body.state)]
  const vault = vaultHex.startsWith('vault_') ? vaultHex : `vault_${vaultHex}`
  const vaultRead = await provider.getSubstate(vault)
  if (vaultRead.verified !== true) throw new Error('Unverified read of the burn wallet vault; try again')
  if (!('Vault' in vaultRead.substate)) throw new Error(`${vault} is not a vault`)
  const container = vaultRead.substate.Vault.resource_container
  if (!('Stealth' in container)) throw new Error('Unexpected vault type')

  const [, totalDeposited] = body.state as [unknown, number | string]
  return { balance: BigInt(container.Stealth.revealed_amount), totalDeposited: BigInt(totalDeposited), vault }
}

// ── 4. Deposit history ────────────────────────────────────────────────────────

export interface Deposit {
  transactionId: string
  amount: bigint
}

/**
 * Every deposit, newest first, from the indexer's event index.
 *
 * Filtered by the burn wallet's COMPONENT ADDRESS, never by topic: any template named
 * CaravelBurnWallet can emit a `CaravelBurnWallet.Deposit` topic, but only the engine sets an
 * event's component and template, and nobody can forge them.
 */
export async function listDeposits(url = INDEXER_URL): Promise<Deposit[]> {
  const deposits: Deposit[] = []
  let before: string | undefined
  for (;;) {
    const query = new URLSearchParams({ substate_id: BURN_WALLET, limit: '100' })
    if (before) query.set('before_id', before)
    const res = await fetch(`${url}/transactions/events?${query}`)
    if (!res.ok) throw new Error(`events: HTTP ${res.status}`)
    const page = await res.json() as {
      events: [string, { substate_id: string | null; template_address: string; topic: string; payload: Record<string, string> }][]
      next_before_id?: number | string | null
    }
    for (const [transactionId, e] of page.events) {
      if (e.substate_id !== BURN_WALLET || e.template_address !== BURN_WALLET_TEMPLATE) continue
      if (e.topic !== 'CaravelBurnWallet.Deposit') continue // also std.component.created / updated
      deposits.push({ transactionId, amount: BigInt(e.payload.amount) })
    }
    if (page.next_before_id == null) return deposits
    before = String(page.next_before_id)
  }
}

// ── Dry run → exact fee → submit ─────────────────────────────────────────────

/**
 * Dry-run a sealed transaction (built with `dry_run: true`) and return the fee it requires.
 *
 * The SDK's `sendDryRun` posts to `/transactions`, which the indexer refuses for dry runs, so this
 * posts to `/transactions/dry-run`. Anything but a clean `Accept` is an error: an
 * `AcceptFeeRejectRest` means the fee would be taken and the burn would not happen.
 */
export async function dryRun(envelope: TransactionEnvelope, url = INDEXER_URL): Promise<bigint> {
  const res = await fetch(`${url}/transactions/dry-run`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ transaction: envelope }),
  })
  if (!res.ok) throw new Error(`dry run: HTTP ${res.status} ${await res.text()}`)
  const { result } = await res.json() as {
    result: { finalize: { result: Record<string, unknown>; total_fees_required: number | string } }
  }
  const verdict = result.finalize.result
  if (!('Accept' in verdict)) throw new Error(`dry run did not Accept: ${JSON.stringify(verdict).slice(0, 400)}`)
  return BigInt(result.finalize.total_fees_required)
}

async function dryRunThenSubmit(
  provider: IndexerProvider,
  build: (fee: bigint) => UnsignedTransactionV1 | Promise<UnsignedTransactionV1>,
  sign: (tx: UnsignedTransactionV1, dryRun: boolean) => Promise<TransactionEnvelope>,
  submit: boolean,
): Promise<BurnResult> {
  // Price it, then check the transaction that pays exactly that fee is accepted at that fee.
  const fee = await dryRun(await sign(await build(PROBE_FEE), true))
  const exact = await build(fee)
  const required = await dryRun(await sign(exact, true))
  if (required > fee) throw new Error(`The fee rose from ${fee} to ${required} µTARI; try again`)
  if (!submit) return { fee }

  const { transaction_id } = await provider.submitTransaction(await sign(exact, false))
  await provider.watchTransactionSSE(transaction_id).watch() // throws unless it committed
  provider.stopWatcher()
  return { fee, transactionId: transaction_id }
}

/** Hands pre-made signatures (the one-time UTXO spend signatures) to `signTransaction`. */
function fixedSignatures(signatures: Awaited<ReturnType<Signer['signTransaction']>>): Signer {
  return {
    getAddress: async () => '',
    getPublicKey: async () => new Uint8Array(32),
    signTransaction: async () => signatures,
  }
}

function hexToBytes(hex: string): Uint8Array {
  return Uint8Array.from(hex.match(/../g)!, (b) => parseInt(b, 16))
}
