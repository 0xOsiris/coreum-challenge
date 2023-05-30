use std::collections::HashMap;

fn main() {
    println!("Hello, Coreum!");
}

// A user can submit a `MultiSend` transaction (similar to bank.MultiSend in cosmos sdk) to transfer multiple
// coins (denoms) from multiple input addresses to multiple output addresses. A denom is the name or symbol
// for a coin type, e.g USDT and USDC can be considered different denoms; in cosmos ecosystem they are called
// denoms, in ethereum world they are called symbols.
// The sum of input coins and output coins must match for every transaction.

#[derive(Clone)]
pub struct MultiSend {
    // inputs contain the list of accounts that want to send coins from, and how many coins from each account we want to send.
    inputs: Vec<Balance>,
    // outputs contains the list of accounts that we want to deposit coins into, and how many coins to deposit into
    // each account
    outputs: Vec<Balance>,
}

impl MultiSend {
    //Validates the summation of i/o are identical.
    pub fn validate_multi_send_tx(&self) -> Result<(), String> {
        let mut multi_send_sum: (i128, i128) = (0, 0);
        //Validate the summations of the i/o on the multi_send_tx prior to continuing
        self.inputs.iter().for_each(|i| {
            multi_send_sum.0 += i.coins.iter().fold(0, |_acc, coin| _acc + coin.amount);
        });
        self.outputs.iter().for_each(|o| {
            multi_send_sum.1 += o.coins.iter().fold(0, |_acc, coin| _acc + coin.amount);
        });

        if multi_send_sum.0 != multi_send_sum.1 {
            Err("Invalid Multi Send Tx".to_string())
        } else {
            Ok(())
        }
    }
}

//Struct holding relevant data to efficiently validate/process the transaction
pub struct TxData {
    multi_send_tx: MultiSend,
    original_balances: Vec<Balance>,
    definitions: Vec<DenomDefinition>,
    non_issuer_input_sum_map: HashMap<String, i128>, //HashMap from denom -> non_issuer_input_sum
    non_issuer_output_sum_map: HashMap<String, i128>, //HashMap from denom -> non_issuer_input_sum
    balances_map: HashMap<String, Balance>,          //Tracks the address balances
    coin_balance_changes_map: HashMap<String, HashMap<String, i128>>, //Tracks the balance changes on an address to a specific coin
    denom_definitions_map: HashMap<String, DenomDefinition>, //Hashmap from denom -> definition
}

impl TxData {
    pub fn new(
        multi_send_tx: MultiSend,
        original_balances: Vec<Balance>,
        definitions: Vec<DenomDefinition>,
    ) -> TxData {
        Self {
            multi_send_tx,
            original_balances,
            definitions,
            non_issuer_input_sum_map: HashMap::new(),
            non_issuer_output_sum_map: HashMap::new(),
            coin_balance_changes_map: HashMap::new(),
            balances_map: HashMap::new(),
            denom_definitions_map: HashMap::new(),
        }
    }
    //Initializes a Hashmap from address to balance
    pub fn initialize_balances_map(&mut self) {
        let mut balances_map = HashMap::new();
        self.original_balances.iter().for_each(|balance| {
            balances_map.insert(balance.address.clone(), balance.clone());
        });

        self.balances_map = balances_map;
    }

    //Initializes HashMap from denom -> definition
    pub fn initialize_definitions_map(&mut self) {
        let mut denominations_map = HashMap::new();
        self.definitions.iter().for_each(|definition| {
            denominations_map.insert(definition.denom.clone(), definition.clone());
        });
        self.denom_definitions_map = denominations_map;
    }

