# Ethereum Transactions for an MPC Threshold Wallet (Rust)

If you already have **threshold ECDSA on secp256k1** working for Bitcoin, you’re *very* close for Ethereum: Ethereum transactions are also signed with **secp256k1 ECDSA**, but the **transaction model, fields, encoding, and hashing** are different.

Below is the mental model + the exact “build → hash → threshold-sign → serialize → broadcast” pipeline you’ll implement.

---

## 1) Bitcoin vs Ethereum: what changes for a wallet

### UTXO vs Account model
- **Bitcoin (UTXO):** you spend specific outputs; you choose inputs; you create change outputs; fee is implicit (`sum(inputs) - sum(outputs)`).
- **Ethereum (account/state):** each address has a **balance** and a **nonce**. A transaction says “from this account, send/call X”, and the chain updates global state.  
  - No input selection.  
  - No change outputs.  
  - You must track the **nonce** and pay **gas**.

### “Transaction id”
- Bitcoin txid is `double-SHA256(serialized_tx)`.
- Ethereum tx hash is `keccak256(raw_signed_tx_bytes)` (Keccak-256, not SHA-256).

---

## 2) Ethereum accounts and addresses (EOA)
An externally-owned account (EOA) is controlled by a secp256k1 keypair.

### Address derivation
1. Get the **uncompressed** public key: `0x04 || X(32) || Y(32)`
2. Compute `keccak256(X||Y)` (drop the `0x04`)
3. Address = **last 20 bytes** of that hash

(So your threshold wallet ultimately controls an EOA address.)

---

## 3) What an Ethereum transaction actually is

A transaction is a signed message that moves ETH and/or calls EVM code. Core fields you’ll see:

- `chainId` – prevents replay across chains (mainnet vs testnet, etc.)
- `nonce` – per-sender counter (must match sender’s next nonce)
- `to` – 20-byte address (empty / null for contract creation)
- `value` – amount of ETH in wei
- `data` – calldata / initcode (empty for plain ETH transfer)
- `gasLimit` – max gas you allow this tx to consume
- fee fields – depends on tx type (legacy vs EIP-1559, etc.)
- signature – `(v/yParity, r, s)` depending on type

Ethereum has multiple **transaction types** (typed envelope system).

### Transaction types you should care about (Jan 2026 reality)
- **Type 0 (legacy):** `gasPrice` model (older)
- **Type 1 (EIP-2930):** adds `accessList` (rare for basic wallets)
- **Type 2 (EIP-1559):** modern default (maxFee / maxPriorityFee)
- **Type 3 (EIP-4844 blob):** mainly for rollups, not needed for a basic ETH wallet
- **Type 4 (EIP-7702):** “set code” / auth-list (newer AA-adjacent), not needed initially

If your goal is “send ETH / call contracts / ERC-20 transfers”, implement **Type 2 first**.

---

## 4) Gas + fees (the part that replaces “sats/vbyte” thinking)

### Gas
- Every EVM operation costs gas.
- You set `gasLimit`.  
  - Plain ETH transfer: typically **21,000 gas**
  - Contract calls vary widely.

### EIP-1559 fees (Type 2)
Ethereum blocks have a **base fee** (protocol-determined) and you add a **tip** to incentivize inclusion.

You specify:
- `maxPriorityFeePerGas` = tip cap
- `maxFeePerGas` = total cap (baseFee + tip)

Effective gas price paid is roughly:
- `effectiveGasPrice = min(maxFeePerGas, baseFeePerGas + maxPriorityFeePerGas)`

Base fee is burned; tip goes to validator.

Total cost to sender:
- `total = value + gasUsed * effectiveGasPrice`

---

## 5) Signing: the exact bytes you threshold-sign

This is where most Ethereum wallet implementations fail: **the “signing hash” depends on transaction type and encoding**.

Ethereum uses:
- **RLP encoding** for payloads
- **Typed transaction envelope**: `typeByte || rlp(payload)` for typed txs
- Hash = `keccak256(...)`

### Type 2 (EIP-1559) — what you’ll implement first

**Signing payload (without signature fields):**
```text
[chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList]
```

**Signing digest:**
```text
keccak256( 0x02 || rlp(payload_without_sig) )
```

**Final encoded signed tx:**
```text
0x02 || rlp([chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList, yParity, r, s])
```

### Signature rules you must enforce
Ethereum requires **low-s** signatures (anti-malleability): `s <= secp256k1n/2`.

Also note:
- In typed txs (type 1/2/3/4), you typically store **`yParity` (0 or 1)** instead of legacy `v=27/28`.
- Your threshold signer must output `(r, s)` **and** recovery info (`yParity` / recid). If your MPC gives you recid, map it to yParity.

(For legacy type 0 with EIP-155, `v` includes chain id; but if you start with type 2, you avoid that complexity.)

---

## 6) Broadcasting + lifecycle (JSON-RPC view)

Your wallet library usually needs a “provider” interface that can call Ethereum JSON-RPC.

