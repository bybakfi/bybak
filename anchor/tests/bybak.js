// Bybak localnet test suite.
//
// Run against a local validator only:
//   solana-test-validator            (separate shell)
//   anchor test --provider.cluster localnet --skip-local-validator
//
// Never point this at devnet/mainnet -- it opens and closes real buybacks.
// The devnet verification that actually ran lives in scripts/devnet-sanity.js.

const anchor = require("@coral-xyz/anchor");
const { PublicKey, SystemProgram, Keypair } = require("@solana/web3.js");
const assert = require("assert");

const REGISTRY_SEED = Buffer.from("registry");
const BUYBACK_SEED = Buffer.from("buyback");
const ATTESTATION_SEED = Buffer.from("attestation");
const ACQUIRED_TOKEN = new PublicKey("So11111111111111111111111111111111111111112");

function u64le(n) {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}

describe("bybak", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.bybak;
  const protocol = provider.wallet.publicKey;

  const registryPda = PublicKey.findProgramAddressSync(
    [REGISTRY_SEED, protocol.toBuffer()],
    program.programId
  )[0];

  const buybackPda = (id) =>
    PublicKey.findProgramAddressSync(
      [BUYBACK_SEED, registryPda.toBuffer(), u64le(id)],
      program.programId
    )[0];

  const attestationPda = (buyback) =>
    PublicKey.findProgramAddressSync(
      [ATTESTATION_SEED, buyback.toBuffer()],
      program.programId
    )[0];

  it("registers a protocol", async () => {
    await program.methods
      .registerProtocol("Bybak Reference Protocol")
      .accounts({
        registry: registryPda,
        protocol,
        payer: protocol,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const registry = await program.account.protocolRegistry.fetch(registryPda);
    assert.strictEqual(registry.protocolName, "Bybak Reference Protocol");
    assert.strictEqual(registry.protocol.toBase58(), protocol.toBase58());
    assert.strictEqual(Number(registry.buybacksCompleted), 0);
    assert.strictEqual(registry.specVersion, 1);
  });

  it("opens and closes a buyback, writing an attestation", async () => {
    const id = 1;
    const buyback = buybackPda(id);
    const input = new anchor.BN(2_500_000_000);

    await program.methods
      .openBuyback(
        new anchor.BN(id),
        input,
        { weighted: { burnBps: 6000, liquidityBps: 2500, stakersBps: 1500 } },
        { twap: { windowSeconds: 3600, sliceCount: 24 } }
      )
      .accounts({
        registry: registryPda,
        buyback,
        acquiredToken: ACQUIRED_TOKEN,
        protocol,
        payer: protocol,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    let state = await program.account.buyback.fetch(buyback);
    assert.ok("open" in state.status);
    assert.strictEqual(state.acquiredToken.toBase58(), ACQUIRED_TOKEN.toBase58());

    const attestation = attestationPda(buyback);
    const acquired = new anchor.BN(1_284_700_000);

    await program.methods
      .closeBuyback(acquired, new anchor.BN(1946))
      .accounts({
        registry: registryPda,
        buyback,
        attestation,
        protocol,
        payer: protocol,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    state = await program.account.buyback.fetch(buyback);
    assert.ok("closed" in state.status);

    const att = await program.account.attestation.fetch(attestation);
    assert.strictEqual(att.buybackId.toString(), String(id));
    assert.strictEqual(att.protocol.toBase58(), protocol.toBase58());
    assert.ok(att.inputAmountLamports.eq(input));
    assert.ok(att.acquiredAmount.eq(acquired));
    assert.strictEqual(att.destinationSplit.burnBps, 6000);
    assert.strictEqual(
      att.destinationSplit.burnBps +
        att.destinationSplit.liquidityBps +
        att.destinationSplit.stakersBps,
      10000
    );

    const registry = await program.account.protocolRegistry.fetch(registryPda);
    assert.strictEqual(Number(registry.buybacksCompleted), 1);
  });

  it("rejects a second close of the same buyback", async () => {
    const buyback = buybackPda(1);
    await assert.rejects(() =>
      program.methods
        .closeBuyback(new anchor.BN(1), new anchor.BN(1))
        .accounts({
          registry: registryPda,
          buyback,
          attestation: attestationPda(buyback),
          protocol,
          payer: protocol,
          systemProgram: SystemProgram.programId,
        })
        .rpc()
    );
  });

  it("rejects a TWAP window under the 60 second floor", async () => {
    const id = 2;
    await assert.rejects(
      () =>
        program.methods
          .openBuyback(
            new anchor.BN(id),
            new anchor.BN(1_000_000),
            { burn: {} },
            { twap: { windowSeconds: 30, sliceCount: 4 } }
          )
          .accounts({
            registry: registryPda,
            buyback: buybackPda(id),
            acquiredToken: ACQUIRED_TOKEN,
            protocol,
            payer: protocol,
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
      /WindowTooShort/
    );
  });

  it("rejects a weighted split that does not total 10000 bps", async () => {
    const id = 3;
    await assert.rejects(
      () =>
        program.methods
          .openBuyback(
            new anchor.BN(id),
            new anchor.BN(1_000_000),
            { weighted: { burnBps: 5000, liquidityBps: 2500, stakersBps: 1500 } },
            { immediate: {} }
          )
          .accounts({
            registry: registryPda,
            buyback: buybackPda(id),
            acquiredToken: ACQUIRED_TOKEN,
            protocol,
            payer: protocol,
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
      /InvalidDestinationSplit/
    );
  });

  it("rejects a stranger opening a buyback on someone else's registry", async () => {
    const stranger = Keypair.generate();
    const id = 4;
    await assert.rejects(() =>
      program.methods
        .openBuyback(
          new anchor.BN(id),
          new anchor.BN(1_000_000),
          { burn: {} },
          { immediate: {} }
        )
        .accounts({
          registry: registryPda,
          buyback: buybackPda(id),
          acquiredToken: ACQUIRED_TOKEN,
          protocol: stranger.publicKey,
          payer: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([stranger])
        .rpc()
    );
  });
});
