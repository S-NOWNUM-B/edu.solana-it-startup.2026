<div align="center">

# Solana Onchain Profile

Учебный профиль на PDA: Anchor-программа и Rust-клиент для создания, обновления и чтения данных в Solana.

![Rust](https://img.shields.io/badge/Rust-1.89.0-000000?style=for-the-badge&logo=rust&logoColor=white)
![Anchor](https://img.shields.io/badge/Anchor-1.1.2-6750A4?style=for-the-badge)
![Solana](https://img.shields.io/badge/Solana-3.1.10-14F195?style=for-the-badge&logo=solana&logoColor=black)
![LiteSVM](https://img.shields.io/badge/LiteSVM-0.10.0-334155?style=for-the-badge)

</div>

## Содержание

- [О проекте](#о-проекте)
- [Технологии](#технологии)
- [Архитектура](#архитектура)
- [Профиль](#профиль)
- [Запуск](#запуск)
- [Команды клиента](#команды-клиента)
- [Проверки](#проверки)

## О проекте

Задание №6 реализует onchain-профиль: кошелёк создаёт аккаунт с именем и описанием, обновляет его и читает через RPC-клиент. Данные хранятся в PDA, адрес которого вычисляется по публичному ключу владельца.

В репозитории сохранены Counter с отдельным Rust-клиентом и токен-программа для создания mint и associated token account, выпуска и перевода токенов через `transfer_checked`.

Задание находится в ветке `task/06-onchain-profile`. Нумерация `task/*` независима от заданий онлайн-платформы в ветках `platform/01-tests`, `platform/02-burn` и `platform/03-escrow`.

## Технологии

| Часть проекта | Технологии |
| :-- | :-- |
| Программы | Rust 1.89.0, Anchor 1.1.2, Solana CLI 3.1.10 |
| Profile-клиент | Rust, Cargo, `solana-rpc-client` 3.1.14 |
| Тесты программ | Rust, LiteSVM 0.10.0 |

## Архитектура

| Каталог | Ответственность |
| :-- | :-- |
| `programs/onchain-profile/` | Anchor-инструкции и аккаунт Profile |
| `clients/profile-client/` | Подписание транзакций, отправка в RPC и чтение профиля |
| `programs/counter/`, `clients/counter-client/` | Счётчик из задания №5 |
| `programs/solana-level-1-token-starter/` | Существующая токен-программа |

Программы и клиенты входят в корневой Cargo workspace. Profile-клиент использует типы аккаунта и инструкций crate `onchain-profile` с feature `no-entrypoint`, подтверждает транзакции и читает аккаунты с commitment `confirmed`.

## Профиль

Каждый authority имеет один канонический PDA с seeds `[b"profile", authority.pubkey()]`. Создание и обновление требуют подписи владельца; чтение публично.

| Поле Profile | Тип | Ограничение |
| :-- | :-- | :-- |
| `authority` | `Pubkey` | Кошелёк владельца |
| `display_name` | `String` | Не пустое после `trim()`, до 32 байт UTF-8 |
| `bio` | `String` | До 160 байт UTF-8; может быть пустым |
| `bump` | `u8` | Канонический bump PDA |

| Инструкция | Результат | Проверки |
| :-- | :-- | :-- |
| `initialize(display_name, bio)` | Создаёт профиль | Подпись authority, PDA и допустимые строки; повторное создание отклоняется |
| `update_profile(display_name, bio)` | Обновляет имя и описание | Подпись, `has_one`, seeds, bump и допустимые строки |

Лимиты считаются в байтах, поэтому символы кириллицы и emoji занимают больше одного байта. Чужой authority не может изменить профиль; отклонённая транзакция сохраняет прежние данные.

## Запуск

На macOS используйте терминал zsh/bash. Все команды выполняйте из корня репозитория; нужны версии из таблицы технологий и кошелёк `~/.config/solana/id.json`.

Если кошелька нет, создайте его в zsh/bash. Условие сохраняет существующий файл:

```bash
if [ ! -e "$HOME/.config/solana/id.json" ]; then
  solana-keygen new --no-bip39-passphrase --silent --outfile ~/.config/solana/id.json
fi
```

Для своего локального кошелька передайте клиенту `--keypair <PATH>` после команды и пополните соответствующий публичный адрес.

1. Соберите программы в zsh/bash:

   ```bash
   anchor build --ignore-keys
   ```

   Сборка создаёт `target/deploy/onchain_profile.so` и файлы существующих программ. Флаг `--ignore-keys` позволяет использовать объявленные program ID без хранения program keypair в репозитории.

2. В отдельном терминале zsh/bash запустите validator и загрузите программу профиля:

   ```bash
   mkdir -p .anchor
   solana-test-validator \
     --bpf-program 3j2EZTtxkTLmxCjnpBzJhhf3QsavZwueQ9QbQzdxqyzj target/deploy/onchain_profile.so \
     --ledger .anchor/profile-ledger
   ```

   Оставьте validator работающим. Ledger сохраняет состояние между запусками; демонстрация использует localnet.

3. В другом терминале zsh/bash пополните локальный кошелёк и запустите демонстрацию:

   ```bash
   solana airdrop 2 --url localhost
   cargo run -p profile-client --locked -- demo
   ```

   `demo` создаёт отсутствующий профиль с именем `SNOWNUMB` и описанием `Solana profile`, затем обновляет описание на `Onchain profile with PDA` и читает результат. Для существующего профиля команда сохраняет имя и заменяет только `bio`.

## Команды клиента

Клиент по умолчанию подключается к `http://127.0.0.1:8899` и использует `~/.config/solana/id.json`. Параметры `--url` и `--keypair` указывайте после команды.

| Команда в zsh/bash | Назначение |
| :-- | :-- |
| `cargo run -p profile-client --locked -- create --name SNOWNUMB --bio "Solana profile"` | Создать профиль кошелька |
| `cargo run -p profile-client --locked -- update --name SNOWNUMB --bio "Onchain profile with PDA"` | Изменить имя и описание |
| `cargo run -p profile-client --locked -- show` | Прочитать профиль своего кошелька |
| `cargo run -p profile-client --locked -- show --authority <PUBKEY>` | Прочитать публичный профиль без файла кошелька |
| `cargo run -p profile-client --locked -- demo` | Создать отсутствующий профиль, обновить и прочитать |

Профиль хранится публично. В `display_name` и `bio` нельзя записывать приватные ключи, seed phrase или другие секреты.

## Проверки

После сборки выполните из корня репозитория в zsh/bash:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Перед тестами нужна Anchor-сборка: LiteSVM загружает файлы программ из `target/deploy/`.

Тесты в `clients/profile-client/tests/profile.rs` проверяют PDA и размер аккаунта, создание, обновление, публичное чтение, изоляцию кошельков и границы UTF-8. Негативные сценарии проверяют полномочия, подпись, повторную инициализацию, подмену PDA и некорректные данные; отклонённые операции сохраняют прежнее состояние.

Команда `demo` проверяет взаимодействие Profile-клиента с запущенным validator.

Файлы кошельков, program keypair, seed phrase и приватные ключи нельзя добавлять в Git или публиковать в документации.