Common calls:
- `eth_chainId`
- `eth_getTransactionCount(address, "pending")` → nonce
- `eth_estimateGas(tx)` → gas limit estimate
- `eth_feeHistory` and/or `eth_maxPriorityFeePerGas` → fee suggestion
- `eth_sendRawTransaction(0x...)` → broadcast signed bytes
- `eth_getTransactionReceipt(txHash)` → confirmation, status, logs

Receipt contains:
- `status` (success/fail)
- `gasUsed`
- `logs` (events emitted by contracts)

---

## 7) “Sending ETH” vs “Calling a contract” vs “ERC-20 transfer”

### A) Plain ETH transfer
- `to = recipient`
- `value = amount_in_wei`
- `data = 0x`
- `gasLimit ≈ 21000`

### B) Contract call (e.g., interact with a dApp)
- `to = contract_address`
- `value = maybe 0 or some ETH`
- `data = ABI-encoded function call`
- `gasLimit = estimated`

### C) ERC-20 transfer (most important contract call)
You don’t send tokens by setting `value`. You call the token contract’s `transfer(address,uint256)`:

- `to = token_contract`
- `value = 0`
- `data = 0xa9059cbb || abi_encode(to, amount)`
  - `0xa9059cbb` is the 4-byte selector for `transfer(address,uint256)`.

So once you can:
1) ABI-encode calldata  
2) Build a type-2 tx  
3) Sign it with threshold ECDSA  

…you can do ETH + ERC-20.

---

## 8) Concrete implementation plan for your Rust MPC wallet

### Core modules you’ll want
1) **primitives**
   - `U256`, `Address`, `Bytes`, `ChainId`
2) **crypto**
   - secp256k1 pubkey recovery + address derivation (keccak)
   - low-s normalization/check
3) **rlp + typed-tx encoding**
   - legacy (optional)
   - type 2 first (EIP-1559)
4) **tx builder**
   - fill nonce, chainId, fees, gasLimit, to, value, data
5) **mpc signer adapter**
   - input: `digest32`
   - output: `(r, s, yParity)` (or `(r,s,recid)`)
6) **provider trait**
   - JSON-RPC calls for nonce/fees/estimate/broadcast

### The “happy path” function you’ll implement
`sign_and_serialize_eip1559(tx_unsigned) -> (raw_tx_bytes, tx_hash)`

Steps:
1. Build payload (no sig)
2. `digest = keccak256(0x02 || rlp(payload))`
3. Threshold-sign digest → `(r,s,yParity)`
4. Enforce low-s (and adjust yParity if your signer doesn’t already normalize)
5. Serialize signed tx
6. `tx_hash = keccak256(raw_signed_tx_bytes)`

---

## 9) Minimal Type-2 example (values are illustrative)

Say you’re sending 0.01 ETH:
- `value = 0.01 * 10^18 wei`
- `gasLimit = 21000`
- `maxPriorityFeePerGas = 2 gwei`
- `maxFeePerGas = 50 gwei`
- `nonce = eth_getTransactionCount(from, "pending")`
- `chainId = eth_chainId`

Unsigned payload:
```text
[chainId, nonce, 2 gwei, 50 gwei, 21000, to, value, 0x, []]
```

Sign `keccak256(0x02 || rlp(payload))`, append `(yParity, r, s)`.

---

## 10) Recommended Rust crates

| Crate | Purpose |
|-------|---------|
| `alloy` | Modern Ethereum library (types, RLP, providers, transactions) — recommended |
| `alloy-primitives` | `U256`, `Address`, `Bytes`, `FixedBytes` |
| `alloy-rlp` | RLP encoding/decoding |
| `alloy-consensus` | Transaction types (Type 0/1/2) |
| `alloy-network` | Network abstractions |
| `alloy-provider` | JSON-RPC provider |
| `tiny-keccak` | Keccak-256 hashing |

The `alloy` ecosystem is the successor to `ethers-rs` and is actively maintained.

---

## 11) HD derivation for Ethereum

Standard BIP-44 path for Ethereum:
```
m/44'/60'/0'/0/x
```
- `44'` = BIP-44
- `60'` = Ethereum coin type
- `0'` = account
- `0` = external chain
- `x` = address index

Your MPC keygen can derive the group public key for each path, then compute the Ethereum address from that.

---

## 12) Checksum addresses (EIP-55)

Ethereum addresses are case-insensitive, but EIP-55 defines a checksum encoding using mixed case:
- Hash the lowercase address with keccak256
- For each hex character, if the corresponding nibble in the hash >= 8, uppercase it

Example: `0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed`

Always display checksum addresses to users for safety.

---

## 13) Test networks for development

| Network | Chain ID | Notes |
|---------|----------|-------|
| Sepolia | 11155111 | Primary testnet, use for testing |
| Holesky | 17000 | Validator/staking testnet |

Faucets:
- Sepolia: https://sepoliafaucet.com, https://faucet.sepolia.dev
- Use Alchemy/Infura for RPC endpoints

---
