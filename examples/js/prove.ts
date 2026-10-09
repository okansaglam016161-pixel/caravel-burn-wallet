// Prove the examples in burn.ts against the LIVE burn wallet, with dry runs only: nothing is spent.
//
//   OWNER_SECRET_HEX=…  VIEW_SECRET_HEX=…    the wallet's keys (environment only)
//   ACCOUNT=component_…                      its account, for the public burn
//   UTXO_IDS=utxo_…,utxo_…                   private outputs it owns, for the private burn
//   node prove.ts
//
// Each burn is built, dry-run to price it, rebuilt at exactly that fee and dry-run again. It is
// never submitted ({ submit: false }).

import { Network } from '@tari-project/ootle'
import { SecretKeyWallet } from '@tari-project/ootle-secret-key-wallet'
import { burnFromPrivate, burnFromPublic, connect, listDeposits, readBurnWallet } from './burn.ts'

const env = (name: string) => {
  const v = process.env[name]?.trim()
  if (!v) throw new Error(`${name} is not set`)
  return v
}
const bytes = (hex: string) => Uint8Array.from(hex.match(/../g)!, (b) => parseInt(b, 16))

const wallet = SecretKeyWallet.fromSecretKey(bytes(env('OWNER_SECRET_HEX')), Network.Esmeralda, bytes(env('VIEW_SECRET_HEX')))
const provider = await connect()
const ONE_TARI = 1_000_000n

const before = await readBurnWallet(provider)
console.log(`burn wallet     ${before.balance} µTARI locked (verified), total deposited ${before.totalDeposited}, vault ${before.vault}`)

const deposits = await listDeposits()
console.log(`deposits        ${deposits.length}: ${deposits.map((d) => `${d.amount} µTARI in ${d.transactionId.slice(0, 12)}…`).join(', ')}`)

const pub = await burnFromPublic(provider, wallet, env('ACCOUNT'), ONE_TARI, { submit: false })
console.log(`public burn     1 TARI: dry run Accept at the exact fee, ${pub.fee} µTARI (not submitted)`)

const priv = await burnFromPrivate(provider, wallet, env('UTXO_IDS').split(','), ONE_TARI, { submit: false })
console.log(`private burn    1 TARI: dry run Accept at the exact fee, ${priv.fee} µTARI (not submitted)`)

const after = await readBurnWallet(provider)
if (after.balance !== before.balance) throw new Error('the burn wallet balance moved during a dry-run-only proof')
console.log(`burn wallet     unchanged: ${after.balance} µTARI`)
