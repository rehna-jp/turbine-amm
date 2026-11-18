use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};
use constant_product_curve::{ConstantProduct, LiquidityPair};

use crate::{error::AmmError, state::Config};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,
    #[account(
        has_one = mint_x,
        has_one = mint_y,
        seeds = [b"config", config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config,
    )]
    pub vault_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config,
    )]
    pub vault_y: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = user,
    )]
    pub user_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = user,
    )]
    pub user_y: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = treasury_token.owner == config.treasury @ AmmError::InvalidAuthority,
    )]
    pub treasury_token: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Swap<'info> {
    pub fn swap(
        &mut self,
        is_x: bool,
        amount_in: u64,
        min_out: u64, 
    ) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);
        require_neq!(amount_in, 0, AmmError::InvalidAmount);
        
        let input_mint = if is_x { self.mint_x.key() } else { self.mint_y.key() };
        require_keys_eq!(self.treasury_token.mint, input_mint, AmmError::InvalidToken);
        // 1. Initialize the curve with current pool balances
       let mut curve = ConstantProduct::init(
            self.vault_x.amount,
            self.vault_y.amount,
            self.vault_x.amount, // curve invariant tracking
            self.config.fee,     // e.g., 30 for 0.3%
            Some(6),                   // decimals
        ).map_err(AmmError::from)?;
        // 2. Perform the swap calculation
        let pair = if is_x { LiquidityPair::X } else { LiquidityPair::Y };
        let res = curve.swap(pair, amount_in, min_out).map_err(AmmError::from)?;

        // 3. Transfer input tokens from user into the vault
        self.deposit_tokens(is_x, res.deposit)?;

        // 4. Transfer output tokens from the vault to the user
        self.withdraw_tokens(is_x, res.withdraw)?;

        let protocol_fee = res.fee / 2;
        if protocol_fee > 0 {
            self.pay_protocol_fee(is_x, protocol_fee)?;
        }

        Ok(())
    }
    // Helper: User -> Vault (User signs)
    fn deposit_tokens(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = if is_x {
            (self.user_x.to_account_info(), self.vault_x.to_account_info())
        } else {
            (self.user_y.to_account_info(), self.vault_y.to_account_info())
        };
        let cpi_program = self.token_program.key();
        let cpi_accounts = Transfer {
            from,
            to,
            authority: self.user.to_account_info(),
        };

        let ctx = CpiContext::new(cpi_program, cpi_accounts);
        transfer(ctx, amount)
    }

    // Helper: Vault -> User (Config PDA signs)
    fn withdraw_tokens(&self, is_x: bool, amount: u64) -> Result<()> {
        // If the user gave X, they receive Y; if they gave Y, they receive X
        let (from, to) = if is_x {
            (self.vault_y.to_account_info(), self.user_y.to_account_info())
        } else {
            (self.vault_x.to_account_info(), self.user_x.to_account_info())
        };

        let cpi_program = self.token_program.key();
        let cpi_accounts = Transfer {
            from,
            to,
            authority: self.config.to_account_info(),
        };
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"config",
            &self.config.seed.to_le_bytes(),
            &[self.config.config_bump],
        ]];

        let ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        transfer(ctx, amount)
    }

    // Helper: Vault -> Treasury (Config PDA signs)
    fn pay_protocol_fee(&self, is_x: bool, amount: u64) -> Result<()>{
        let from = if is_x {
            self.vault_x.to_account_info()
        }else{
            self.vault_y.to_account_info()
        };

        let cpi_program = self.token_program.key();
        let cpi_accounts = Transfer{
            from,
            to: self.treasury_token.to_account_info(),
            authority: self.config.to_account_info(),
        };

        let signer_seeds: &[&[&[u8]]] = &[&[b"config",
            &self.config.seed.to_le_bytes(),
            &[self.config.config_bump],
        ]];

        let ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        transfer(ctx, amount)
    }
}