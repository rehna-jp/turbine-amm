use {
    anchor_lang::{InstructionData, ToAccountMetas},
    anchor_spl::associated_token,
    litesvm::LiteSVM,
    litesvm_token::CreateMint,
    solana_keypair::Keypair,
    solana_message::{Instruction, Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};



mod ix_handlers;
use ix_handlers::*;

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> litesvm::types::TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

// Setup function to initialize LiteSVM and create a payer keypair
fn setup() -> (
    LiteSVM,
    Keypair,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let program_id = ammq32026::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/ammq32026.so"
    ));

    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
    // This done using litesvm-token's CreateMint utility which creates the mint in the LiteSVM environment
    let mint_x = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();

    let mint_y = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();

    let config =
        Pubkey::find_program_address(&[b"config", &123u64.to_le_bytes()], &ammq32026::id()).0;
    let mint_lp = Pubkey::find_program_address(&[b"lp", config.as_ref()], &ammq32026::id()).0;

    // Derive the PDA for the vault associated token account using the config PDA and Mint A
    let vault_x = associated_token::get_associated_token_address(&config, &mint_x);
    let vault_y = associated_token::get_associated_token_address(&config, &mint_y);

    (
        svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    )
}

#[test]
fn test_initialize() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let instruction = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );
    let res = send(&mut svm, &[instruction], &payer, &[&payer]);
    assert!(res.is_ok());
}

#[test]
pub fn test_deposit() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );

    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]);
    assert!(res.is_ok());
}

#[test]
pub fn test_remove_liquidity() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );

    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let remove_liquidity_ix = create_remove_liquidity_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix, remove_liquidity_ix], &payer, &[&payer]);
    assert!(res.is_ok());


}

#[test]
pub fn test_swap(){
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );

    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]);
    assert!(res.is_ok());


    // 2. Create a separate swapper/trader user
    let swapper = Keypair::new();
    svm.airdrop(&swapper.pubkey(), 1_000_000_000).unwrap();
    // Create token accounts for swapper and mint them some Token X to trade
    let swapper_x = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_x)
        .owner(&swapper.pubkey())
        .send()
        .unwrap();
    litesvm_token::MintTo::new(&mut svm, &payer, &mint_x, &swapper_x, 100_000_000)
        .send()
        .unwrap();
    let _swapper_y = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_y)
        .owner(&swapper.pubkey())
        .send()
        .unwrap();

    let treasury_token = anchor_spl::associated_token::get_associated_token_address(&payer.pubkey(), &mint_x);

    let swap_ix= create_swap_ix(
        &mut svm,
        &swapper,
        mint_x,
        mint_y,
        config,
        vault_x,
        vault_y,
        treasury_token,
        true,
        10_000_000,
        0,
    );

    let res = send(&mut svm, &[swap_ix], &swapper, &[&swapper]);
    assert!(res.is_ok(), "Swap transaction failed: {:?}", res.err());
}

#[test]
pub fn test_swap_slippage_exceeded() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );
    send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]).unwrap();

    let swapper = Keypair::new();
    svm.airdrop(&swapper.pubkey(), 1_000_000_000).unwrap();

    let swapper_x = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &swapper, &mint_x)
        .owner(&swapper.pubkey())
        .send()
        .unwrap();

    litesvm_token::MintTo::new(&mut svm, &payer, &mint_x, &swapper_x, 100_000_000)
        .send()
        .unwrap();

    let _swapper_y = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &swapper, &mint_y)
        .owner(&swapper.pubkey())
        .send()
        .unwrap();

    let treasury_token = anchor_spl::associated_token::get_associated_token_address(
        &payer.pubkey(),
        &mint_x,
    );

    let swap_ix = create_swap_ix(
        &mut svm,
        &swapper,
        mint_x,
        mint_y,
        config,
        vault_x,
        vault_y,
        treasury_token,
        true,
        10_000_000,
        999_999_999, // <--- Impossible min_out! Must fail
    );

    let res = send(&mut svm, &[swap_ix], &swapper, &[&swapper]);
    assert!(res.is_err(), "Swap should fail when slippage is exceeded");
}

#[test]
pub fn test_swap_zero_amount() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );
    send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]).unwrap();

    let swapper = Keypair::new();
    svm.airdrop(&swapper.pubkey(), 1_000_000_000).unwrap();

    let swapper_x = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &swapper, &mint_x)
        .owner(&swapper.pubkey())
        .send()
        .unwrap();

    litesvm_token::MintTo::new(&mut svm, &payer, &mint_x, &swapper_x, 100_000_000)
        .send()
        .unwrap();

    let _swapper_y = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &swapper, &mint_y)
        .owner(&swapper.pubkey())
        .send()
        .unwrap();

    let treasury_token = anchor_spl::associated_token::get_associated_token_address(
        &payer.pubkey(),
        &mint_x,
    );

    let swap_ix = create_swap_ix(
        &mut svm,
        &swapper,
        mint_x,
        mint_y,
        config,
        vault_x,
        vault_y,
        treasury_token,
        true,
        0, // <--- 0 amount! Must fail
        0,
    );

    let res = send(&mut svm, &[swap_ix], &swapper, &[&swapper]);
    assert!(res.is_err(), "Swap should fail when amount is zero");
}

