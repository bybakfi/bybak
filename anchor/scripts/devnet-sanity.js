// Devnet sanity check: register_protocol -> open_buyback -> close_buyback,
// then read every account back and assert the attestation matches SPEC.md section 2.
//
// Run through scripts/safe_solana_tx.sh so the keypair/cluster/balance gate applies.

const fs = require("fs");
const os = require("os");
const path = require("path");
const anchor = require("@coral-xyz/anchor");
const { Connection, Keypair, PublicKey, SystemProgram } = require("@solana/web3.js");

const RPC = "https://api.devnet.solana.com";
const EXPECTED_PUBKEY = "BmnQou5hLWSvhw4CY5TzEzWVfz7UANyhAaGRc4YTQcqD";
const KEYPAIR_PATH = path.join(os.homedir(), "bybak-deploy.json");
// Wrapped SOL mint -- stands in as the "acquired token" for the sanity run.
const ACQUIRED_TOKEN = new PublicKey("So11111111111111111111111111111111111111112");

const REGISTRY_SEED = Buffer.from("registry");
const BUYBACK_SEED = Buffer.from("buyback");
const ATTESTATION_SEED = Buffer.from("attestation");

function u64le(n) {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}

(async () => {
  const secret = Uint8Array.from(JSON.parse(fs.readFileSync(KEYPAIR_PATH, "utf8")));
  const kp = Keypair.fromSecretKey(secret);

  if (kp.publicKey.toBase58() !== EXPECTED_PUBKEY) {
    throw new Error(`PUBKEY MISMATCH -- ABORT. derived=${kp.publicKey.toBase58()}`);
  }

  const connection = new Connection(RPC, "confirmed");
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(kp), {
    commitment: "confirmed",
    preflightCommitment: "confirmed",
  });
  anchor.setProvider(provider);

  const idl = JSON.parse(fs.readFileSync("target/idl/bybak.json", "utf8"));
  const program = new anchor.Program(idl, provider);
  console.log("program id :", program.programId.toBase58());
  console.log("signer     :", kp.publicKey.toBase58());

  const [registryPda] = PublicKey.findProgramAddressSync(
    [REGISTRY_SEED, kp.publicKey.toBuffer()],
    program.programId
  );

  // --- 1. register_protocol -------------------------------------------------
  let registry = await program.account.protocolRegistry.fetchNullable(registryPda);
  if (registry === null) {
    const sig = await program.methods
      .registerProtocol("Bybak Reference Protocol")
      .accounts({
        registry: registryPda,
        protocol: kp.publicKey,
        payer: kp.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    console.log("register_protocol tx:", sig);
    registry = await program.account.protocolRegistry.fetch(registryPda);
  } else {
    console.log("registry already exists, reusing");
  }
  console.log("registry pda:", registryPda.toBase58(), "| name:", registry.protocolName);

  // --- 2. open_buyback ------------------------------------------------------
  const buybackId = Number(registry.buybacksOpened) + 1;
  const [buybackPda] = PublicKey.findProgramAddressSync(
    [BUYBACK_SEED, registryPda.toBuffer(), u64le(buybackId)],
    program.programId
  );

  const inputLamports = new anchor.BN(2_500_000_000); // 2.5 SOL of protocol revenue
  const openSig = await program.methods
    .openBuyback(
      new anchor.BN(buybackId),
      inputLamports,
      { weighted: { burnBps: 6000, liquidityBps: 2500, stakersBps: 1500 } },
      { twap: { windowSeconds: 3600, sliceCount: 24 } }
    )
    .accounts({
      registry: registryPda,
      buyback: buybackPda,
      acquiredToken: ACQUIRED_TOKEN,
      protocol: kp.publicKey,
      payer: kp.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("open_buyback tx:", openSig);

  const buyback = await program.account.buyback.fetch(buybackPda);
  console.log(
    "buyback pda:", buybackPda.toBase58(),
    "| id:", buyback.buybackId.toString(),
    "| status:", Object.keys(buyback.status)[0],
    "| schedule:", JSON.stringify(buyback.schedule)
  );

  // --- 3. close_buyback -----------------------------------------------------
  const [attestationPda] = PublicKey.findProgramAddressSync(
    [ATTESTATION_SEED, buybackPda.toBuffer()],
    program.programId
  );

  const acquiredAmount = new anchor.BN(1_284_700_000);
  const avgPrice = new anchor.BN(1_946);
  const closeSig = await program.methods
    .closeBuyback(acquiredAmount, avgPrice)
    .accounts({
      registry: registryPda,
      buyback: buybackPda,
      attestation: attestationPda,
      protocol: kp.publicKey,
      payer: kp.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("close_buyback tx:", closeSig);

  // --- 4. verify attestation against SPEC.md section 2 ----------------------
  const att = await program.account.attestation.fetch(attestationPda);
  const registryAfter = await program.account.protocolRegistry.fetch(registryPda);
  const buybackAfter = await program.account.buyback.fetch(buybackPda);

  console.log("\n--- attestation", attestationPda.toBase58(), "---");
  console.log("  buyback_id                      :", att.buybackId.toString());
  console.log("  protocol                        :", att.protocol.toBase58());
  console.log("  input_amount_lamports           :", att.inputAmountLamports.toString());
  console.log("  acquired_token                  :", att.acquiredToken.toBase58());
  console.log("  acquired_amount                 :", att.acquiredAmount.toString());
  console.log("  average_price_lamports_per_token:", att.averagePriceLamportsPerToken.toString());
  console.log("  destination_split               :", JSON.stringify(att.destinationSplit));
  console.log("  completed_at_slot               :", att.completedAtSlot.toString());
  console.log("  spec_version                    :", att.specVersion);

  const checks = [
    ["status is Closed", Object.keys(buybackAfter.status)[0] === "closed"],
    ["attestation.buyback_id matches", att.buybackId.toString() === String(buybackId)],
    ["attestation.protocol matches signer", att.protocol.toBase58() === kp.publicKey.toBase58()],
    ["attestation.acquired_token matches", att.acquiredToken.toBase58() === ACQUIRED_TOKEN.toBase58()],
    ["attestation.input matches declared", att.inputAmountLamports.eq(inputLamports)],
    ["attestation.acquired matches", att.acquiredAmount.eq(acquiredAmount)],
    ["split totals 10000 bps",
      att.destinationSplit.burnBps + att.destinationSplit.liquidityBps + att.destinationSplit.stakersBps === 10000],
    ["split preserved from Weighted", att.destinationSplit.burnBps === 6000],
    ["registry.buybacks_completed incremented", Number(registryAfter.buybacksCompleted) >= 1],
    ["registry.total_acquired incremented", registryAfter.totalAcquired.eq(acquiredAmount)],
  ];

  console.log("\n--- assertions ---");
  let failed = 0;
  for (const [label, ok] of checks) {
    console.log(`  ${ok ? "PASS" : "FAIL"}  ${label}`);
    if (!ok) failed++;
  }

  // --- 5. negative check: double close must be rejected ---------------------
  let doubleCloseRejected = false;
  try {
    await program.methods
      .closeBuyback(acquiredAmount, avgPrice)
      .accounts({
        registry: registryPda,
        buyback: buybackPda,
        attestation: attestationPda,
        protocol: kp.publicKey,
        payer: kp.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  } catch (e) {
    doubleCloseRejected = true;
  }
  console.log(`  ${doubleCloseRejected ? "PASS" : "FAIL"}  double close rejected`);
  if (!doubleCloseRejected) failed++;

  console.log("\nSIGNATURES");
  console.log("  open_buyback :", openSig);
  console.log("  close_buyback:", closeSig);
  console.log("\nPDAs");
  console.log("  registry   :", registryPda.toBase58());
  console.log("  buyback    :", buybackPda.toBase58());
  console.log("  attestation:", attestationPda.toBase58());

  if (failed > 0) {
    console.error(`\n${failed} assertion(s) FAILED`);
    process.exit(1);
  }
  console.log("\nALL SANITY CHECKS PASSED");
})().catch((e) => {
  console.error("SANITY RUN FAILED:", e);
  process.exit(1);
});
