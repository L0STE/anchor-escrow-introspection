use anchor_lang::prelude::*;
use anchor_spl::{
    token::{Token, TokenAccount, Mint, Transfer, transfer, CloseAccount, close_account}, 
    associated_token::{AssociatedToken, get_associated_token_address}
};
use solana_program::sysvar::instructions::{
        self,
        load_current_index_checked, 
        load_instruction_at_checked
    };
use anchor_lang::Discriminator;

declare_id!("E1sj1hjSof4cpxvwV9Ufi86htipaEnyVwBProXJ5bmEW");

#[program]
pub mod escrow_instropection {
    use super::*;

    pub fn make(ctx: Context<Make>, deposit_amount: u64, take_amount: u64) -> Result<()> {
        ctx.accounts.escrow.maker = *ctx.accounts.maker.key;
        ctx.accounts.escrow.take_amount = take_amount;
        ctx.accounts.escrow.mint_b = ctx.accounts.mint_b.key();
        ctx.accounts.escrow.bump = ctx.bumps.escrow;
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.maker_ata.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.maker.to_account_info()
            }
        );
        transfer(cpi_ctx, deposit_amount)
    }

    pub fn take_start(ctx: Context<Take>) -> Result<()> {

        // take_start: Taker receives Mint A from the Vault
        // take_end: Taker sends Mint B to the Maker

        let escrow = ctx.accounts.escrow.as_ref().unwrap();

        // Check what is the current index of the instruction that is being executed (Might be in a different position than [0]).
        let index = load_current_index_checked(&ctx.accounts.instructions.to_account_info())?;

        // We then Load the instruction that is right after / the one that we want to check for the correct input.
        let ix = load_instruction_at_checked(index as usize + 1, &ctx.accounts.instructions.to_account_info())?;
        
        // Before going thourgh the next part we should ask ourselves what information are essential to not incurr in a malicious attack.

        // We usually start by checking if the program used is the one that we expect. In this occasion we will use the Escrow program.
        require_keys_eq!(ix.program_id, ID, EscrowError::InvalidProgram);

        // We then check if the instruction is the one that we expect. To do so we compare the discriminator of the instruction with the one that we expect > In this case the TakeEnd instruction.
        // NB: Anchor makes it easier for us by providing the Discriminator trait directly on the instruction > use anchor_lang::Discriminator. For a prorgram instruction usually the first 8 bytes are the discriminator.
        require!(ix.data[0..8].eq(instruction::TakeEnd::DISCRIMINATOR.as_slice()), EscrowError::InvalidIx);

        // This part will then always be different based on the logic of the instruction that we are checking.

        // In our case we want to make sure that the maker is getting the right amount of token & that the we are sending the right token and to the right persone (an Ata provdes both check).
        // To perform the fisrt check we search for the amount that we will send in as a parameter in the instruction. We search it with an offset of 8 bytes (the discriminator) and a length of 8 bytes (since it's a u64).
        require!(ix.data[8..16].eq(&escrow.take_amount.to_le_bytes()), EscrowError::InvalidAmount);

        // To perform the second check we need to get the associated token address of the maker and compare it with the one that we are sending the token to. This time we search this information in the account struct of the instruction.
        let maker_ata = get_associated_token_address(&ctx.accounts.maker.key(), &escrow.mint_b);
        require_keys_eq!(ix.accounts.get(3).unwrap().pubkey, maker_ata, EscrowError::InvalidMakerATA);

        let binding = [escrow.bump];
        let signer_seeds = [&[b"escrow", ctx.accounts.maker.to_account_info().key.as_ref(), &binding][..]];
        
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sending_ata.to_account_info(),
                to: ctx.accounts.destination_ata.to_account_info(),
                authority: escrow.to_account_info()
            },
            &signer_seeds
        );
        transfer(cpi_ctx, ctx.accounts.sending_ata.amount)?;

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.sending_ata.to_account_info(),
                destination: ctx.accounts.maker.to_account_info(),
                authority: escrow.to_account_info()
            },
            &signer_seeds
        );
        close_account(cpi_ctx)
    }

    pub fn take_end(ctx: Context<Take>, amount: u64) -> Result<()> {
        
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sending_ata.to_account_info(),
                to: ctx.accounts.destination_ata.to_account_info(),
                authority: ctx.accounts.taker.to_account_info()
            },
        );
        transfer(cpi_ctx, amount)

    }

    pub fn take_token(ctx: Context<Take>) -> Result<()> {

        // In the last example we saw how to perform checks on a program instruction. In this example we will see how to perform instruction introspection on a spl_transfer function.

        let escrow = ctx.accounts.escrow.as_ref().unwrap();

        // Same Checks as before
        let index = load_current_index_checked(&ctx.accounts.instructions.to_account_info())?;
        let ix = load_instruction_at_checked(index as usize + 1, &ctx.accounts.instructions.to_account_info())?;

        // Since we want to check for a spl_transfer, this time the program should be the Token Program         
        require_keys_eq!(ix.program_id, ctx.accounts.token_program.key(), EscrowError::InvalidProgram);

        // The Discriminator on instruction coming from the Token Program is 1 byte long. We can then check if the instruction is a spl_transfer by checking if the first byte is equal to 3 (as u8).
        require_eq!(ix.data[0], 3u8, EscrowError::InvalidIx);

        // Here we perform the same checks as before but this time the offset is only 1 byte long since the discriminator is only 1 byte long.
        require!(ix.data[1..9].eq(&escrow.take_amount.to_le_bytes()), EscrowError::InvalidAmount);

        // Here we peroform the same checks as before but this time we check fo the first account: We are checking for the ATA of destination.
        // Transfer {
        //     from: Account(0)
        //     to: Account(1)
        //     authority: Account(2)
        // }
        let maker_ata = get_associated_token_address(&ctx.accounts.maker.key(), &escrow.mint_b);
        require_keys_eq!(ix.accounts.get(1).unwrap().pubkey, maker_ata, EscrowError::InvalidMakerATA);

        // This time we'll not see any additional instruction because we will perform the transfer directly on the Typescript side.

        let binding = [escrow.bump];
        let signer_seeds = [&[b"escrow", ctx.accounts.maker.to_account_info().key.as_ref(), &binding][..]];
        
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sending_ata.to_account_info(),
                to: ctx.accounts.destination_ata.to_account_info(),
                authority: escrow.to_account_info()
            },
            &signer_seeds
        );
        transfer(cpi_ctx, ctx.accounts.sending_ata.amount)?;

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.sending_ata.to_account_info(),
                destination: ctx.accounts.maker.to_account_info(),
                authority: escrow.to_account_info()
            },
            &signer_seeds
        );
        close_account(cpi_ctx)
    }

    pub fn take_sol(ctx: Context<Take>) -> Result<()> {

        // In the last example we saw how to perform checks on a program instruction & Spl Program instruction. In this example we will see how to perform instruction introspection on a system_program function.

        let escrow = ctx.accounts.escrow.as_ref().unwrap();

        // Same Checks as before
        let index = load_current_index_checked(&ctx.accounts.instructions.to_account_info())?;
        let ix = load_instruction_at_checked(index as usize + 1, &ctx.accounts.instructions.to_account_info())?;

        // Since we want to check for a system_tranfer (or sol Transfer), this time the program should be the System Program         
        require_keys_eq!(ix.program_id, ctx.accounts.system_program.key(), EscrowError::InvalidProgram);

        // The Discriminator on instruction coming from the Token Program is 4 byte long. We can then check if the instruction is a sol_transfer by checking if the first byte is equal to 2 (as u8).
        require_eq!(ix.data[0], 2u8, EscrowError::InvalidIx);

        // Here we perform the same checks as before but this time the offset is only 4 byte long since the discriminator is only 1 byte long.
        require!(ix.data[4..12].eq(&escrow.take_amount.to_le_bytes()), EscrowError::InvalidAmount);

        // Here we peroform the same checks as before but this time we check fo the first account: We are checking for the Publickey of destination.
        // pub struct Transfer<'info> {
        //     from: account(0)
        //     to: Account(1)
        // }
        require_keys_eq!(ix.accounts.get(1).unwrap().pubkey, escrow.maker, EscrowError::InvalidMakerATA);

        // This time we'll not see any additional instruction because we will perform the transfer directly on the Typescript side.

        let binding = [escrow.bump];
        let signer_seeds = [&[b"escrow", ctx.accounts.maker.to_account_info().key.as_ref(), &binding][..]];
        
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sending_ata.to_account_info(),
                to: ctx.accounts.destination_ata.to_account_info(),
                authority: escrow.to_account_info()
            },
            &signer_seeds
        );
        transfer(cpi_ctx, ctx.accounts.sending_ata.amount)?;

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.sending_ata.to_account_info(),
                destination: ctx.accounts.maker.to_account_info(),
                authority: escrow.to_account_info()
            },
            &signer_seeds
        );
        close_account(cpi_ctx)
    }
}

