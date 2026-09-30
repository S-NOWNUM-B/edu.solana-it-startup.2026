<div align="center">

# Solana Anchor Counter

Учебная Counter-программа Anchor и Rust-клиент для взаимодействия с локальным Solana validator.

![Rust](https://img.shields.io/badge/Rust-1.89.0-000000?style=for-the-badge&logo=rust&logoColor=white)
![Anchor](https://img.shields.io/badge/Anchor-1.1.2-6750A4?style=for-the-badge)
![Solana](https://img.shields.io/badge/Solana-3.1.10-14F195?style=for-the-badge&logo=solana&logoColor=black)
![LiteSVM](https://img.shields.io/badge/LiteSVM-0.10.0-334155?style=for-the-badge)

</div>

## Содержание

- [О проекте](#о-проекте)
- [Технологии](#технологии)
- [Архитектура](#архитектура)
- [Counter](#counter)
- [Запуск](#запуск)
- [Команды клиента](#команды-клиента)
- [Проверки](#проверки)

## О проекте

Задание №5 реализует Counter Program: кошелёк создаёт свой счётчик, увеличивает его и читает состояние через RPC-клиент. Демонстрация выполняет реальные транзакции в localnet.

В репозитории также сохранена токен-программа: создание mint и associated token account, выпуск токенов и перевод через `transfer_checked`. Она использует `token_interface` для совместимости с Token Program и Token-2022.

Учебное задание находится в ветке `task/05-anchor-counter`. Нумерация `task/*` независима от заданий онлайн-платформы в ветках `platform/01-tests`, `platform/02-burn` и `platform/03-escrow`.

## Технологии

| Часть проекта | Технологии |
| :-- | :-- |
| Программы | Rust 1.89.0, Anchor 1.1.2, Solana CLI 3.1.10 |
| Counter-клиент | Rust, Cargo, `solana-rpc-client` 3.1.14 |
| Тесты программ | Rust, LiteSVM 0.10.0 |

## Архитектура

| Каталог | Ответственность |
| :-- | :-- |
| `programs/counter/` | Anchor-инструкции и состояние счётчика |
| `clients/counter-client/` | Подписание транзакций, отправка в RPC и чтение Counter |
| `programs/solana-level-1-token-starter/` | Существующая токен-программа |

Программы и клиент входят в корневой Cargo workspace. Клиент использует типы аккаунта и инструкций из crate `counter` с feature `no-entrypoint`, подтверждает транзакции и читает аккаунты с commitment `confirmed`.

## Counter

Каждый authority имеет один PDA с seeds `[b"counter", authority.pubkey()]`. Аккаунт хранит `authority: Pubkey`, `count: u64` и `bump: u8`.

| Инструкция | Результат | Проверки |
| :-- | :-- | :-- |
| `initialize` | Создаёт Counter с `count = 0` | Подпись authority, правильный PDA; повторное создание отклоняется |
| `increment` | Увеличивает `count` на 1 | Подпись, `has_one`, seeds; переполнение отклоняется |

Счётчики разных кошельков независимы. Изменить чужой Counter нельзя; неуспешная транзакция сохраняет прежнее значение.

## Запуск

На macOS используйте терминал zsh/bash. Все команды выполняйте из корня репозитория; нужны версии из таблицы технологий и кошелёк `~/.config/solana/id.json`.

Если кошелька нет, создайте его в zsh/bash. Условие сохраняет существующий файл:

```bash
if [ ! -e "$HOME/.config/solana/id.json" ]; then
  solana-keygen new --no-bip39-passphrase --silent --outfile ~/.config/solana/id.json
fi
```

Можно использовать свой локальный кошелёк, передав его путь клиенту через `--keypair` после команды.

1. Соберите программы:

   ```bash
   anchor build --ignore-keys
   ```

   Сборка создаёт `target/deploy/counter.so` и файл токен-программы. Флаг `--ignore-keys` позволяет использовать объявленные program ID без хранения program keypair в репозитории.

2. В отдельном терминале zsh/bash запустите validator и загрузите Counter под объявленным ID:

   ```bash
   mkdir -p .anchor
   solana-test-validator \
     --bpf-program HhHRzXNfzoM5ZWCNEmVtgz2upLMrbwPCdk63KjhoFybR target/deploy/counter.so \
     --ledger .anchor/counter-ledger
   ```

   Оставьте validator работающим. Ledger сохраняет состояние между запусками; этот сценарий использует localnet.

3. В другом терминале zsh/bash пополните локальный кошелёк и запустите демонстрацию:

   ```bash
   solana airdrop 2 --url localhost
   cargo run -p counter-client --locked -- demo
   ```

   `demo` создаёт отсутствующий Counter и выполняет три увеличения с чтением после каждого. Для нового аккаунта значения будут `1`, `2`, `3`; существующий счётчик продолжает увеличиваться без сброса.

## Команды клиента

Клиент по умолчанию подключается к `http://127.0.0.1:8899` и использует `~/.config/solana/id.json`. Параметры `--url` и `--keypair` позволяют выбрать RPC и файл кошелька.

| Команда в zsh/bash | Назначение |
| :-- | :-- |
| `cargo run -p counter-client --locked -- initialize` | Создать Counter со значением 0 |
| `cargo run -p counter-client --locked -- increment` | Увеличить Counter на 1 |
| `cargo run -p counter-client --locked -- show` | Прочитать PDA, authority и значение |
| `cargo run -p counter-client --locked -- demo` | Создать отсутствующий Counter и увеличить трижды |

## Проверки

После сборки выполните из корня репозитория в zsh/bash:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Девять LiteSVM-тестов в `clients/counter-client/tests/counter.rs` проверяют начальное состояние и PDA, сохранение увеличений, изоляцию кошельков, отклонение чужого authority, повторной инициализации, отсутствующей подписи и переполнения. Программа отклоняет подмену PDA, а клиент — аккаунт с чужим владельцем, пустыми данными или неверным discriminator.

Тесты используют те же builders инструкций и декодирование, что и RPC-клиент. Команда `demo` проверяет взаимодействие с запущенным validator.

Файлы кошельков, program keypair, seed phrase и приватные ключи нельзя добавлять в Git или публиковать в выводе документации.
