# Buyback Standard

Most protocol buybacks are opaque manual processes. Multisig transfers, off-chain
execution, no independent verification. The idea here is to turn that into a
standard: TWAP acquisition, destination routing, and per-execution attestation,
all on Solana.

## Concept

- **Buyback Engine** — TWAP acquisition through Jupiter routing, destination router (burn / LP / stakers), scheduled execution
- **Buyback Registry** — cross-protocol on-chain feed of every registered buyback
- **Proof-of-Buyback** — attestation for each execution: input revenue, average acquisition price, destination amounts

A registered protocol's buyback is not a promise. It is a recorded return.

## How it works

1. A registered protocol declares a revenue source and destination policy.
2. Revenue accrues into a vault on Solana.
3. The TWAP executor acquires the protocol's token through Jupiter over a scheduled window.
4. The destination router splits the acquired supply between burn, LP re-injection, and staker distribution per the declared policy.
5. Each execution writes an on-chain attestation. Anyone can query the registry.