#[derive(Accounts)]
pub struct Make<'info> {
    #[account(
        mut
    )]
    maker: Signer<'info>,
    mint_a: Account<'info, Mint>,
    mint_b: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker
    )]
    maker_ata: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = escrow
    )]
    vault: Account<'info, TokenAccount>,
    #[account(
        init,
        space = Escrow::INIT_SPACE,
        payer = maker,
        seeds = [b"escrow", maker.key().as_ref()],
        bump
    )]
    escrow: Account<'info, Escrow>,
    token_program: Program<'info, Token>,
    associated_token_program: Program<'info, AssociatedToken>,
    system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct Take<'info> {
    #[account(mut)]
    taker: Signer<'info>,
    #[account(mut)]
    maker: SystemAccount<'info>,
    #[account(mut)]
    sending_ata: Account<'info, TokenAccount>, //Start: Vault; End: TakerAtaB
    #[account(mut)]
    destination_ata: Account<'info, TokenAccount>, //Start: TakerAtaA; End: MakerAtaB
    #[account(
        mut,
        close = maker,
        seeds = [b"escrow", maker.key().as_ref()],
        bump = escrow.bump
    )]
    escrow: Option<Account<'info, Escrow>>,
    #[account(address = instructions::ID)]
    /// CHECK: InstructionsSysvar account
    instructions: UncheckedAccount<'info>,
    token_program: Program<'info, Token>,
    associated_token_program: Program<'info, AssociatedToken>,
    system_program: Program<'info, System>
}

#[account]
pub struct Escrow {
    maker: Pubkey,
    mint_b: Pubkey,
    take_amount: u64,
    bump: u8
}

impl Space for Escrow {
    const INIT_SPACE: usize = 8 + 32 + 32 + 8 + 1;
}

#[error_code]
pub enum EscrowError {
    #[msg("Invalid instruction")]
    InvalidIx,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Invalid program")]
    InvalidProgram,
    #[msg("Invalid Maker ATA")]
    InvalidMakerATA
}