import * as anchor from "@coral-xyz/anchor";
import { Program, BN, AnchorError } from "@coral-xyz/anchor";
import { EscrowInstropection } from "../target/types/escrow_instropection";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SYSVAR_INSTRUCTIONS_PUBKEY,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMint2Instruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  getMinimumBalanceForRentExemptMint,
  createTransferCheckedInstruction,
  createTransferInstruction
} from "@solana/spl-token";
import { randomBytes } from "crypto";

describe("escrow-instropection", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.EscrowInstropection as Program<EscrowInstropection>;

  const provider = anchor.getProvider();

  const connection = provider.connection;

  const confirm = async (signature: string): Promise<string> => {
    const block = await connection.getLatestBlockhash();
    await connection.confirmTransaction({
      signature,
      ...block,
    });
    return signature;
  };

  const log = async (signature: string): Promise<string> => {
    console.log(
      `Your transaction signature: https://explorer.solana.com/transaction/${signature}?cluster=custom&customUrl=${connection.rpcEndpoint}`
    );
    return signature;
  };

  const [maker, taker, mintA, mintB] = Array.from({ length: 4 }, () =>
    Keypair.generate()
  );

  const [makerAtaA, makerAtaB, takerAtaA, takerAtaB] = [maker, taker]
    .map((a) =>
      [mintA, mintB].map((m) =>
        getAssociatedTokenAddressSync(m.publicKey, a.publicKey)
      )
    )
    .flat();

  const escrow = PublicKey.findProgramAddressSync(
    [Buffer.from("escrow"), maker.publicKey.toBuffer()],
    program.programId
  )[0];
  const vault = getAssociatedTokenAddressSync(mintA.publicKey, escrow, true);

  // Accounts
  const accounts = {
    maker: maker.publicKey,
    taker: taker.publicKey,
    mintA: mintA.publicKey,
    mintB: mintB.publicKey,
    makerAtaA,
    makerAtaB,
    takerAtaA,
    takerAtaB,
    escrow,
    vault,
    associatedTokenprogram: ASSOCIATED_TOKEN_PROGRAM_ID,
    tokenProgram: TOKEN_PROGRAM_ID,
    systemProgram: SystemProgram.programId 
  }

  it("Airdrop and create mints", async () => {
    let lamports = await getMinimumBalanceForRentExemptMint(connection);
    let tx = new Transaction();
    tx.instructions = [
      ...[maker, taker].map((k) =>
        SystemProgram.transfer({
          fromPubkey: provider.publicKey,
          toPubkey: k.publicKey,
          lamports: 10 * LAMPORTS_PER_SOL,
        })
      ),
      ...[mintA, mintB].map((m) =>
        SystemProgram.createAccount({
          fromPubkey: provider.publicKey,
          newAccountPubkey: m.publicKey,
          lamports,
          space: MINT_SIZE,
          programId: TOKEN_PROGRAM_ID,
        })
      ),
      ...[
        [mintA.publicKey, maker.publicKey, makerAtaA],
        [mintB.publicKey, taker.publicKey, takerAtaB],
      ]
      .flatMap((x) => [
        createInitializeMint2Instruction(x[0], 6, x[1], null),
        createAssociatedTokenAccountIdempotentInstruction(provider.publicKey, x[2], x[1], x[0]),
        createMintToInstruction(x[0], x[2], x[1], 1e9),
      ])
    ];

    await provider.sendAndConfirm(tx, [mintA, mintB, maker, taker]).then(log);
  });

  it("Make", async () => {
    await program.methods
      .make(new BN(1e6), new BN(1e6))
      .accounts({...accounts, makerAta: makerAtaA})
      .signers([maker])
      .rpc()
      .then(confirm)
      .then(log);
  });

  it("Take Invalid Ix", async () => {
    let tx = new Transaction();

    let takeIx = await program.methods
    .take()
    .accounts({
      ...accounts,
      takerAta: takerAtaA,
      instructions: SYSVAR_INSTRUCTIONS_PUBKEY
    })
    .instruction();

    tx.instructions = [
      createAssociatedTokenAccountIdempotentInstruction(taker.publicKey, makerAtaB, maker.publicKey, mintB.publicKey),
      takeIx,
      createTransferCheckedInstruction(
        takerAtaB,
        mintB.publicKey,
        makerAtaB,
        taker.publicKey,
        1_000_000,
        6
      )
    ]
    try {
      await provider.sendAndConfirm(tx, [ taker ]).then(confirm).then(log);
    } catch(e) {
      let error = e as AnchorError;
      for (let log of error.logs) {
        if (
          log.startsWith("Program log: AnchorError") &&
          log.endsWith("Error Code: InvalidIx. Error Number: 6000. Error Message: Invalid instruction.")
        ) {
          return;
        }
      }
      throw e;
    }
  })

  it("Take Invalid Amount", async () => {
    let tx = new Transaction();

    let takeIx = await program.methods
    .take()
    .accounts({
      ...accounts,
      takerAta: takerAtaA,
      instructions: SYSVAR_INSTRUCTIONS_PUBKEY
    })
    .instruction();

    tx.instructions = [
      createAssociatedTokenAccountIdempotentInstruction(taker.publicKey, makerAtaB, maker.publicKey, mintB.publicKey),
      takeIx,
      createTransferInstruction(
        takerAtaB,
        makerAtaB,
        taker.publicKey,
        1_000_001
      )
    ]
    try {
      await provider.sendAndConfirm(tx, [ taker ]).then(confirm).then(log);
    } catch(e) {
      let error = e as AnchorError;
      for (let log of error.logs) {
        if (
          log.startsWith("Program log: AnchorError") &&
          log.endsWith("Error Code: InvalidAmount. Error Number: 6001. Error Message: Invalid amount.")
        ) {
          return;
        }
      }
      throw e;
    }
  })

  it("Take Invalid Token Program", async () => {
    let tx = new Transaction();

    let takeIx = await program.methods
    .take()
    .accounts({
      ...accounts,
      takerAta: takerAtaA,
      instructions: SYSVAR_INSTRUCTIONS_PUBKEY
    })
    .instruction();

    tx.instructions = [
      createAssociatedTokenAccountIdempotentInstruction(taker.publicKey, makerAtaB, maker.publicKey, mintB.publicKey),
      takeIx,
      createTransferInstruction(
        takerAtaB,
        makerAtaB,
        taker.publicKey,
        1_000_000,
        undefined,
        TOKEN_2022_PROGRAM_ID
      )
    ]
    try {
      await provider.sendAndConfirm(tx, [ taker ]).then(confirm).then(log);
    } catch(e) {
      let error = e as AnchorError;
      for (let log of error.logs) {
        if (
          log.startsWith("Program log: AnchorError") &&
          log.endsWith("Error Code: InvalidTokenProgram. Error Number: 6002. Error Message: Invalid Token program.")
        ) {
          return;
        }
      }
      throw e;
    }
  })

  it("Take Invalid Maker ATA", async () => {
    let tx = new Transaction();

    let takeIx = await program.methods
    .take()
    .accounts({
      ...accounts,
      takerAta: takerAtaA,
      instructions: SYSVAR_INSTRUCTIONS_PUBKEY
    })
    .instruction();

    tx.instructions = [
      createAssociatedTokenAccountIdempotentInstruction(taker.publicKey, makerAtaB, maker.publicKey, mintB.publicKey),
      takeIx,
      createTransferInstruction(
        takerAtaB,
        takerAtaB,
        taker.publicKey,
        1_000_000,
      )
    ]
    try {
      await provider.sendAndConfirm(tx, [ taker ]).then(confirm).then(log);
    } catch(e) {
      let error = e as AnchorError;
      for (let log of error.logs) {
        if (
          log.startsWith("Program log: AnchorError") &&
          log.endsWith("Error Code: InvalidMakerATA. Error Number: 6003. Error Message: Invalid Maker ATA.")
        ) {
          return;
        }
      }
      throw e;
    }
  })

  it("Take", async () => {
    let tx = new Transaction();

    let takeIx = await program.methods
    .take()
    .accounts({
      ...accounts,
      takerAta: takerAtaA,
      instructions: SYSVAR_INSTRUCTIONS_PUBKEY
    })
    .instruction();

    tx.instructions = [
      createAssociatedTokenAccountIdempotentInstruction(taker.publicKey, makerAtaB, maker.publicKey, mintB.publicKey),
      takeIx,
      createTransferInstruction(
        takerAtaB,
        makerAtaB,
        taker.publicKey,
        1_000_000
      )
    ]
    try {
      await provider.sendAndConfirm(tx, [ taker ]).then(confirm).then(log);
    } catch(e) {
      console.log(e);
      throw(e)
    }
  })
});