//! Bybak -- Solana Verifiable Buyback Standard (MVP).
//!
//! A protocol registers once, then declares each buyback *before* any acquisition
//! happens (input size, destination, schedule). An off-chain executor performs the
//! acquisition across the declared schedule. On completion the protocol writes an
//! `Attestation` account that anyone can read and verify against on-chain state.
//!
//! The on-chain surface implements the interfaces described in SPEC.md:
//!   1. Buyback trigger  -- `open_buyback` (input_amount, destination, schedule) -> BuybackId
//!   2. Attestation      -- `close_buyback` writes the `Attestation` account
//!   3. Compliance       -- registry counters anyone can index by protocol and by time

use anchor_lang::prelude::*;

declare_id!("8n1BA3TB1tfYzU75GR9CDePXZEeoXXEYQVEs3QqwTRrj");

/// Version of the Bybak standard this program implements.
pub const BYBAK_SPEC_VERSION: u8 = 1;

/// Basis-point denominator for destination splits.
pub const BPS_DENOMINATOR: u16 = 10_000;

/// A TWAP schedule must span at least this many seconds, so that a "buyback"
/// cannot be a single-block print dressed up as an averaged acquisition.
pub const MIN_TWAP_WINDOW_SECONDS: u32 = 60;

/// Maximum length of a human-readable protocol name.
pub const MAX_PROTOCOL_NAME_LEN: usize = 32;

pub const REGISTRY_SEED: &[u8] = b"registry";
pub const BUYBACK_SEED: &[u8] = b"buyback";
pub const ATTESTATION_SEED: &[u8] = b"attestation";

#[program]
pub mod bybak {
    use super::*;

    /// A protocol declares itself Bybak-compliant. Creates a `ProtocolRegistry` PDA
    /// that all of that protocol's buybacks and attestations hang off of.
    pub fn register_protocol(ctx: Context<RegisterProtocol>, protocol_name: String) -> Result<()> {
        require!(!protocol_name.is_empty(), BybakError::NameEmpty);
        require!(
            protocol_name.len() <= MAX_PROTOCOL_NAME_LEN,
            BybakError::NameTooLong
        );

        let slot = Clock::get()?.slot;
        let registry = &mut ctx.accounts.registry;
        registry.protocol = ctx.accounts.protocol.key();
        registry.protocol_name = protocol_name;
        registry.spec_version = BYBAK_SPEC_VERSION;
        registry.buybacks_opened = 0;
        registry.buybacks_completed = 0;
        registry.total_input_lamports = 0;
        registry.total_acquired = 0;
        registry.created_at_slot = slot;
        registry.bump = ctx.bumps.registry;

        emit!(ProtocolRegistered {
            registry: registry.key(),
            protocol: registry.protocol,
            protocol_name: registry.protocol_name.clone(),
            created_at_slot: slot,
        });
        Ok(())
    }

    /// Open a buyback: declare the window and the destination *before* any
    /// acquisition happens. The `buyback_id` is chosen by the protocol and is unique
    /// per registry -- the PDA seeds enforce it, so a duplicate id fails to initialize.
    pub fn open_buyback(
        ctx: Context<OpenBuyback>,
        buyback_id: u64,
        input_amount_lamports: u64,
        destination: Destination,
        schedule: Schedule,
    ) -> Result<()> {
        require!(input_amount_lamports > 0, BybakError::ZeroInput);
        destination.validate()?;
        schedule.validate()?;

        let slot = Clock::get()?.slot;
        let registry_key = ctx.accounts.registry.key();

        let buyback = &mut ctx.accounts.buyback;
        buyback.registry = registry_key;
        buyback.protocol = ctx.accounts.protocol.key();
        buyback.buyback_id = buyback_id;
        buyback.input_amount_lamports = input_amount_lamports;
        buyback.acquired_token = ctx.accounts.acquired_token.key();
        buyback.destination = destination.clone();
        buyback.schedule = schedule.clone();
        buyback.opened_at_slot = slot;
        buyback.acquired_amount = 0;
        buyback.status = BuybackStatus::Open;
        buyback.bump = ctx.bumps.buyback;

        let buyback_key = buyback.key();
        let protocol_key = buyback.protocol;
        let acquired_token = buyback.acquired_token;

        let registry = &mut ctx.accounts.registry;
        registry.buybacks_opened = registry.buybacks_opened.saturating_add(1);
        registry.total_input_lamports = registry
            .total_input_lamports
            .checked_add(input_amount_lamports)
            .ok_or(BybakError::MathOverflow)?;

        emit!(BuybackOpened {
            buyback: buyback_key,
            registry: registry_key,
            protocol: protocol_key,
            buyback_id,
            input_amount_lamports,
            acquired_token,
            destination,
            schedule,
            opened_at_slot: slot,
        });
        Ok(())
    }