    //Initializes the burn & commission data necessary for burn/commision calculations.
    ///NOTE: Must be called after the prior 2 initialization functions to initialize the HashMaps.
    pub fn initialize_bc_data(&mut self) {
        //Populate non_issuer_input_sum_map
        for input in self.multi_send_tx.inputs.iter() {
            for coin in input.coins.iter() {
                if let Some(definition) = self.denom_definitions_map.get(&coin.denom) {
                    if input.address != definition.issuer {
                        if let Some(amount) =
                            self.non_issuer_input_sum_map.get_mut(&definition.denom)
                        {
                            *amount += coin.amount;
                        } else {
                            self.non_issuer_input_sum_map
                                .insert(definition.denom.clone(), coin.amount);
                        }
                    }
                }
            }
        }

        //Populate non issuer output sum map
        for output in self.multi_send_tx.outputs.iter() {
            for coin in output.coins.iter() {
                if let Some(definition) = self.denom_definitions_map.get(&coin.denom) {
                    if output.address != definition.issuer {
                        if let Some(amount) =
                            self.non_issuer_output_sum_map.get_mut(&definition.denom)
                        {
                            *amount += coin.amount;
                        } else {
                            self.non_issuer_output_sum_map
                                .insert(definition.denom.clone(), coin.amount);
                        }
                    }
                }
            }
        }
    }

    //Collect the nested hashmap into a Vec<Balance>
    pub fn collect_balance_changes(self) -> Vec<Balance> {
        self.coin_balance_changes_map
            .into_iter()
            .map(|(address, v)| Balance {
                address,
                coins: v
                    .into_iter()
                    .map(|(denom, amount)| Coin { denom, amount })
                    .collect::<Vec<Coin>>(),
            })
            .collect::<Vec<Balance>>()
    }
}

#[derive(Clone, Debug)]
pub struct Coin {
    pub denom: String,
    pub amount: i128,
}

#[derive(Clone, Debug)]
pub struct Balance {
    address: String,
    coins: Vec<Coin>,
}

// A Denom has a definition (`CoinDefinition`) which contains different attributes related to the denom:
#[derive(Clone, Debug)]
pub struct DenomDefinition {
    // the unique identifier for the token (e.g `core`, `eth`, `usdt`, etc.)
    denom: String,
    // The address that created the token
    issuer: String,
    // burn_rate is a number between 0 and 1. If it is above zero, in every transfer,
    // some additional tokens will be burnt on top of the transferred value, from the senders address.
    // The tokens to be burnt are calculated by multiplying the TransferAmount by burn rate, and
    // rounding it up to an integer value. For example if an account sends 100 token and burn_rate is
    // 0.2, then 120 (100 + 100 * 0.2) will be deducted from sender account and 100 will be deposited to the recipient
    // account (i.e 20 tokens will be burnt)
    burn_rate: f64,
    // commission_rate is exactly same as the burn_rate, but the calculated value will be transferred to the
    // issuer's account address instead of being burnt.
    commission_rate: f64,
}

// Implement `calculate_balance_changes` with the following requirements.
// - Output of the function is the balance changes that must be applied to different accounts
//   (negative means deduction, positive means addition), or an error. the error indicates that the transaction must be rejected.
// - If sum of inputs and outputs in multi_send_tx does not match the tx must be rejected(i.e return error).

