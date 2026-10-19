use serde::{Deserialize, Serialize};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use spl_associated_token_account_interface::address::get_associated_token_address_with_program_id;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const MINT: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";
pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const GENESIS: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
pub const DECIMALS: u8 = 6;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Attempt {
    pub signature: String,
    pub outcome: String,
    pub reason: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Receipt {
    pub signature: String,
    pub slot: u64,
    pub amount_raw: String,
    pub mint: String,
    pub recipient_ata: String,
    pub commitment: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Invoice {
    pub public_id: String,
    pub title: String,
    pub amount: String,
    pub amount_raw: String,
    pub network: String,
    pub mint: String,
    pub merchant_wallet: String,
    pub recipient_ata: String,
    pub reference: String,
    pub state: String,
    pub solana_pay_url: String,
    pub attempts: Vec<Attempt>,
    pub receipt: Option<Receipt>,
}
#[derive(Clone, Serialize, Deserialize)]
struct RequestRecord {
    title: String,
    amount_raw: String,
    public_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Disk {
    schema: u8,
    merchant_wallet: String,
    invoices: Vec<Invoice>,
    requests: BTreeMap<String, RequestRecord>,
}
pub struct Store {
    path: PathBuf,
    disk: Disk,
    _lock: File,
}

pub fn random_id() -> String {
    Keypair::new().pubkey().to_string()
}
pub fn parse_amount(value: &str) -> Result<u64, String> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || (whole.len() > 1 && whole.starts_with('0'))
        || fraction.len() > 6
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
    {
        return Err(
            "Сумма должна быть положительной десятичной строкой с точностью до 6 знаков".into(),
        );
    }
    let whole = whole.parse::<u64>().map_err(|_| "Слишком большая сумма")?;
    let frac = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().map_err(|_| "Некорректная сумма")?
            * 10u64.pow(6 - fraction.len() as u32)
    };
    whole
        .checked_mul(1_000_000)
        .and_then(|v| v.checked_add(frac))
        .filter(|v| *v > 0 && *v <= i64::MAX as u64)
        .ok_or_else(|| "Сумма вне допустимого диапазона".into())
}
pub fn format_amount(raw: u64) -> String {
    let fraction = format!("{:06}", raw % 1_000_000);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        (raw / 1_000_000).to_string()
    } else {
        format!("{}.{}", raw / 1_000_000, fraction)
    }
}
fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!("pending-{}", random_id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
impl Store {
    pub fn open(path: PathBuf, wallet: &str) -> Result<Self, String> {
        wallet
            .parse::<Pubkey>()
            .map_err(|_| "Некорректный кошелёк продавца")?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "Хранилище уже открыто другим процессом")?;
        let disk: Disk = if path.exists() {
            serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Повреждённое хранилище: {e}"))?
        } else {
            Disk {
                schema: 1,
                merchant_wallet: wallet.into(),
                invoices: vec![],
                requests: BTreeMap::new(),
            }
        };
        if disk.schema != 1 || disk.merchant_wallet != wallet {
            return Err("Хранилище имеет другую версию или кошелёк продавца".into());
        }
        for invoice in &disk.invoices {
            if invoice.mint != MINT
                || invoice.network != "devnet"
                || invoice.merchant_wallet != wallet
                || parse_amount(&invoice.amount)?.to_string() != invoice.amount_raw
            {
                return Err("Нарушены реквизиты сохранённого счёта".into());
            }
        }
        Ok(Self {
            path,
            disk,
            _lock: lock,
        })
    }
    fn save(&mut self, disk: Disk) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&disk).map_err(|e| e.to_string())?;
        atomic_write(&self.path, &bytes).map_err(|e| format!("Не удалось сохранить счёт: {e}"))?;
        self.disk = disk;
        Ok(())
    }
    pub fn list(&self) -> Vec<Invoice> {
        self.disk.invoices.iter().rev().cloned().collect()
    }
    pub fn get(&self, id: &str) -> Option<Invoice> {
        self.disk
            .invoices
            .iter()
            .find(|v| v.public_id == id)
            .cloned()
    }
    pub fn create(
        &mut self,
        title: &str,
        amount: &str,
        key: &str,
    ) -> Result<(Invoice, bool), String> {
        if title.trim().is_empty() || title.len() > 100 || title.chars().any(char::is_control) {
            return Err(
                "Название должно содержать от 1 до 100 UTF-8 байт без управляющих символов".into(),
            );
        }
        if key.is_empty()
            || key.len() > 100
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err("Требуется корректный Idempotency-Key".into());
        }
        let raw = parse_amount(amount)?;
        if let Some(record) = self.disk.requests.get(key) {
            if record.title != title.trim() || record.amount_raw != raw.to_string() {
                return Err("Idempotency-Key уже использован с другими данными".into());
            }
            return self
                .get(&record.public_id)
                .map(|v| (v, false))
                .ok_or_else(|| "Счёт запроса не найден".into());
        }
        if self.disk.invoices.len() >= 1000 {
            return Err("Достигнут лимит 1000 счетов локального хранилища".into());
        }
        let wallet: Pubkey = self
            .disk
            .merchant_wallet
            .parse()
            .map_err(|_| "Некорректный кошелёк")?;
        let ata = get_associated_token_address_with_program_id(
            &wallet,
            &MINT.parse().unwrap(),
            &TOKEN.parse().unwrap(),
        )
        .to_string();
        let amount = format_amount(raw);
        let reference = random_id();
        let invoice = Invoice { public_id: random_id(), title: title.trim().into(), amount: amount.clone(), amount_raw: raw.to_string(), network: "devnet".into(), mint: MINT.into(), merchant_wallet: wallet.to_string(), recipient_ata: ata,
            reference: reference.clone(), state: "open".into(), solana_pay_url: format!("solana:{wallet}?amount={amount}&spl-token={MINT}&reference={reference}&label=SNOWNUMB%20Pay"), attempts: vec![], receipt: None };
        let mut disk = self.disk.clone();
        disk.requests.insert(
            key.into(),
            RequestRecord {
                title: invoice.title.clone(),
                amount_raw: invoice.amount_raw.clone(),
                public_id: invoice.public_id.clone(),
            },
        );
        disk.invoices.push(invoice.clone());
        self.save(disk)?;
        Ok((invoice, true))
    }
    pub fn close(&mut self, id: &str) -> Result<Invoice, String> {
        let mut disk = self.disk.clone();
        let invoice = disk
            .invoices
            .iter_mut()
            .find(|v| v.public_id == id)
            .ok_or("Счёт не найден")?;
        if invoice.state != "open" {
            return Err("Закрыть можно только ожидающий оплаты счёт".into());
        }
        invoice.state = "closed".into();
        let result = invoice.clone();
        self.save(disk)?;
        Ok(result)
    }
    pub fn record(
        &mut self,
        id: &str,
        signature: &str,
        verification: Result<u64, String>,
    ) -> Result<(), String> {
        let mut disk = self.disk.clone();
        if disk.invoices.iter().any(|v| {
            v.public_id != id
                && (v.receipt.as_ref().is_some_and(|r| r.signature == signature)
                    || v.attempts
                        .iter()
                        .any(|a| a.signature == signature && a.outcome == "verified"))
        }) {
            return Err("Перевод уже отнесён к другому счёту".into());
        }
        let invoice = disk
            .invoices
            .iter_mut()
            .find(|v| v.public_id == id)
            .ok_or("Счёт не найден")?;
        if invoice.attempts.iter().any(|a| a.signature == signature) {
            return Ok(());
        }
        if invoice.attempts.len() >= 1000 {
            return Err("Журнал достиг лимита; нужна отдельная сверка".into());
        }
        let (outcome, reason) = match verification {
            Ok(slot) if invoice.state == "open" => {
                invoice.state = "paid".into();
                invoice.receipt = Some(Receipt {
                    signature: signature.into(),
                    slot,
                    amount_raw: invoice.amount_raw.clone(),
                    mint: MINT.into(),
                    recipient_ata: invoice.recipient_ata.clone(),
                    commitment: "finalized".into(),
                });
                (
                    "verified",
                    "Сумма, mint, ATA, reference и finalized проверены через RPC".to_string(),
                )
            }
            Ok(_) => (
                "review_required",
                "Поздний или дополнительный платёж; требуется решение продавца".into(),
            ),
            Err(reason) => ("review_required", reason),
        };
        invoice.attempts.push(Attempt {
            signature: signature.into(),
            outcome: outcome.into(),
            reason,
        });
        self.save(disk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const WALLET: &str = "9KmHj9ZwNcPysvJM3CYAuc7Jp1hzVbEC7QaBAT7uD7N2";
    fn path() -> PathBuf {
        std::env::temp_dir().join(format!("payments-{}.json", random_id()))
    }
    #[test]
    fn amounts_are_exact_and_reject_ambiguous_input() {
        assert_eq!(parse_amount("125.50").unwrap(), 125_500_000);
        assert_eq!(
            parse_amount("9007199254.740993").unwrap(),
            9_007_199_254_740_993
        );
        for bad in [
            "0",
            "-1",
            "1e2",
            " 1",
            "1.",
            ".1",
            "01",
            "1.0000001",
            "9223372036854.775808",
        ] {
            assert!(parse_amount(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn persistence_idempotence_and_single_process_lock() {
        let p = path();
        let mut store = Store::open(p.clone(), WALLET).unwrap();
        let invoice = store.create("Счёт", "125.50", "request1").unwrap().0;
        assert!(!store.create("Счёт", "125.5", "request1").unwrap().1);
        assert!(store.create("Другой", "125.5", "request1").is_err());
        assert!(Store::open(p.clone(), WALLET).is_err());
        drop(store);
        let store = Store::open(p.clone(), WALLET).unwrap();
        assert_eq!(
            store.get(&invoice.public_id).unwrap().amount_raw,
            "125500000"
        );
        drop(store);
        let _ = fs::remove_file(&p);
        let _ = fs::remove_file(p.with_extension("lock"));
    }
    #[test]
    fn stale_temporary_does_not_block_writes_after_restart() {
        let p = path();
        let mut store = Store::open(p.clone(), WALLET).unwrap();
        store.create("First", "1", "first").unwrap();
        drop(store);
        fs::write(p.with_extension("pending"), b"interrupted write").unwrap();
        let mut store = Store::open(p.clone(), WALLET).unwrap();
        store.create("Second", "2", "second").unwrap();
        assert_eq!(store.list().len(), 2);
        drop(store);
        for path in [&p, &p.with_extension("lock"), &p.with_extension("pending")] {
            let _ = fs::remove_file(path);
        }
    }
    #[test]
    fn receipt_is_not_duplicated_and_late_payment_does_not_reopen() {
        let p = path();
        let mut store = Store::open(p.clone(), WALLET).unwrap();
        let a = store.create("A", "1", "a").unwrap().0;
        store.record(&a.public_id, "sig-a", Ok(10)).unwrap();
        store.record(&a.public_id, "sig-a", Ok(10)).unwrap();
        store.record(&a.public_id, "sig-b", Ok(11)).unwrap();
        let paid = store.get(&a.public_id).unwrap();
        assert_eq!(paid.attempts.len(), 2);
        assert_eq!(paid.receipt.unwrap().signature, "sig-a");
        let b = store.create("B", "1", "b").unwrap().0;
        store.close(&b.public_id).unwrap();
        store.record(&b.public_id, "sig-c", Ok(12)).unwrap();
        let closed = store.get(&b.public_id).unwrap();
        assert_eq!(closed.state, "closed");
        assert!(closed.receipt.is_none());
        assert_eq!(closed.attempts[0].outcome, "review_required");
        drop(store);
        let _ = fs::remove_file(&p);
        let _ = fs::remove_file(p.with_extension("lock"));
    }
}
