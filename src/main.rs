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
struct MultiSend {
    // inputs contain the list of accounts that want to send coins from, and how many coins from each account we want to send.
    inputs: Vec<Balance>,
    // outputs contains the list of accounts that we want to deposit coins into, and how many coins to deposit into
    // each account
    outputs: Vec<Balance>,
}
#[derive(Clone)]
pub struct Coin {
    pub denom: String,
    pub amount: i128,
}
#[derive(Clone)]
struct Balance {
    address: String,
    coins: Vec<Coin>,
}

// A Denom has a definition (`CoinDefinition`) which contains different attributes related to the denom:
struct DenomDefinition {
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
    let inputs = &multi_send_tx.inputs;
    //Validate the summations of the i/o on the multi_send_tx prior to continuing
    let mut multi_send_sum: (i128, i128) = (0, 0);

    multi_send_tx
        .inputs
        .iter()
        .zip(&multi_send_tx.outputs)
        .for_each(|(i, o)| {
            multi_send_sum.0 += i.coins.iter().fold(0, |_acc, coin| _acc + coin.amount);
            multi_send_sum.1 += o.coins.iter().fold(0, |_acc, coin| _acc + coin.amount);
        });

    if multi_send_sum.0 == multi_send_sum.1 {
        //Get a mutable reference to original_balances to return on completion.
        let mut processed_balances: Vec<Balance> = original_balances;
        //Accumulate HashMaps of balances & deniminations for easy lookup
        let balances_map = accumulate_balances_map(processed_balances.clone());
        let denominations_map = accumulate_denominations_map(&definitions);

        //Process the inputs accounting for burn/commision rate on sender/issuer
        for (i, input) in inputs.iter().enumerate() {
            for (j, coin) in input.coins.iter().enumerate() {
                if let Some(definition) = denominations_map.get(&coin.denom) {
                    if definition.issuer != input.address {
                        //Calculate the burn/commission amount as per the definition
                        let burn_amount = round(coin.amount as f64 * definition.burn_rate);
                        let commision_amount =
                            round(coin.amount as f64 * definition.commission_rate);
                        //Unwrap here since we know the address exists in the balances_map
                        let current_balance =
                            balances_map.get(&input.address).unwrap().coins[j].amount;
                        //Ensure the input.address has sufficient balance on the corresponding "coin" denom
                        if current_balance < coin.amount + burn_amount + commision_amount {
                            return Err(format!(
                                "Insufficient balance on {} in account {}",
                                coin.denom, input.address
                            ));
                        }
                        //Update the processed balances of the token sender
                        processed_balances[i].coins[j].amount =
                            -(burn_amount + commision_amount + coin.amount);

                        //Update the issuer balance based on the commission rate.
                        //TODO: This logic is pretty ugly, try and clean this up/speed up time complexity significantly
                        if commision_amount != 0 {
                            if let Some(issuer_balance) = balances_map.get(&definition.issuer) {
                                let issuer_coin_index = if let Some(coin) = issuer_balance
                                    .coins
                                    .iter()
                                    .enumerate()
                                    .find(|(i, c)| c.denom == definition.denom)
                                {
                                    i
                                } else {
                                    //TODO: Probably return an error here
                                    //This will never hit if the issuer is supplied in the MultiSend
                                    return Err("Issuer not found".to_string());
                                };

                                //Index of issuer balance in processed balances
                                let issuer_balance_index = if let Some(index) = processed_balances
                                    .iter()
                                    .enumerate()
                                    .find(|(i, balance)| balance.address == issuer_balance.address)
                                {
                                    i
                                } else {
                                    //TODO: Probably return an error here
                                    //This will never hit if the issuer is supplied in the MultiSend
                                    return Err("Issuer not found".to_string());
                                };

                                processed_balances[issuer_balance_index].coins[issuer_coin_index]
                                    .amount = commision_amount;
                            }
                        }
                    }
                }
            }
        }

        for (i, output) in multi_send_tx.outputs.iter().enumerate() {
            for (j, coin) in output.coins.iter().enumerate() {
                //Update the processed balances of the token receiver
                processed_balances[i].coins[j].amount =
                    balances_map.get(&output.address).unwrap().coins[j].amount + coin.amount;
            }
        }
        Ok(processed_balances)
    } else {
        return Err("Invalid Multi Send Tx".to_string());
    }
}

//Helper function to accumulate a HashMap from Address -> Balance
fn accumulate_balances_map(original_balances: Vec<Balance>) -> HashMap<String, Balance> {
    let mut balances_map = HashMap::new();
    original_balances.iter().for_each(|balance| {
        balances_map.insert(balance.address.clone(), balance.clone());
    });

    balances_map
}

//Helper function to accumulate a HashMap from denom -> DenomDefinition
fn accumulate_denominations_map(
    definitions: &Vec<DenomDefinition>,
) -> HashMap<String, &DenomDefinition> {
    let mut denominations_map = HashMap::new();
    definitions.iter().for_each(|definition| {
        denominations_map.insert(definition.denom.clone(), definition);
    });
    denominations_map
}

//Helper function to round up an f64 to an i128
fn round(n: f64) -> i128 {
    (n + 0.5) as i128
}
#[cfg(test)]
mod tests {
    use crate::{Balance, Coin, DenomDefinition, MultiSend};
    use std::error::Error;
    fn initialize_invalid_sum_data() -> (Vec<Balance>, Vec<DenomDefinition>, MultiSend) {
        let mut original_balances: Vec<Balance> = vec![];
        let mut definitions: Vec<DenomDefinition> = vec![];

        original_balances.push(Balance {
            address: "account1".to_string(),
            coins: vec![Coin {
                denom: "denom1".to_string(),
                amount: 1000_000,
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

    #[test]
    pub fn test_invalid_sum() -> Result<(), Box<dyn Error>> {
        use crate::calculate_balance_changes;

        let (original_balances, definitions, multi_send) = initialize_invalid_sum_data();
        assert_eq!(
            calculate_balance_changes(original_balances, definitions, multi_send).err(),
            Some("Invalid Multi Send Tx".to_string())
        );
        Ok(())
    }
}