// - Apply burn_rate and commission_rate as described by their definition.
// - If the sender does not have enough balances (in the original_balances) to cover the input amount on top of burn_rate and
// commission_rate, the transaction must be rejected.
// - burn_rate and commission_rate does not apply to the issuer. So to calculate the correct values you must do this for every denom:
//      - sum all the inputs coming from accounts that are not an issuer (let's call it non_issuer_input_sum)
//      - sum all the outputs going to accounts that are not an issuer (let's call it non_issuer_output_sum)
//      - total burn amount is total_burn = min(non_issuer_input_sum, non_issuer_output_sum)
//      - total_burn is distributed between all input accounts as: account_share = roundup(total_burn * input_from_account / non_issuer_input_sum)
//      - total_burn_amount = sum (account_shares) // notice that in previous step we rounded up, so we need to recalculate the total again.
//      - commission_rate is exactly the same, but we send the calculate value to issuer, and not burn.
//      - Example:
//          burn_rate: 10%
//
//          inputs:
//          60, 90
//          25 <-- issuer
//
//          outputs:
//          50
//          100 <-- issuer
//          25
//          In this case burn amount is: min(non_issuer_inputs, non_issuer_outputs) = min(75+75, 50+25) = 75
//          Expected burn: 75 * 10% = 7.5
//          And now we divide it proportionally between all input sender: first_sender_share  = 7.5 * 60 / 150  = 3
//                                                                        second_sender_share = 7.5 * 90 / 150  = 4.5
// - In README.md we have provided more examples to help you better understand the requirements.
// - Write different unit tests to cover all the edge cases, we would like to see how you structure your tests.
//   There are examples in README.md, you can convert them into tests, but you should add more cases.
fn calculate_balance_changes(
    original_balances: Vec<Balance>,
    definitions: Vec<DenomDefinition>,
    multi_send_tx: MultiSend,
) -> Result<Vec<Balance>, String> {
    //First validate the transaction
    multi_send_tx.validate_multi_send_tx()?;

    let mut tx_data = TxData::new(multi_send_tx, original_balances, definitions);

    //Initialize the maps for denoms & balances
    tx_data.initialize_balances_map();
    tx_data.initialize_definitions_map();

    //Populate the commission & burn rate data
    tx_data.initialize_bc_data();

    //Process the inputs accounting for burn/commision rate on sender/issuer
    //Account changes on the inputs
    for input in tx_data.multi_send_tx.inputs.iter() {
        for (idx, coin) in input.coins.iter().enumerate() {
            if let Some(definition) = tx_data.denom_definitions_map.get(&coin.denom) {
                //Only decrease balance by the burn/commission if the address is not the issuer.
                if input.address != definition.issuer {
                    //Get the non_issuer_input_sum & non_issuer_output_sum for the denom
                    let non_issuer_input_sum =
                        tx_data.non_issuer_input_sum_map.get(&coin.denom).unwrap(); //Unwrap since all should be copacetic in the map
                    let non_issuer_output_sum =
                        tx_data.non_issuer_output_sum_map.get(&coin.denom).unwrap(); //Here as well

                    //Calculate the total burn/commission
                    let total_bc = min(*non_issuer_input_sum, *non_issuer_output_sum);
                    //Calculate the commission and burn amount
                    let burn_amount = evaluate_rate(
                        coin.amount,
                        definition.burn_rate,
                        total_bc,
                        *non_issuer_input_sum,
                    );
                    let commission_amount = evaluate_rate(
                        coin.amount,
                        definition.commission_rate,
                        total_bc,
                        *non_issuer_input_sum,
                    );
                    //Ensure the input address has sufficient balance to cover the amount + burn + commision
                    //Unwraping is fine here, as we know the address exists in the map
                    if let Some(_coin) = tx_data
                        .balances_map
                        .get(&input.address)
                        .unwrap()
                        .coins
                        .get(idx)
                    {
                        if _coin.amount < coin.amount + burn_amount + commission_amount {
                            return Err(format!(
                                "Inssuficient wallet balance on {} for coin {}",
                                input.address, coin.denom
                            ));
                        }
                    } else {
                        return Err(format!(
                            "Inssuficient wallet balance on {} for coin {}",
                            input.address, coin.denom
                        ));
                    }

                    //Update the senders balance in the coin_balance_changes hashmap
                    if let Some(coin_map) = tx_data.coin_balance_changes_map.get_mut(&input.address)
                    {
                        if let Some(coin_amount) = coin_map.get_mut(&coin.denom) {
                            *coin_amount += -(burn_amount + commission_amount + coin.amount)
                        } else {
                            coin_map.insert(
                                coin.denom.clone(),
                                -(burn_amount + commission_amount + coin.amount),
                            );
                        }
                    } else {
                        let mut coin_map = HashMap::new();
                        coin_map.insert(
                            coin.denom.clone(),
                            -(burn_amount + commission_amount + coin.amount),
                        );
                        tx_data
                            .coin_balance_changes_map
                            .insert(input.address.clone(), coin_map);
                    }

                    //Update the issuers balance in the coin_balance_changes hashmap
                    if let Some(coin_map) =
                        tx_data.coin_balance_changes_map.get_mut(&definition.issuer)
                    {
                        if let Some(coin_amount) = coin_map.get_mut(&coin.denom) {
                            *coin_amount += commission_amount
                        } else if commission_amount != 0 {
                            coin_map.insert(coin.denom.clone(), commission_amount);
                        }
                    } else if commission_amount != 0 {
                        let mut coin_map = HashMap::new();
                        coin_map.insert(coin.denom.clone(), commission_amount);
                        tx_data
                            .coin_balance_changes_map
                            .insert(definition.issuer.clone(), coin_map);
                    }
                } else {
                    //Update the issuers balance in the coin_balance_changes hashmap
                    //If the issuer is sending the tokens simply decrease the balance by the amount spent
                    if let Some(coin_map) = tx_data.coin_balance_changes_map.get_mut(&input.address)
                    {
                        if let Some(coin_amount) = coin_map.get_mut(&coin.denom) {
                            *coin_amount -= coin.amount
                        } else {
                            coin_map.insert(coin.denom.clone(), coin.amount);
                        }
                    } else {
                        let mut coin_map = HashMap::new();
                        coin_map.insert(coin.denom.clone(), -coin.amount);
                        tx_data
                            .coin_balance_changes_map
                            .insert(input.address.clone(), coin_map);
                    }
                }
            }
        }
    }

    //Process the output amounts
    for output in tx_data.multi_send_tx.outputs.iter() {
        for coin in output.coins.iter() {
            //Update the senders balance in the coin_balance_changes hashmap
            if let Some(coin_map) = tx_data.coin_balance_changes_map.get_mut(&output.address) {
                if let Some(coin_amount) = coin_map.get_mut(&coin.denom) {
                    *coin_amount += coin.amount
                } else {
                    coin_map.insert(coin.denom.clone(), coin.amount);
                }
            } else {
                let mut coin_map = HashMap::new();
                coin_map.insert(coin.denom.clone(), coin.amount);
                tx_data
                    .coin_balance_changes_map
                    .insert(output.address.clone(), coin_map);
            }
        }
    }

    //Return the processed balances as a vector.
    Ok(tx_data.collect_balance_changes())
}

