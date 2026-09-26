<div align="center">

# wallet-ledger

Локальная CLI-утилита для создания аккаунтов и перевода условных единиц.

![Rust](https://img.shields.io/badge/Rust-1.89.0-000000?style=for-the-badge&logo=rust&logoColor=white)
![Cargo](https://img.shields.io/badge/Cargo-standalone-334155?style=for-the-badge)

</div>

## О проекте

Программа хранит аккаунты и историю операций в памяти. Данные исчезают после выхода; Solana SDK, Anchor и сторонние библиотеки не используются.

## Технологии

Rust 1.89.0, Cargo и стандартная библиотека Rust.

## Архитектура

- `src/main.rs` — меню и ввод через stdin/stdout.
- `src/models.rs` — `Account`, `Transaction` и `TransactionType`.
- `src/errors.rs` — ошибки `AppError`.
- `src/ledger.rs` — хранение данных в `Vec`, создание аккаунтов и проверка переводов.

## Запуск

На macOS выполните в терминале zsh/bash из корня репозитория:

```bash
cd rust-cli
cargo run
```
