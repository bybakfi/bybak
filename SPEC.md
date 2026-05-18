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