fn min(a: i128, b: i128) -> i128 {
    if a < b {
        a
    } else {
        b
    }
}

//roundup(total_burn * input_from_account / non_issuer_input_sum)
fn evaluate_rate(amount: i128, rate: f64, total_amount: i128, non_issuer_input_sum: i128) -> i128 {
    roundup((total_amount as f64 * rate) * amount as f64 / non_issuer_input_sum as f64)
}

//Helper function to round up an f64 to an i128
fn roundup(n: f64) -> i128 {
    (n + 0.5) as i128
}

#[cfg(test)]
mod tests {
    use crate::calculate_balance_changes;
    use crate::{Balance, Coin, DenomDefinition, MultiSend};
    use std::collections::HashMap;
    use std::error::Error;

    #[test]
    pub fn test_invalid_sum() -> Result<(), Box<dyn Error>> {
        let (original_balances, definitions, multi_send) = initialize_invalid_sum_data();
        assert_eq!(
            calculate_balance_changes(original_balances, definitions, multi_send).err(),
            Some("Invalid Multi Send Tx".to_string())
        );
        Ok(())
    }

    #[test]
    //NOTE: Example #1 from README
    pub fn test_no_issuer_on_sender_or_receiver() -> Result<(), Box<dyn Error>> {
        let (original_balances, definitions, multi_send) =
            initialize_no_issuer_on_sender_or_receiver();
        let mut assertion_map = HashMap::new();
        assertion_map.insert(
            "account_recipient".to_string(),
            vec![
                Coin {
                    denom: "denom1".to_string(),
                    amount: 1000,
                },
                Coin {
                    denom: "denom2".to_string(),
                    amount: 1000,
                },
            ],
        );
        assertion_map.insert(
            "issuer_account_A".to_string(),
            vec![Coin {
                denom: "denom1".to_string(),
                amount: 120,
            }],
        );
        assertion_map.insert(
            "account1".to_string(),
            vec![Coin {
                denom: "denom1".to_string(),
                amount: -1200,
            }],
        );
        assertion_map.insert(
            "account2".to_string(),
            vec![Coin {
                denom: "denom2".to_string(),
                amount: -2000,
            }],
        );

        let balance_changes =
            calculate_balance_changes(original_balances, definitions, multi_send).unwrap();
        for balance_change in balance_changes.iter() {
            assertion_map
                .get(&balance_change.address)
                .unwrap()
                .iter()
                .zip(balance_change.coins.clone())
                .for_each(|(assertion_coin, coin)| {
                    assert_eq!(assertion_coin.amount, coin.amount);
                })
        }
        Ok(())
    }
    #[test]
    ///NOTE: Example #2
    pub fn test_issuer_exists_on_sender_receiver() -> Result<(), Box<dyn Error>> {
        let (original_balances, definitions, multi_send) =
            initialize_issuer_exists_on_sender_receiver();
        let mut assertion_map = HashMap::new();
        assertion_map.insert(
            "account_recipient".to_string(),
            vec![Coin {
                denom: "denom1".to_string(),
                amount: 500,
            }],
        );
        assertion_map.insert(
            "issuer_account_A".to_string(),
            vec![Coin {
                denom: "denom1".to_string(),
                amount: 560,
            }],
        );
        assertion_map.insert(
            "account1".to_string(),
            vec![Coin {
                denom: "denom1".to_string(),
                amount: -715,
            }],
        );
        assertion_map.insert(
            "account2".to_string(),
            vec![Coin {
                denom: "denom1".to_string(),
                amount: -385,
            }],
        );

        let balance_changes =
            calculate_balance_changes(original_balances, definitions, multi_send).unwrap();
        for balance_change in balance_changes.iter() {
            assertion_map
                .get(&balance_change.address)
                .unwrap()
                .iter()
                .zip(balance_change.coins.clone())
                .for_each(|(assertion_coin, coin)| {
                    assert_eq!(assertion_coin.amount, coin.amount);
                })
        }
        Ok(())
    }
    #[test]
    ///NOTE: Example #3
    pub fn test_insufficient_balance() -> Result<(), Box<dyn Error>> {
        let (original_balances, definitions, multi_send) = initialize_insufficient_balance_data();
        assert_eq!(
            calculate_balance_changes(original_balances, definitions, multi_send).err(),
            Some(format!(
                "Inssuficient wallet balance on {} for coin {}",
                "account1", "denom1"
            ))
        );
        Ok(())
    }

