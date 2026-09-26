mod errors;
mod ledger;
mod models;

use std::io::{self, Write};

use ledger::Ledger;

fn prompt(message: &str) -> io::Result<Option<String>> {
    print!("{message}");
    io::stdout().flush()?;

    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Ok(None);
    }

    Ok(Some(input.trim().to_owned()))
}

fn prompt_u64(message: &str) -> io::Result<Option<u64>> {
    let Some(input) = prompt(message)? else {
        return Ok(None);
    };

    match input.parse() {
        Ok(value) => Ok(Some(value)),
        Err(_) => {
            println!("Please enter a non-negative whole number.");
            Ok(None)
        }
    }
}

fn create_account(ledger: &mut Ledger) -> io::Result<()> {
    let Some(name) = prompt("Account name: ")? else {
        return Ok(());
    };
    let Some(balance) = prompt_u64("Initial balance: ")? else {
        return Ok(());
    };

    match ledger.create_account(&name, balance) {
        Ok(id) => println!("Created account #{id}."),
        Err(error) => println!("Error: {error}"),
    }

    Ok(())
}

fn transfer(ledger: &mut Ledger) -> io::Result<()> {
    let Some(from) = prompt_u64("Sender account ID: ")? else {
        return Ok(());
    };
    let Some(to) = prompt_u64("Receiver account ID: ")? else {
        return Ok(());
    };
    let Some(amount) = prompt_u64("Amount: ")? else {
        return Ok(());
    };

    match ledger.transfer(from, to, amount) {
        Ok(()) => println!("Transferred {amount} from account #{from} to account #{to}."),
        Err(error) => println!("Error: {error}"),
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let mut ledger = Ledger::new();

    loop {
        println!("\n1. Create account");
        println!("2. Show accounts");
        println!("3. Transfer");
        println!("4. Show transactions");
        println!("5. Exit");

        let Some(choice) = prompt("Choose an option: ")? else {
            break;
        };

        match choice.as_str() {
            "1" => create_account(&mut ledger)?,
            "2" => ledger.print_accounts(),
            "3" => transfer(&mut ledger)?,
            "4" => ledger.print_transactions(),
            "5" => break,
            _ => println!("Please choose a number from 1 to 5."),
        }
    }

    println!("Goodbye!");
    Ok(())
}
