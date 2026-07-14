# Buyback Standard (draft)

This document describes the interfaces a Solana program must implement to be
considered a compliant buyback source.

## 1. Buyback trigger

```
trigger_buyback(
  input_amount: u64,
  destination: Destination,
  schedule: Schedule,
) -> BuybackId
```

`Destination` is one of `Burn`, `Liquidity`, `Stakers`, or a weighted composition.
`Schedule` is one of `Immediate`, `Twap { window_seconds, slice_count }`.

## 2. Attestation

Every completed buyback emits an `Attestation` account containing:

- `buyback_id: BuybackId`
- `protocol: Pubkey`
- `input_amount_lamports: u64`
- `acquired_token: Pubkey`
- `acquired_amount: u64`
- `average_price_lamports_per_token: u64`
- `destination_split: DestinationSplit`
- `completed_at_slot: u64`

The registry indexes attestations by protocol and by time.

## 3. Compliance

A protocol is compliant when the above interfaces are implemented and its
executions consistently emit attestations that verify against on-chain state.

The specification, execution engine, and registry are versioned together.