    //Test setup helper functions
    fn initialize_insufficient_balance_data() -> (Vec<Balance>, Vec<DenomDefinition>, MultiSend) {
        let mut original_balances: Vec<Balance> = vec![];
        let mut definitions: Vec<DenomDefinition> = vec![];

        original_balances.push(Balance {
            address: "account1".to_string(),
            coins: vec![],
        });
        definitions.push(DenomDefinition {
            denom: "denom1".to_string(),
            issuer: "issuer_account_A".to_string(),
            burn_rate: 0_f64,
            commission_rate: 0_f64,
        });
        let multi_send: MultiSend = MultiSend {
            inputs: vec![Balance {
                address: "account1".to_string(),
                coins: vec![Coin {
                    denom: "denom1".to_string(),
                    amount: 350,
                }],
            }],
            outputs: vec![Balance {
                address: "account_recipient".to_string(),
                coins: vec![Coin {
                    denom: "denom1".to_string(),
                    amount: 350,
                }],
            }],
        };

        (original_balances, definitions, multi_send)
    }

    //Test setup helper functions
    fn initialize_invalid_sum_data() -> (Vec<Balance>, Vec<DenomDefinition>, MultiSend) {
        let mut original_balances: Vec<Balance> = vec![];
        let mut definitions: Vec<DenomDefinition> = vec![];

        original_balances.push(Balance {
            address: "account1".to_string(),
            coins: vec![Coin {
                denom: "denom1".to_string(),
                amount: 1_000_000,
            }],
        });
        definitions.push(DenomDefinition {
            denom: "denom1".to_string(),
            issuer: "issuer_account_A".to_string(),
            burn_rate: 0_f64,
            commission_rate: 0_f64,
        });
        let multi_send: MultiSend = MultiSend {
            inputs: vec![Balance {
                address: "account1".to_string(),
                coins: vec![Coin {
                    denom: "denom1".to_string(),
                    amount: 350,
                }],
            }],
            outputs: vec![Balance {
                address: "account_recipient".to_string(),
                coins: vec![Coin {
                    denom: "denom1".to_string(),
                    amount: 450,
                }],
            }],
        };

        (original_balances, definitions, multi_send)
    }