    /// Close a buyback: attest completion with the acquired amount and the average
    /// price paid. Writes the `Attestation` account described in SPEC.md section 2.
    pub fn close_buyback(
        ctx: Context<CloseBuyback>,
        acquired_amount: u64,
        average_price_lamports_per_token: u64,
    ) -> Result<()> {
        require!(
            ctx.accounts.buyback.status == BuybackStatus::Open,
            BybakError::AlreadyClosed
        );
        require!(acquired_amount > 0, BybakError::ZeroAcquired);
        require!(
            average_price_lamports_per_token > 0,
            BybakError::ZeroAveragePrice
        );

        let slot = Clock::get()?.slot;

        let buyback = &mut ctx.accounts.buyback;
        buyback.acquired_amount = acquired_amount;
        buyback.status = BuybackStatus::Closed;

        let split = buyback.destination.split();
        let buyback_key = buyback.key();
        let buyback_registry = buyback.registry;
        let buyback_protocol = buyback.protocol;
        let buyback_id = buyback.buyback_id;
        let input_amount_lamports = buyback.input_amount_lamports;
        let acquired_token = buyback.acquired_token;

        let attestation = &mut ctx.accounts.attestation;
        attestation.buyback = buyback_key;
        attestation.registry = buyback_registry;
        attestation.protocol = buyback_protocol;
        attestation.buyback_id = buyback_id;
        attestation.input_amount_lamports = input_amount_lamports;
        attestation.acquired_token = acquired_token;
        attestation.acquired_amount = acquired_amount;
        attestation.average_price_lamports_per_token = average_price_lamports_per_token;
        attestation.destination_split = split.clone();
        attestation.spec_version = BYBAK_SPEC_VERSION;
        attestation.completed_at_slot = slot;
        attestation.bump = ctx.bumps.attestation;

        let attestation_key = attestation.key();

        let registry = &mut ctx.accounts.registry;
        registry.buybacks_completed = registry.buybacks_completed.saturating_add(1);
        registry.total_acquired = registry
            .total_acquired
            .checked_add(acquired_amount)
            .ok_or(BybakError::MathOverflow)?;

        emit!(BuybackClosed {
            attestation: attestation_key,
            buyback: buyback_key,
            registry: buyback_registry,
            protocol: buyback_protocol,
            buyback_id,
            input_amount_lamports,
            acquired_token,
            acquired_amount,
            average_price_lamports_per_token,
            destination_split: split,
            completed_at_slot: slot,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Where the acquired tokens go. SPEC.md section 1.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug, InitSpace)]
pub enum Destination {
    Burn,
    Liquidity,
    Stakers,
    Weighted {
        burn_bps: u16,
        liquidity_bps: u16,
        stakers_bps: u16,
    },
}

impl Destination {
    /// Resolve to the canonical basis-point split recorded in the attestation.
    pub fn split(&self) -> DestinationSplit {
        match *self {
            Destination::Burn => DestinationSplit {
                burn_bps: BPS_DENOMINATOR,
                liquidity_bps: 0,
                stakers_bps: 0,
            },
            Destination::Liquidity => DestinationSplit {
                burn_bps: 0,
                liquidity_bps: BPS_DENOMINATOR,
                stakers_bps: 0,
            },
            Destination::Stakers => DestinationSplit {
                burn_bps: 0,
                liquidity_bps: 0,
                stakers_bps: BPS_DENOMINATOR,
            },
            Destination::Weighted {
                burn_bps,
                liquidity_bps,
                stakers_bps,
            } => DestinationSplit {
                burn_bps,
                liquidity_bps,
                stakers_bps,
            },
        }
    }

    /// A weighted composition must account for exactly 100% of the acquired amount.
    pub fn validate(&self) -> Result<()> {
        if let Destination::Weighted {
            burn_bps,
            liquidity_bps,
            stakers_bps,
        } = *self
        {
            let total = (burn_bps as u32)
                .checked_add(liquidity_bps as u32)
                .and_then(|v| v.checked_add(stakers_bps as u32))
                .ok_or(BybakError::MathOverflow)?;
            require!(
                total == BPS_DENOMINATOR as u32,
                BybakError::InvalidDestinationSplit
            );
        }
        Ok(())
    }
}

/// The canonical resolved split stored on the attestation. SPEC.md section 2.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug, InitSpace)]
pub struct DestinationSplit {
    pub burn_bps: u16,
    pub liquidity_bps: u16,
    pub stakers_bps: u16,
}

/// How the acquisition is spread over time. SPEC.md section 1.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug, InitSpace)]
pub enum Schedule {
    Immediate,
    Twap {
        window_seconds: u32,
        slice_count: u16,
    },
}

impl Schedule {
    pub fn validate(&self) -> Result<()> {
        if let Schedule::Twap {
            window_seconds,
            slice_count,
        } = *self
        {
            require!(slice_count > 0, BybakError::ZeroSlices);
            require!(
                window_seconds >= MIN_TWAP_WINDOW_SECONDS,
                BybakError::WindowTooShort
            );
        }
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum BuybackStatus {
    Open,
    Closed,
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct ProtocolRegistry {
    /// Authority that may open and close buybacks under this registry.
    pub protocol: Pubkey,
    #[max_len(MAX_PROTOCOL_NAME_LEN)]
    pub protocol_name: String,
    pub spec_version: u8,
    pub buybacks_opened: u64,
    pub buybacks_completed: u64,
    pub total_input_lamports: u64,
    pub total_acquired: u64,
    pub created_at_slot: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Buyback {
    pub registry: Pubkey,
    pub protocol: Pubkey,
    pub buyback_id: u64,
    pub input_amount_lamports: u64,
    pub acquired_token: Pubkey,
    pub destination: Destination,
    pub schedule: Schedule,
    pub opened_at_slot: u64,
    pub acquired_amount: u64,
    pub status: BuybackStatus,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Attestation {
    pub buyback: Pubkey,
    pub registry: Pubkey,
    pub protocol: Pubkey,
    pub buyback_id: u64,
    pub input_amount_lamports: u64,
    pub acquired_token: Pubkey,
    pub acquired_amount: u64,
    pub average_price_lamports_per_token: u64,
    pub destination_split: DestinationSplit,
    pub spec_version: u8,
    pub completed_at_slot: u64,
    pub bump: u8,
}

// ---------------------------------------------------------------------------
// Contexts
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct RegisterProtocol<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ProtocolRegistry::INIT_SPACE,
        seeds = [REGISTRY_SEED, protocol.key().as_ref()],
        bump
    )]
    pub registry: Account<'info, ProtocolRegistry>,
    /// The protocol authority. Must sign so nobody can squat another key's registry.
    pub protocol: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(buyback_id: u64)]
pub struct OpenBuyback<'info> {
    #[account(
        mut,
        has_one = protocol @ BybakError::UnauthorizedProtocol,
        seeds = [REGISTRY_SEED, protocol.key().as_ref()],
        bump = registry.bump,
    )]
    pub registry: Account<'info, ProtocolRegistry>,
    #[account(
        init,
        payer = payer,
        space = 8 + Buyback::INIT_SPACE,
        seeds = [BUYBACK_SEED, registry.key().as_ref(), &buyback_id.to_le_bytes()],
        bump
    )]
    pub buyback: Account<'info, Buyback>,
    /// CHECK: mint of the token being bought back. Recorded on the attestation for
    /// indexing only; this program never moves the token itself, so no ownership
    /// check is meaningful here.
    pub acquired_token: UncheckedAccount<'info>,
    pub protocol: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CloseBuyback<'info> {
    #[account(
        mut,
        has_one = protocol @ BybakError::UnauthorizedProtocol,
        seeds = [REGISTRY_SEED, protocol.key().as_ref()],
        bump = registry.bump,
    )]
    pub registry: Account<'info, ProtocolRegistry>,
    #[account(
        mut,
        has_one = registry @ BybakError::RegistryMismatch,
        has_one = protocol @ BybakError::UnauthorizedProtocol,
    )]
    pub buyback: Account<'info, Buyback>,
    #[account(
        init,
        payer = payer,
        space = 8 + Attestation::INIT_SPACE,
        seeds = [ATTESTATION_SEED, buyback.key().as_ref()],
        bump
    )]
    pub attestation: Account<'info, Attestation>,
    pub protocol: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[event]
