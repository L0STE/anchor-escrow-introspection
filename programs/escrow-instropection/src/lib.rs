use anchor_lang::prelude::*;
use anchor_spl::{token::{Token, TokenAccount, Mint}, associated_token::AssociatedToken};
use solana_program::sysvar::instructions::{
    self,
    load_current_index_checked, 
    load_instruction_at_checked
};

declare_id!("8nKTU6gXpgxREBG67fsp8sAnrk5pzhnjKVV4pssXvcLV");

#[program]
pub mod escrow_instropection {
    use anchor_spl::{token::{Transfer, transfer, CloseAccount, close_account}, associated_token::get_associated_token_address};

    use super::*;

    pub fn make(ctx: Context<Make>, deposit_amount: u64, take_amount: u64) -> Result<()> {
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

    pub fn take(ctx: Context<Take>) -> Result<()> {
        let index = load_current_index_checked(&ctx.accounts.instructions.to_account_info())?;
        let ix = load_instruction_at_checked(index as usize + 1, &ctx.accounts.instructions.to_account_info())?;

        let maker_ata = get_associated_token_address(&ctx.accounts.maker.key(), &ctx.accounts.escrow.mint_b);

        require_keys_eq!(ix.program_id, ctx.accounts.token_program.key(), EscrowError::InvalidTokenProgram);
        require_eq!(ix.data[0], 3u8, EscrowError::InvalidIx);
        require!(ix.data[1..9].eq(&ctx.accounts.escrow.take_amount.to_le_bytes()), EscrowError::InvalidAmount);
        require_keys_eq!(ix.accounts.get(1).unwrap().pubkey, maker_ata, EscrowError::InvalidMakerATA);

        let binding = [ctx.accounts.escrow.bump];
        let signer_seeds = [&[b"escrow", ctx.accounts.maker.to_account_info().key.as_ref(), &binding][..]];
        
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.taker_ata.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info()
            },
            &signer_seeds
        );
        transfer(cpi_ctx, ctx.accounts.vault.amount)?;

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.vault.to_account_info(),
                destination: ctx.accounts.taker.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info()
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
    #[account(
        mut
    )]
    taker: Signer<'info>,
    #[account(
        mut
    )]
    maker: SystemAccount<'info>,
    mint_a: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_a,
        associated_token::authority = taker
    )]
    taker_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow
    )]
    vault: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"escrow", maker.key().as_ref()],
        bump = escrow.bump
    )]
    escrow: Account<'info, Escrow>,
    #[account(address = instructions::ID)]
    /// CHECK: InstructionsSysvar account
    instructions: UncheckedAccount<'info>,
    token_program: Program<'info, Token>,
    associated_token_program: Program<'info, AssociatedToken>,
    system_program: Program<'info, System>
}

#[account]
pub struct Escrow {
    mint_b: Pubkey,
    take_amount: u64,
    bump: u8
}

impl Space for Escrow {
    const INIT_SPACE: usize = 8 + 32 + 8 + 1;
}

#[error_code]
pub enum EscrowError {
    #[msg("Invalid instruction")]
    InvalidIx,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Invalid Token program")]
    InvalidTokenProgram,
    #[msg("Invalid Maker ATA")]
    InvalidMakerATA
}