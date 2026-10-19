//! Проверяет только ответы доверенного RPC с encoding=json и commitment=finalized.
//! Не подтверждает финальность самостоятельно и не принимает данные от плательщика.
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ATA: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const SYSTEM: &str = "11111111111111111111111111111111";
const COMPUTE: &str = "ComputeBudget111111111111111111111111111111";
const MEMO: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
type Check<T> = Result<T, String>;

pub struct ExpectedPayment<'a> {
    pub signature: &'a str,
    pub mint: &'a str,
    pub token_program: &'a str,
    pub merchant_wallet: &'a str,
    pub recipient_ata: &'a str,
    pub reference: &'a str,
    pub amount_raw: u64,
    pub decimals: u8,
}
#[derive(Debug)]
pub struct VerifiedPayment {
    pub signature: String,
    pub slot: u64,
}
struct Key<'a> {
    address: &'a str,
    signer: bool,
    writable: bool,
}
struct Instruction {
    program: usize,
    accounts: Vec<usize>,
    data: Vec<u8>,
}
#[derive(PartialEq)]
struct Balance<'a> {
    mint: &'a str,
    owner: &'a str,
    program: &'a str,
    decimals: u64,
    amount: u64,
}
fn demand(condition: bool, message: &str) -> Check<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn number(value: &Value) -> Check<usize> {
    value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| "Missing integer metadata".into())
}
fn array(value: &Value) -> Check<&Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| "Missing array metadata".into())
}
fn address(value: &Value) -> Check<&str> {
    let text = value.as_str().ok_or("Missing account address")?;
    text.parse::<solana_pubkey::Pubkey>()
        .map_err(|_| "Invalid account address")?;
    Ok(text)
}
fn instruction(value: &Value, key_count: usize) -> Check<Instruction> {
    let program = number(&value["programIdIndex"])?;
    let accounts = array(&value["accounts"])?
        .iter()
        .map(number)
        .collect::<Check<Vec<_>>>()?;
    demand(
        program < key_count && accounts.iter().all(|i| *i < key_count),
        "Invalid instruction account index",
    )?;
    let data = bs58::decode(value["data"].as_str().ok_or("Missing instruction data")?)
        .into_vec()
        .map_err(|_| "Invalid instruction data")?;
    Ok(Instruction {
        program,
        accounts,
        data,
    })
}
fn keys<'a>(tx: &'a Value) -> Check<Vec<Key<'a>>> {
    let message = &tx["transaction"]["message"];
    let static_keys = array(&message["accountKeys"])?;
    let required = number(&message["header"]["numRequiredSignatures"])?;
    let signed_readonly = number(&message["header"]["numReadonlySignedAccounts"])?;
    let unsigned_readonly = number(&message["header"]["numReadonlyUnsignedAccounts"])?;
    demand(
        required > 0
            && required <= static_keys.len()
            && signed_readonly <= required
            && unsigned_readonly <= static_keys.len() - required,
        "Invalid message header",
    )?;
    demand(
        array(&tx["transaction"]["signatures"])?.len() == required,
        "Signature count mismatch",
    )?;
    let mut result = static_keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            Ok(Key {
                address: address(key)?,
                signer: i < required,
                writable: if i < required {
                    i < required - signed_readonly
                } else {
                    i < static_keys.len() - unsigned_readonly
                },
            })
        })
        .collect::<Check<Vec<_>>>()?;
    match &tx["version"] {
        Value::String(version) if version == "legacy" => {
            if let Some(loaded) = tx["meta"].get("loadedAddresses") {
                demand(
                    array(&loaded["writable"])?.is_empty()
                        && array(&loaded["readonly"])?.is_empty(),
                    "Legacy transaction has loaded addresses",
                )?;
            }
        }
        Value::Number(version) if version.as_u64() == Some(0) => {
            let lookups = array(&message["addressTableLookups"])?;
            for (field, writable) in [("writable", true), ("readonly", false)] {
                let loaded = array(&tx["meta"]["loadedAddresses"][field])?;
                let count = lookups.iter().try_fold(0usize, |n, lookup| -> Check<_> {
                    address(&lookup["accountKey"])?;
                    let indices = array(&lookup[format!("{field}Indexes")])?;
                    for index in indices {
                        demand(number(index)? <= 255, "Invalid lookup index")?;
                    }
                    n.checked_add(indices.len())
                        .ok_or_else(|| "Lookup size overflow".into())
                })?;
                demand(count == loaded.len(), "Unresolved address table")?;
                for key in loaded {
                    result.push(Key {
                        address: address(key)?,
                        signer: false,
                        writable,
                    });
                }
            }
        }
        _ => return Err("Unsupported transaction version; manual review required".into()),
    }
    let mut seen = HashSet::new();
    demand(
        result.len() <= 256 && result.iter().all(|key| seen.insert(key.address)),
        "Duplicate or excessive account keys",
    )?;
    Ok(result)
}
fn balances<'a>(value: &'a Value, keys: &[Key<'_>]) -> Check<HashMap<usize, Balance<'a>>> {
    let mut result = HashMap::new();
    for row in array(value)? {
        let index = number(&row["accountIndex"])?;
        demand(index < keys.len(), "Invalid token balance index")?;
        let balance = Balance {
            mint: address(&row["mint"])?,
            owner: address(&row["owner"])?,
            program: address(&row["programId"])?,
            decimals: row["uiTokenAmount"]["decimals"]
                .as_u64()
                .ok_or("Missing token decimals")?,
            amount: row["uiTokenAmount"]["amount"]
                .as_str()
                .ok_or("Missing raw token balance")?
                .parse()
                .map_err(|_| "Invalid raw token balance")?,
        };
        demand(
            result.insert(index, balance).is_none(),
            "Duplicate token balance metadata",
        )?;
    }
    Ok(result)
}
fn expected_balance(row: &Balance<'_>, expected: &ExpectedPayment<'_>) -> Check<()> {
    demand(
        row.mint == expected.mint
            && row.program == expected.token_program
            && row.decimals == u64::from(expected.decimals),
        "Token mint/program/decimals mismatch",
    )
}
fn same_identity(pre: &Balance<'_>, post: &Balance<'_>) -> bool {
    pre.mint == post.mint
        && pre.program == post.program
        && pre.decimals == post.decimals
        && pre.owner == post.owner
}
fn validate_ata(ix: &Instruction, keys: &[Key<'_>], expected: &ExpectedPayment<'_>) -> Check<()> {
    demand(
        ix.accounts.len() == 6 && (ix.data.is_empty() || ix.data == [0] || ix.data == [1]),
        "Unsupported associated account instruction",
    )?;
    let a = &ix.accounts;
    demand(
        keys[a[0]].signer
            && keys[a[0]].writable
            && keys[a[1]].writable
            && keys[a[1]].address == expected.recipient_ata
            && keys[a[2]].address == expected.merchant_wallet
            && keys[a[3]].address == expected.mint
            && keys[a[4]].address == SYSTEM
            && keys[a[5]].address == TOKEN,
        "Associated account creation mismatch",
    )
}
fn validate_ata_inner(
    ix: &Instruction,
    keys: &[Key<'_>],
    expected: &ExpectedPayment<'_>,
    payer: usize,
) -> Check<()> {
    let a = &ix.accounts;
    let valid = match keys[ix.program].address {
        SYSTEM => {
            // Единственный допустимый system CPI: создание конкретного ATA.
            ix.data.len() == 52
                && ix.data[..4] == [0, 0, 0, 0]
                && a.len() == 2
                && a[0] == payer
                && keys[a[1]].address == expected.recipient_ata
                && u64::from_le_bytes(ix.data[12..20].try_into().unwrap()) == 165
                && bs58::encode(&ix.data[20..52]).into_string() == TOKEN
        }
        TOKEN => match ix.data.first() {
            Some(21) => {
                (ix.data.len() == 1 || ix.data == [21, 7, 0])
                    && a.len() == 1
                    && keys[a[0]].address == expected.mint
            }
            Some(22) => {
                ix.data.len() == 1 && a.len() == 1 && keys[a[0]].address == expected.recipient_ata
            }
            Some(18) => {
                ix.data.len() == 33
                    && a.len() == 2
                    && keys[a[0]].address == expected.recipient_ata
                    && keys[a[1]].address == expected.mint
                    && bs58::encode(&ix.data[1..]).into_string() == expected.merchant_wallet
            }
            _ => false,
        },
        _ => false,
    };
    demand(
        valid,
        "Unexpected inner instruction; manual review required",
    )
}

pub fn verify_transaction(tx: &Value, expected: &ExpectedPayment<'_>) -> Check<VerifiedPayment> {
    demand(
        expected.token_program == TOKEN && expected.decimals == 6 && expected.amount_raw > 0,
        "Invalid expected payment",
    )?;
    let merchant = expected
        .merchant_wallet
        .parse()
        .map_err(|_| "Invalid merchant address")?;
    let mint = expected.mint.parse().map_err(|_| "Invalid mint address")?;
    let program = expected
        .token_program
        .parse()
        .map_err(|_| "Invalid token program")?;
    let canonical = spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&merchant, &mint, &program);
    demand(
        canonical.to_string() == expected.recipient_ata
            && expected.reference != expected.recipient_ata
            && expected.reference != expected.mint,
        "Invalid canonical recipient/reference",
    )?;
    demand(
        tx["transaction"]["signatures"][0].as_str() == Some(expected.signature),
        "Signature mismatch",
    )?;
    demand(
        tx.get("meta").is_some_and(Value::is_object) && tx["meta"].get("err") == Some(&Value::Null),
        "Missing metadata or failed transaction",
    )?;
    let slot = tx["slot"].as_u64().ok_or("Missing slot")?;
    let keys = keys(tx)?;
    let recipient = keys
        .iter()
        .position(|key| key.address == expected.recipient_ata)
        .ok_or("Missing recipient account")?;
    let reference = keys
        .iter()
        .position(|key| key.address == expected.reference)
        .ok_or("Missing reference account")?;
    demand(
        !keys[reference].signer && !keys[reference].writable,
        "Reference must be readonly and nonsigner",
    )?;
    let pre = balances(&tx["meta"]["preTokenBalances"], &keys)?;
    let post = balances(&tx["meta"]["postTokenBalances"], &keys)?;
    let instructions = array(&tx["transaction"]["message"]["instructions"])?
        .iter()
        .map(|ix| instruction(ix, keys.len()))
        .collect::<Check<Vec<_>>>()?;
    let mut transfer = None;
    let mut ata_creation = None;
    for (index, ix) in instructions.iter().enumerate() {
        match keys[ix.program].address {
            TOKEN => {
                demand(transfer.is_none(), "Multiple token instructions")?;
                let (base, destination, authority) = match ix.data.first() {
                    Some(3) if ix.data.len() == 9 => (3, 1, 2),
                    Some(12) if ix.data.len() == 10 && ix.data[9] == expected.decimals => (4, 2, 3),
                    _ => return Err("Unsupported token instruction".into()),
                };
                let a = &ix.accounts;
                demand(
                    a.len() == base + 1
                        && a[base] == reference
                        && a[destination] == recipient
                        && a[0] != recipient
                        && keys[a[0]].writable
                        && keys[recipient].writable
                        && keys[a[authority]].signer,
                    "Transfer accounts/reference mismatch",
                )?;
                if base == 4 {
                    demand(
                        keys[a[1]].address == expected.mint,
                        "Transfer mint mismatch",
                    )?;
                }
                demand(
                    u64::from_le_bytes(ix.data[1..9].try_into().unwrap()) == expected.amount_raw,
                    "Transfer amount mismatch",
                )?;
                transfer = Some(a[0]);
            }
            ATA => {
                demand(
                    ata_creation.is_none(),
                    "Multiple associated account instructions",
                )?;
                validate_ata(ix, &keys, expected)?;
                ata_creation = Some(index);
            }
            COMPUTE | MEMO => {}
            _ => return Err("Unexpected top-level program; manual review required".into()),
        }
    }
    let source = transfer.ok_or("Missing token transfer")?;
    let mut inner_groups = HashSet::new();
    let mut created_recipient = false;
    let mut initialized_recipient = false;
    for group in array(&tx["meta"]["innerInstructions"])? {
        let index = number(&group["index"])?;
        demand(
            index < instructions.len() && inner_groups.insert(index),
            "Invalid or duplicate inner instruction group",
        )?;
        for value in array(&group["instructions"])? {
            let inner = instruction(value, keys.len())?;
            demand(
                ata_creation == Some(index),
                "CPI outside associated account creation",
            )?;
            validate_ata_inner(&inner, &keys, expected, instructions[index].accounts[0])?;
            created_recipient |= keys[inner.program].address == SYSTEM;
            initialized_recipient |=
                keys[inner.program].address == TOKEN && inner.data.first() == Some(&18);
        }
    }
    let recipient_post = post
        .get(&recipient)
        .ok_or("Missing recipient post balance")?;
    expected_balance(recipient_post, expected)?;
    demand(
        recipient_post.owner == expected.merchant_wallet,
        "Recipient owner mismatch",
    )?;
    let recipient_pre = match pre.get(&recipient) {
        Some(row) => {
            demand(
                same_identity(row, recipient_post),
                "Recipient metadata changed",
            )?;
            row.amount
        }
        None => {
            demand(
                ata_creation.is_some() && created_recipient && initialized_recipient,
                "Missing recipient pre balance without complete ATA creation metadata",
            )?;
            0
        }
    };
    demand(
        recipient_post.amount.checked_sub(recipient_pre) == Some(expected.amount_raw),
        "Recipient balance delta mismatch",
    )?;
    let source_pre = pre.get(&source).ok_or("Missing source pre balance")?;
    let source_post = post.get(&source).ok_or("Missing source post balance")?;
    expected_balance(source_pre, expected)?;
    demand(
        same_identity(source_pre, source_post)
            && source_pre.owner != expected.merchant_wallet
            && source_pre.amount.checked_sub(source_post.amount) == Some(expected.amount_raw),
        "Source identity or balance delta mismatch",
    )?;
    for index in pre
        .keys()
        .chain(post.keys())
        .copied()
        .collect::<HashSet<_>>()
    {
        if index == source || index == recipient {
            continue;
        }
        demand(
            pre.get(&index) == post.get(&index),
            "Unrelated token balance changed",
        )?;
    }
    Ok(VerifiedPayment {
        signature: expected.signature.into(),
        slot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const MINT: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";
    fn pubkey(byte: u8) -> String {
        solana_pubkey::Pubkey::new_from_array([byte; 32]).to_string()
    }
    fn setup() -> (Value, Vec<String>) {
        let merchant = pubkey(6);
        let ata = spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&merchant.parse().unwrap(), &MINT.parse().unwrap(), &TOKEN.parse().unwrap()).to_string();
        let keys = vec![
            pubkey(1),
            pubkey(2),
            ata,
            MINT.into(),
            pubkey(4),
            TOKEN.into(),
            merchant,
        ];
        let balance = |index, owner: &str, amount: &str| json!({"accountIndex":index,"mint":MINT,"owner":owner,"programId":TOKEN,"uiTokenAmount":{"amount":amount,"decimals":6}});
        let mut data = vec![12];
        data.extend(100_u64.to_le_bytes());
        data.push(6);
        let tx = json!({"version":"legacy","slot":123,"transaction":{"signatures":["signature"],"message":{"header":{"numRequiredSignatures":1,"numReadonlySignedAccounts":0,"numReadonlyUnsignedAccounts":4},"accountKeys":keys,"instructions":[{"programIdIndex":5,"accounts":[1,3,2,0,4],"data":bs58::encode(data).into_string()}]}},"meta":{"err":null,"innerInstructions":[],"preTokenBalances":[balance(1,&keys[0],"1000"),balance(2,&keys[6],"0")],"postTokenBalances":[balance(1,&keys[0],"900"),balance(2,&keys[6],"100")]}});
        (tx, keys)
    }
    fn expected(keys: &[String]) -> ExpectedPayment<'_> {
        ExpectedPayment {
            signature: "signature",
            mint: MINT,
            token_program: TOKEN,
            merchant_wallet: &keys[6],
            recipient_ata: &keys[2],
            reference: &keys[4],
            amount_raw: 100,
            decimals: 6,
        }
    }
    #[test]
    fn accepts_compiled_transfer_checked_and_transfer() {
        let (mut tx, k) = setup();
        let proof = verify_transaction(&tx, &expected(&k)).unwrap();
        assert_eq!(proof.slot, 123);
        assert_eq!(proof.signature, "signature");
        let mut data = vec![3];
        data.extend(100_u64.to_le_bytes());
        tx["transaction"]["message"]["instructions"][0]["data"] =
            json!(bs58::encode(data).into_string());
        tx["transaction"]["message"]["instructions"][0]["accounts"] = json!([1, 2, 0, 4]);
        assert!(verify_transaction(&tx, &expected(&k)).is_ok());
    }
    #[test]
    fn rejects_tampered_metadata_and_instruction_fields() {
        let (tx, k) = setup();
        for (pointer, value) in [
            ("/version", json!(1)),
            ("/meta/err", json!({"InstructionError":[0,"x"]})),
            ("/meta", Value::Null),
            ("/slot", Value::Null),
            ("/transaction/signatures/0", json!("other")),
            ("/meta/postTokenBalances/1/mint", json!(pubkey(9))),
            ("/meta/postTokenBalances/1/programId", json!(pubkey(9))),
            ("/meta/postTokenBalances/1/owner", json!(pubkey(9))),
            ("/meta/postTokenBalances/1/uiTokenAmount/decimals", json!(9)),
            (
                "/meta/postTokenBalances/1/uiTokenAmount/amount",
                json!("99"),
            ),
            (
                "/meta/preTokenBalances/0/uiTokenAmount/amount",
                json!("999"),
            ),
            (
                "/transaction/message/instructions/0/programIdIndex",
                json!(3),
            ),
            (
                "/transaction/message/instructions/0/accounts",
                json!([1, 3, 1, 0, 4]),
            ),
            (
                "/transaction/message/instructions/0/accounts",
                json!([1, 3, 2, 0]),
            ),
            (
                "/transaction/message/instructions/0/data",
                json!(bs58::encode([12, 101, 0, 0, 0, 0, 0, 0, 0, 6]).into_string()),
            ),
            (
                "/transaction/message/header/numRequiredSignatures",
                json!(5),
            ),
            (
                "/transaction/message/header/numReadonlyUnsignedAccounts",
                json!(2),
            ),
            (
                "/meta/innerInstructions",
                json!([{"index":0,"instructions":[{"programIdIndex":5,"accounts":[1,3,2,0,4],"data":"1"}]}]),
            ),
        ] {
            let mut changed = tx.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(
                verify_transaction(&changed, &expected(&k)).is_err(),
                "accepted {pointer}"
            );
        }
        let mut missing = tx.clone();
        missing["meta"].as_object_mut().unwrap().remove("err");
        assert!(verify_transaction(&missing, &expected(&k)).is_err());
    }
    #[test]
    fn rejects_duplicate_transfers_balances_and_keys() {
        let (tx, k) = setup();
        for pointer in [
            "/transaction/message/instructions",
            "/meta/postTokenBalances",
            "/transaction/message/accountKeys",
        ] {
            let mut changed = tx.clone();
            let list = changed
                .pointer_mut(pointer)
                .unwrap()
                .as_array_mut()
                .unwrap();
            list.push(list[0].clone());
            assert!(verify_transaction(&changed, &expected(&k)).is_err());
        }
    }
    #[test]
    fn rejects_signer_or_writable_reference_even_when_header_is_valid() {
        let (mut tx, k) = setup();
        tx["transaction"]["signatures"] = json!(["signature", "b", "c", "d", "e"]);
        tx["transaction"]["message"]["header"] = json!({"numRequiredSignatures":5,"numReadonlySignedAccounts":1,"numReadonlyUnsignedAccounts":2});
        assert!(verify_transaction(&tx, &expected(&k)).is_err());
        let (mut tx, k) = setup();
        tx["transaction"]["message"]["header"]["numReadonlyUnsignedAccounts"] = json!(2);
        assert!(verify_transaction(&tx, &expected(&k)).is_err());
    }
    #[test]
    fn resolves_v0_loaded_reference_and_requires_complete_lookups() {
        let (mut tx, mut k) = setup();
        let reference = k.remove(4);
        tx["version"] = json!(0);
        tx["transaction"]["message"]["accountKeys"] = json!(k);
        tx["transaction"]["message"]["header"]["numReadonlyUnsignedAccounts"] = json!(3);
        tx["transaction"]["message"]["addressTableLookups"] =
            json!([{"accountKey":pubkey(20),"writableIndexes":[],"readonlyIndexes":[1]}]);
        tx["meta"]["loadedAddresses"] = json!({"writable":[],"readonly":[reference]});
        tx["transaction"]["message"]["instructions"][0]["programIdIndex"] = json!(4);
        tx["transaction"]["message"]["instructions"][0]["accounts"] = json!([1, 3, 2, 0, 6]);
        let mut original = k.clone();
        original.insert(4, reference);
        assert!(verify_transaction(&tx, &expected(&original)).is_ok());
        tx["meta"]["loadedAddresses"]["readonly"] = json!([]);
        assert!(verify_transaction(&tx, &expected(&original)).is_err());
    }
    #[test]
    fn accepts_new_canonical_ata_and_rejects_inner_transfers() {
        let (mut tx, k) = setup();
        let mut keys = k.clone();
        keys.extend([SYSTEM.into(), ATA.into()]);
        tx["transaction"]["message"]["accountKeys"] = json!(keys);
        tx["transaction"]["message"]["header"]["numReadonlyUnsignedAccounts"] = json!(6);
        let creation = json!({"programIdIndex":8,"accounts":[0,2,6,3,7,5],"data":"2"});
        tx["transaction"]["message"]["instructions"]
            .as_array_mut()
            .unwrap()
            .insert(0, creation);
        tx["meta"]["preTokenBalances"].as_array_mut().unwrap().pop();
        let ix = |program, accounts: Vec<usize>, data: Vec<u8>| json!({"programIdIndex":program,"accounts":accounts,"data":bs58::encode(data).into_string()});
        let mut create = vec![0, 0, 0, 0];
        create.extend(2_039_280_u64.to_le_bytes());
        create.extend(165_u64.to_le_bytes());
        create.extend(bs58::decode(TOKEN).into_vec().unwrap());
        let mut initialize = vec![18];
        initialize.extend(bs58::decode(&k[6]).into_vec().unwrap());
        tx["meta"]["innerInstructions"] = json!([{ "index":0,"instructions":[
            ix(5,vec![3],vec![21,7,0]),ix(7,vec![0,2],create),
            ix(5,vec![2],vec![22]),ix(5,vec![2,3],initialize)
        ] }]);
        assert!(verify_transaction(&tx, &expected(&k)).is_ok());
        let mut transfer_data = vec![3];
        transfer_data.extend(100_u64.to_le_bytes());
        tx["meta"]["innerInstructions"][0]["instructions"]
            .as_array_mut()
            .unwrap()
            .push(ix(5, vec![1, 2, 0], transfer_data));
        assert!(verify_transaction(&tx, &expected(&k)).is_err());
    }
    #[test]
    fn rejects_wrong_source_mint_unrelated_outflow_and_unbound_reference() {
        let (tx, k) = setup();
        let mut changed = tx.clone();
        changed["meta"]["preTokenBalances"][0]["mint"] = json!(pubkey(8));
        assert!(verify_transaction(&changed, &expected(&k)).is_err());
        let mut changed = tx.clone();
        let mut extra = changed["meta"]["postTokenBalances"][1].clone();
        extra["accountIndex"] = json!(6);
        changed["meta"]["postTokenBalances"]
            .as_array_mut()
            .unwrap()
            .push(extra);
        assert!(verify_transaction(&changed, &expected(&k)).is_err());
        let mut changed = tx;
        changed["transaction"]["message"]["instructions"][0]["accounts"] = json!([1, 3, 2, 0, 6]);
        assert!(verify_transaction(&changed, &expected(&k)).is_err());
    }
    #[test]
    fn rejects_missing_pre_balance_without_ata_creation() {
        let (mut tx, k) = setup();
        tx["meta"]["preTokenBalances"].as_array_mut().unwrap().pop();
        assert!(verify_transaction(&tx, &expected(&k)).is_err());
    }
}
