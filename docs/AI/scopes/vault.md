# Liquidity Vault

- Программа `liquidity-vault/programs/liquidity-vault/` и Rust-клиент `liquidity-vault/clients/vault-client/` в отдельном workspace `liquidity-vault/` реализуют недели 8–13 методички; команды выполняются из этой папки: vault, share-токены, вывод, Raydium CLMM.
- Vault принимает SNTOK (Token-2022). Все инструкции работают через `token_interface`; не заменяйте на классический `token`.
- PDA: `["vault", token_mint]` — VaultState и authority хранилищ и share mint; `["vault_tokens", vault]`, `["share_mint", vault]`, `["user_card", vault, user]`. Не меняйте seeds и порядок полей существующих аккаунтов: vault уже создан в devnet.
- SBF собирайте `cargo-build-sbf` из Solana 3.1.10 (`~/.local/share/solana/install/releases/3.1.10/solana-release/bin`): сборка 4.x не исполняется в LiteSVM 0.10. Деплой в локальный валидатор 3.1.10 выполняйте тем же `solana`.
- Клиент пишет только в devnet или localnet (проверка genesis), всегда выполняет simulation и читает итоговое состояние через RPC.
