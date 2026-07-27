# Bybak

Solana Verifiable Buyback Standard. Revenue returns like a homing pigeon to the
loft — every buyback recorded on-chain, verified against its ring number.

Most protocol buybacks are opaque manual processes. Multisig transfers, off-chain
execution, no independent verification. Bybak turns that into a standard: TWAP
acquisition, destination routing, and per-execution attestation, all on Solana.

## Concept

- **Buyback Engine** — TWAP acquisition through Jupiter routing, destination router (burn / LP / stakers), scheduled execution
- **Buyback Registry** — cross-protocol on-chain feed of every registered buyback
- **Proof-of-Buyback** — attestation for each execution: input revenue, average acquisition price, destination amounts

The registered protocol's buyback is not a promise. It is a homing return, recorded.

## Repositories

- [`bybak`](https://github.com/bybak-labs/bybak) — this repository. Standard specification, program interfaces, integration notes.

## How it works

1. A registered protocol declares a revenue source and destination policy.
2. Revenue accrues into a Bybak vault on Solana.
3. The TWAP executor acquires the protocol's token through Jupiter over a scheduled window.
4. The destination router splits the acquired supply between burn, LP re-injection, and staker distribution per the declared policy.
5. Each execution writes an on-chain attestation. Anyone can query the registry.

## Status

Standard draft. The front-end preview is live at https://bybak.fi. Program
deployment and registry rollout are next.

## Links

- Site: https://bybak.fi
- X: https://x.com/bybak_fi
