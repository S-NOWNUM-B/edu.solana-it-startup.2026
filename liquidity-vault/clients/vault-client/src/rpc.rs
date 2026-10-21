use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::{read_keypair_file, Keypair};
use solana_message::Instruction;
use solana_rpc_client::rpc_client::RpcClient;
use solana_rpc_client_api::config::{
    RpcSendTransactionConfig, RpcSimulateTransactionAccountsConfig, RpcSimulateTransactionConfig,
};
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{error::Error, io, path::Path, time::Duration};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub const DEVNET_URL: &str = "https://api.devnet.solana.com";
const DEVNET_GENESIS: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const MAINNET_GENESIS: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";
const TESTNET_GENESIS: &str = "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY";

pub fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(io::Error::other(message.into()).into())
}

/// Запись разрешена только в devnet или локальном валидаторе.
pub fn network_name(genesis: &str) -> Result<&'static str> {
    match genesis {
        DEVNET_GENESIS => Ok("devnet"),
        MAINNET_GENESIS | TESTNET_GENESIS => fail("Only devnet or localnet are allowed"),
        _ => Ok("localnet"),
    }
}

pub struct Client {
    pub rpc: RpcClient,
    pub payer: Keypair,
    pub network: &'static str,
    url: String,
}

impl Client {
    pub fn connect(url: &str, keypair: &Path) -> Result<Self> {
        let rpc = RpcClient::new_with_timeout_and_commitment(
            url,
            Duration::from_secs(60),
            CommitmentConfig::confirmed(),
        );
        let network = network_name(&rpc.get_genesis_hash()?.to_string())?;
        let payer = read_keypair_file(keypair)
            .map_err(|_| io::Error::other(format!("Cannot read keypair: {}", keypair.display())))?;
        let program = rpc.get_account(&liquidity_vault::ID)?;
        if !program.executable {
            return fail("Liquidity Vault program is not deployed");
        }
        println!(
            "RPC: {url}\nNetwork: {network}\nProgram: {}\nWallet: {}",
            liquidity_vault::ID,
            payer.pubkey()
        );
        Ok(Self {
            rpc,
            payer,
            network,
            url: url.into(),
        })
    }

    pub fn explorer(&self, kind: &str, value: impl std::fmt::Display) -> String {
        let cluster = if self.network == "devnet" {
            "devnet".to_string()
        } else {
            format!("custom&customUrl={}", self.url)
        };
        format!("https://explorer.solana.com/{kind}/{value}?cluster={cluster}")
    }

    fn sign(
        &self,
        instructions: &[Instruction],
        extra_signers: &[&Keypair],
    ) -> Result<Transaction> {
        let mut signers = vec![&self.payer];
        signers.extend_from_slice(extra_signers);
        Ok(Transaction::new_signed_with_payer(
            instructions,
            Some(&self.payer.pubkey()),
            &signers,
            self.rpc.get_latest_blockhash()?,
        ))
    }

    /// Simulation с чтением аккаунтов после выполнения: так клиент получает ожидаемую сумму.
    pub fn simulate_accounts(
        &self,
        instructions: &[Instruction],
        addresses: &[Pubkey],
    ) -> Result<Vec<Vec<u8>>> {
        let tx = self.sign(instructions, &[])?;
        let result = self
            .rpc
            .simulate_transaction_with_config(
                &tx,
                RpcSimulateTransactionConfig {
                    commitment: Some(CommitmentConfig::confirmed()),
                    accounts: Some(RpcSimulateTransactionAccountsConfig {
                        encoding: None,
                        addresses: addresses.iter().map(ToString::to_string).collect(),
                    }),
                    ..Default::default()
                },
            )?
            .value;
        if let Some(error) = result.err {
            for line in result.logs.unwrap_or_default() {
                println!("  {line}");
            }
            return fail(format!("Quote simulation failed: {error:?}"));
        }
        result
            .accounts
            .unwrap_or_default()
            .into_iter()
            .map(|account| {
                account
                    .and_then(|account| account.data.decode())
                    .ok_or_else(|| "Simulation did not return account data".into())
            })
            .collect()
    }

    /// Отправка без preflight: отклонённая транзакция попадает в сеть и видна в Explorer.
    pub fn send_expecting_failure(
        &self,
        label: &str,
        instructions: &[Instruction],
    ) -> Result<String> {
        let tx = self.sign(instructions, &[])?;
        let signature = self.rpc.send_transaction_with_config(
            &tx,
            RpcSendTransactionConfig {
                skip_preflight: true,
                ..Default::default()
            },
        )?;
        self.rpc
            .poll_for_signature_with_commitment(&signature, CommitmentConfig::confirmed())?;
        let status = self.rpc.get_signature_status(&signature)?;
        println!("{label}: {}", self.explorer("tx", signature));
        match status {
            Some(Err(error)) => println!("  rejected on-chain: {error}"),
            _ => return fail(format!("{label}: transaction unexpectedly succeeded")),
        }
        Ok(signature.to_string())
    }

    /// Simulation, затем отправка и подтверждение; печатает логи и ссылку на Explorer.
    pub fn send(
        &self,
        label: &str,
        instructions: &[Instruction],
        extra_signers: &[&Keypair],
    ) -> Result<String> {
        let tx = self.sign(instructions, extra_signers)?;
        let simulation = self.rpc.simulate_transaction(&tx)?.value;
        for line in simulation.logs.unwrap_or_default() {
            println!("  {line}");
        }
        if let Some(error) = simulation.err {
            return fail(format!("{label}: simulation failed: {error:?}"));
        }
        let signature = self.rpc.send_and_confirm_transaction(&tx)?;
        println!("{label}: {}", self.explorer("tx", signature));
        Ok(signature.to_string())
    }

    pub fn account<T: AccountDeserialize>(&self, address: &Pubkey) -> Result<Option<T>> {
        let account = self
            .rpc
            .get_account_with_commitment(address, CommitmentConfig::confirmed())?
            .value;
        match account {
            Some(account) => Ok(Some(T::try_deserialize(&mut account.data.as_slice())?)),
            None => Ok(None),
        }
    }

    pub fn exists(&self, address: &Pubkey) -> Result<bool> {
        Ok(self
            .rpc
            .get_account_with_commitment(address, CommitmentConfig::confirmed())?
            .value
            .is_some())
    }

    pub fn token_balance(&self, account: &Pubkey) -> Result<u64> {
        if !self.exists(account)? {
            return Ok(0);
        }
        Ok(self
            .rpc
            .get_token_account_balance(account)?
            .amount
            .parse()?)
    }
}

pub fn vault_instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: liquidity_vault::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}