pub struct ProtocolRegistered {
    pub registry: Pubkey,
    pub protocol: Pubkey,
    pub protocol_name: String,
    pub created_at_slot: u64,
}

#[event]
pub struct BuybackOpened {
    pub buyback: Pubkey,
    pub registry: Pubkey,
    pub protocol: Pubkey,
    pub buyback_id: u64,
    pub input_amount_lamports: u64,
    pub acquired_token: Pubkey,
    pub destination: Destination,
    pub schedule: Schedule,
    pub opened_at_slot: u64,
}

#[event]
pub struct BuybackClosed {
    pub attestation: Pubkey,
    pub buyback: Pubkey,
    pub registry: Pubkey,
    pub protocol: Pubkey,
    pub buyback_id: u64,
    pub input_amount_lamports: u64,
    pub acquired_token: Pubkey,
    pub acquired_amount: u64,
    pub average_price_lamports_per_token: u64,
    pub destination_split: DestinationSplit,
    pub completed_at_slot: u64,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[error_code]
pub enum BybakError {
    #[msg("Protocol name must not be empty.")]
    NameEmpty,
    #[msg("Protocol name exceeds 32 chars.")]
    NameTooLong,
    #[msg("Input amount must be positive.")]
    ZeroInput,
    #[msg("Slice count must be positive.")]
    ZeroSlices,
    #[msg("TWAP window must be at least 60 seconds.")]
    WindowTooShort,
    #[msg("Weighted destination split must total 10000 bps.")]
    InvalidDestinationSplit,
    #[msg("Buyback already closed.")]
    AlreadyClosed,
    #[msg("Acquired amount must be positive.")]
    ZeroAcquired,
    #[msg("Average price must be positive.")]
    ZeroAveragePrice,
    #[msg("Signer is not the registered protocol authority.")]
    UnauthorizedProtocol,
    #[msg("Buyback does not belong to the supplied registry.")]
    RegistryMismatch,
    #[msg("Arithmetic overflow.")]
    MathOverflow,
}