    fn initialize_no_issuer_on_sender_or_receiver(
    ) -> (Vec<Balance>, Vec<DenomDefinition>, MultiSend) {
        let mut original_balances: Vec<Balance> = vec![];
        let mut definitions: Vec<DenomDefinition> = vec![];

        original_balances.push(Balance {
            address: "account1".to_string(),
            coins: vec![Coin {
                denom: "denom1".to_string(),
                amount: 1_000_000,
            }],
        });
        original_balances.push(Balance {
            address: "account2".to_string(),
            coins: vec![Coin {
                denom: "denom2".to_string(),
                amount: 1_000_000,
            }],
        });
        definitions.push(DenomDefinition {
            denom: "denom1".to_string(),
            issuer: "issuer_account_A".to_string(),
            burn_rate: 0.08_f64,
            commission_rate: 0.12_f64,
        });
        definitions.push(DenomDefinition {
            denom: "denom2".to_string(),
            issuer: "issuer_account_B".to_string(),
            burn_rate: 1_f64,
            commission_rate: 0_f64,
        });
        let multi_send: MultiSend = MultiSend {
            inputs: vec![
                Balance {
                    address: "account1".to_string(),
                    coins: vec![Coin {
                        denom: "denom1".to_string(),
                        amount: 1000,
                    }],
                },
                Balance {
                    address: "account2".to_string(),
                    coins: vec![Coin {
                        denom: "denom2".to_string(),
                        amount: 1000,
                    }],
                },
            ],
            outputs: vec![Balance {
                address: "account_recipient".to_string(),
                coins: vec![
                    Coin {
                        denom: "denom1".to_string(),
                        amount: 1000,
                    },
                    Coin {
                        denom: "denom2".to_string(),
                        amount: 1000,
                    },
                ],
            }],
        };

        (original_balances, definitions, multi_send)
    }

    fn initialize_issuer_exists_on_sender_receiver(
    ) -> (Vec<Balance>, Vec<DenomDefinition>, MultiSend) {
        let mut original_balances: Vec<Balance> = vec![];
        let mut definitions: Vec<DenomDefinition> = vec![];

        original_balances.push(Balance {
            address: "account1".to_string(),
            coins: vec![Coin {
                denom: "denom1".to_string(),
                amount: 1_000_000,
            }],
        });
        original_balances.push(Balance {
            address: "account2".to_string(),
            coins: vec![Coin {
                denom: "denom2".to_string(),
                amount: 1_000_000,
            }],
        });
        definitions.push(DenomDefinition {
            denom: "denom1".to_string(),
            issuer: "issuer_account_A".to_string(),
            burn_rate: 0.08_f64,
            commission_rate: 0.12_f64,
        });

        let multi_send: MultiSend = MultiSend {
            inputs: vec![
                Balance {
                    address: "account1".to_string(),
                    coins: vec![Coin {
                        denom: "denom1".to_string(),
                        amount: 650,
                    }],
                },
                Balance {
                    address: "account2".to_string(),
                    coins: vec![Coin {
                        denom: "denom1".to_string(),
                        amount: 350,
                    }],
                },
            ],
            outputs: vec![
                Balance {
                    address: "account_recipient".to_string(),
                    coins: vec![Coin {
                        denom: "denom1".to_string(),
                        amount: 500,
                    }],
                },
                Balance {
                    address: "issuer_account_A".to_string(),
                    coins: vec![Coin {
                        denom: "denom1".to_string(),
                        amount: 500,
                    }],
                },
            ],
        };

        (original_balances, definitions, multi_send)
    }
}