#[test]
pub fn test_remove_liquidity_slippage_exceeded() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let user = payer.pubkey();
    let user_x = anchor_spl::associated_token::get_associated_token_address(&user, &mint_x);
    let user_y = anchor_spl::associated_token::get_associated_token_address(&user, &mint_y);
    let user_lp = anchor_spl::associated_token::get_associated_token_address(&user, &mint_lp);

    // Create remove liquidity instruction with impossible min_x/min_y
    let remove_ix = solana_message::Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::RemoveLiquidity {
            lp_amount: 50_000_000,
            min_x: 999_999_999, // <--- Impossible min_x! Must fail
            min_y: 999_999_999,
        }
        .data(),
        ammq32026::accounts::RemoveLiquidity {
            user,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = send(&mut svm, &[init_ix, deposit_ix, remove_ix], &payer, &[&payer]);
    assert!(res.is_err(), "Remove liquidity should fail when min amounts exceed proportional share");
}

#[test]
pub fn test_deposit_zero_amount() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );

    let user = payer.pubkey();
    let user_x = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_x)
        .owner(&user).send().unwrap();
    litesvm_token::MintTo::new(&mut svm, &payer, &mint_x, &user_x, 100_000_000).send().unwrap();

    let user_y = litesvm_token::CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_y)
        .owner(&user).send().unwrap();
    litesvm_token::MintTo::new(&mut svm, &payer, &mint_y, &user_y, 100_000_000).send().unwrap();

    let user_lp = anchor_spl::associated_token::get_associated_token_address(&user, &mint_lp);

    let deposit_ix = solana_message::Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Deposit {
            amount: 0, // <--- Zero amount! Must fail
            max_x: 10_000_000,
            max_y: 10_000_000,
        }
        .data(),
        ammq32026::accounts::Deposit {
            user,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]);
    assert!(res.is_err(), "Deposit should fail when amount is zero");
}

#[test]
pub fn test_deposit_slippage_exceeded() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );
    // Initial deposit establishes the pool reserves
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );
    send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]).unwrap();

    let user = payer.pubkey();
    let user_x = anchor_spl::associated_token::get_associated_token_address(&user, &mint_x);
    let user_y = anchor_spl::associated_token::get_associated_token_address(&user, &mint_y);
    let user_lp = anchor_spl::associated_token::get_associated_token_address(&user, &mint_lp);

    // Second deposit: wants 50M LP tokens, but sets max_x to only 10 tokens (way too low)
    let bad_deposit_ix = solana_message::Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Deposit {
            amount: 50_000_000,
            max_x: 10, // <--- Slippage limit too tight! Must fail
            max_y: 100_000_000,
        }
        .data(),
        ammq32026::accounts::Deposit {
            user,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = send(&mut svm, &[bad_deposit_ix], &payer, &[&payer]);
    assert!(res.is_err(), "Deposit should fail when required tokens exceed max_x / max_y");
}

#[test]
pub fn test_remove_liquidity_zero_amount() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let user = payer.pubkey();
    let user_x = anchor_spl::associated_token::get_associated_token_address(&user, &mint_x);
    let user_y = anchor_spl::associated_token::get_associated_token_address(&user, &mint_y);
    let user_lp = anchor_spl::associated_token::get_associated_token_address(&user, &mint_lp);

    let remove_ix = solana_message::Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::RemoveLiquidity {
            lp_amount: 0, // <--- Zero LP! Must fail
            min_x: 0,
            min_y: 0,
        }
        .data(),
        ammq32026::accounts::RemoveLiquidity {
            user,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = send(&mut svm, &[init_ix, deposit_ix, remove_ix], &payer, &[&payer]);
    assert!(res.is_err(), "Remove liquidity should fail when lp_amount is zero");
}

#[test]
pub fn test_remove_liquidity_insufficient_lp() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    );
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let user = payer.pubkey();
    let user_x = anchor_spl::associated_token::get_associated_token_address(&user, &mint_x);
    let user_y = anchor_spl::associated_token::get_associated_token_address(&user, &mint_y);
    let user_lp = anchor_spl::associated_token::get_associated_token_address(&user, &mint_lp);

    // User only received 100M LP tokens from deposit, but tries to withdraw 1 Billion
    let remove_ix = solana_message::Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::RemoveLiquidity {
            lp_amount: 1_000_000_000, // <--- Way more than user owns!
            min_x: 0,
            min_y: 0,
        }
        .data(),
        ammq32026::accounts::RemoveLiquidity {
            user,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x,
            vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = send(&mut svm, &[init_ix, deposit_ix, remove_ix], &payer, &[&payer]);
    assert!(res.is_err(), "Remove liquidity should fail when user has insufficient LP balance");
}