export async function getPrice(): Promise<void> {
  console.log('Getting data from ', readingPubkey.toBase58())
  const priceFeedAccount = "FmAmfoyPXiA8Vhhe6MZTr3U6rZfEZ1ctEHay1ysqCqcf"
  const AggregatorPublicKey = new PublicKey(priceFeedAccount)
  const instruction = new TransactionInstruction({
    keys: [{ pubkey: readingPubkey, isSigner: false, isWritable: true },
    { pubkey: AggregatorPublicKey, isSigner: false, isWritable: false }],
    programId,
    data: Buffer.alloc(0), 
  })
  await sendAndConfirmTransaction(
    connection,
    new Transaction().add(instruction),
    [payer],
  )
}
  
  pub fn process_instruction(
    _program_id: &Pubkey, // Ignored
    accounts: &[AccountInfo], // Public key of the account to read price data from
    _instruction_data: &[u8], // Ignored
) -> ProgramResult {
    msg!("Chainlink Solana Demo program entrypoint");

    let accounts_iter = &mut accounts.iter();
    // This is the account of our our account
    let my_account = next_account_info(accounts_iter)?;
    // This is the account of the data feed for prices
    let feed_account = next_account_info(accounts_iter)?;

    const DECIMALS: u32 = 9;

    let price = chainlink::get_price(&chainlink::id(), feed_account)?;

    if let Some(price) = price {
        let decimal = Decimal::new(price, DECIMALS);
        msg!("Price is {}", decimal);
    } else {
        msg!("No current price");
    }

     // Store the price ourselves
     let mut price_data_account = PriceFeedAccount::try_from_slice(&my_account.data.borrow())?;
     price_data_account.answer = price.unwrap_or(0);
     price_data_account.serialize(&mut &mut my_account.data.borrow_mut()[..])?;


    Ok(())
}
  
  export async function reportPrice(): Promise<void> {
  // const priceFeedAccount = "FmAmfoyPXiA8Vhhe6MZTr3U6rZfEZ1ctEHay1ysqCqcf"
  // const AggregatorPublicKey = new PublicKey(priceFeedAccount)
  const accountInfo = await connection.getAccountInfo(readingPubkey)
  if (accountInfo === null) {
    throw new Error('Error: cannot find the aggregator account')
  }
  const latestPrice = borsh.deserialize(
    AggregatorSchema,
    AggregatorAccount,
    accountInfo.data,
  )
  console.log("Current price of SOL/USD is: ", latestPrice.answer.toString())
}
