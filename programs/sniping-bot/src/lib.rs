use anchor_lang::prelude::*;

declare_id!("J56WFGghoXjWE9zD1xSsk3bEEk2bMjhWNUr3e5Sp4vJN");

#[program]
pub mod sniping_bot {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
