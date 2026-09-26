<div align="center">

# Solana Level 1 Token Starter

Учебный репозиторий с токен-программой Anchor и отдельной Rust CLI-утилитой.

![Rust](https://img.shields.io/badge/Rust-1.89.0-000000?style=for-the-badge&logo=rust&logoColor=white)
![Anchor](https://img.shields.io/badge/Anchor-1.1.2-6750A4?style=for-the-badge)
![Solana](https://img.shields.io/badge/Solana-3.1.10-14F195?style=for-the-badge&logo=solana&logoColor=black)
![LiteSVM](https://img.shields.io/badge/LiteSVM-0.10.0-334155?style=for-the-badge)

</div>

## О проекте

Репозиторий содержит учебную Solana-программу для создания mint и токен-аккаунтов, выпуска и перевода токенов. Отдельная утилита `wallet-ledger` имитирует аккаунты и переводы локально, без подключения к блокчейну.

## Технологии

| Часть проекта | Технологии |
| :-- | :-- |
| Токен-программа | Rust, Anchor 1.1.2, Solana CLI 3.1.10 |
| Тест программы | Rust, LiteSVM 0.10.0 |
| CLI-утилита | Rust 1.89.0, Cargo, стандартная библиотека |

## Архитектура

- `programs/solana-level-1-token-starter/` — Anchor-программа с инструкциями создания mint, создания токен-аккаунта, выпуска и перевода токенов.
- `rust-cli/` — самостоятельный Cargo-проект с меню, аккаунтами и историей переводов в памяти. Его зависимости не входят в корневой workspace.

## Запуск

На macOS используйте терминал zsh/bash. Для токен-программы выполните из корня репозитория:

```bash
anchor build --ignore-keys
cargo test --workspace --locked
```

Для CLI-утилиты выполните из корня репозитория:

```bash
cd rust-cli
cargo run
```
