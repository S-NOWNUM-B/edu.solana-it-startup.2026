<div align="center">

# Solana SNOWNUMB TOKEN

Токен SNTOK, DeFi через Orca Whirlpools и dashboard публичных данных Solana: балансы, токены, транзакции и активность программ.

![Rust](https://img.shields.io/badge/Rust-1.89.0-000000?style=for-the-badge&logo=rust&logoColor=white)
![Anchor](https://img.shields.io/badge/Anchor-1.1.2-6750A4?style=for-the-badge)
![Solana](https://img.shields.io/badge/Solana-3.1.10-14F195?style=for-the-badge&logo=solana&logoColor=black)
![LiteSVM](https://img.shields.io/badge/LiteSVM-0.10.0-334155?style=for-the-badge)

</div>

## Содержание

- [Задание 10 RPC dashboard](#задание-10-rpc-dashboard)
- [Задание 9 DeFi на Solana](#задание-9-defi-на-solana)
- [Задание 7 SPL Token и Token Extensions](#задание-7-spl-token-и-token-extensions)
- [О проекте](#о-проекте)
- [Технологии](#технологии)
- [Архитектура](#архитектура)
- [Профиль](#профиль)
- [Запуск](#запуск)
- [Команды клиента](#команды-клиента)
- [Проверки](#проверки)

## Задание 10 RPC dashboard

Ветка `task/10-rpc-dashboard` добавляет веб-dashboard **SNOWNUMB CHAIN** поверх Rust RPC-клиента. Он читает публичный адрес кошелька в devnet и показывает баланс SOL, токен-счета SPL Token и Token-2022, последние транзакции кошелька и активность Orca Whirlpools и Token-2022 в сети.

### Запуск dashboard

Из корня репозитория в терминале macOS zsh/bash:

```bash
cargo run -p dashboard-client --locked
```

Откройте [dashboard](http://127.0.0.1:8787) в браузере. Начальный адрес — кошелёк SNTOK из задания 9. Можно ввести другой публичный адрес и нажать «Обновить данные». Подключение кошелька и подпись не нужны.

Другой порт, начальный адрес или devnet RPC в zsh/bash:

```bash
cargo run -p dashboard-client --locked -- --port 8787 --wallet <PUBKEY> --url https://api.devnet.solana.com
```

Сервер слушает `127.0.0.1` и проверяет genesis hash devnet до запуска. Он выполняет только чтение через RPC. HTML, CSS и JavaScript встроены в бинарник через `include_str!`, поэтому после изменения `web/` перезапустите `cargo run`, чтобы пересобрать интерфейс. Остановка — Ctrl+C в терминале.

### Данные и HTTP API

| Раздел | Источник и поведение |
| :-- | :-- |
| Баланс SOL | [getBalance](https://solana.com/docs/rpc/http/getbalance), сумма в lamports и слот ответа |
| Токены | [getTokenAccountsByOwner](https://solana.com/docs/rpc/http/gettokenaccountsbyowner) с `jsonParsed` для обоих token program; mint, decimals, raw amount и токен-счёт |
| Транзакции кошелька | [getSignaturesForAddress](https://solana.com/docs/rpc/http/getsignaturesforaddress), до 6 последних подписей, результат и слот |
| Активность программ | По 6 последних подписей Orca Whirlpools и Token-2022 во всей сети devnet |

Имя и символ SNTOK берутся из существующего `clients/token-client/metadata.json`. Остальные неизвестные mint показываются по адресу. Строка токенов соответствует отдельному токен-счёту; активы в сводке считаются по уникальным mint. Ленты показывают последние записи, а не общий счётчик за период.

| HTTP endpoint | Ответ |
| :-- | :-- |
| `GET /api/config` | Начальный кошелёк, RPC, сеть и metadata SNTOK |
| `GET /api/dashboard?wallet=<PUBKEY>` | `balance`, `tokens`, `transactions`, `programs`, `errors`; без query используется начальный кошелёк |

Commitment — `confirmed`. `balance.lamports` и `tokens[].amount` передаются строками целых минимальных единиц. Интерфейс форматирует raw amount по decimals без float и потери точности. Ошибка RPC возвращает `null` для недоступного раздела и сообщение в `errors`; успешные пустые данные возвращаются пустым массивом. UI различает загрузку, пустой результат и ошибку. Каждый RPC-вызов ограничен 8 секундами, включая повторные запросы при HTTP 429. Неверный адрес отклоняется с HTTP 400, методы записи — с HTTP 405.

### Проверки dashboard

В терминале macOS zsh/bash:

```bash
cargo fmt --all -- --check
cargo clippy -p dashboard-client --all-targets --locked -- -D warnings
cargo test -p dashboard-client --locked
cargo build -p dashboard-client --locked
```

Три теста проверяют входной адрес, точность сумм больше 2^53 и владельца токен-счёта. Live API и браузер показали **0.1772994 SOL** и **874 509.707785 SNTOK** в проверенном снимке кошелька. Баланс SOL совпал с независимым чтением Solana CLI; доступны 6 транзакций кошелька и обе ленты программ. Проверены desktop/mobile, смена адреса, неверный адрес, HTTP 400/405/404 и ограничение HTTP 429 с частичным результатом.

Исходники: `clients/dashboard-client/src/main.rs` и `clients/dashboard-client/web/`. Word-отчёт со скриншотами dashboard сохраняется локально.

## Задание 9 DeFi на Solana

Ветка `task/09-solana-defi` интегрирует **Orca Whirlpools** через Rust SDK 8.0.0. `clients/defi-client/` создаёт пул **SNTOK/wSOL**, открывает позицию ликвидности, получает котировку и выполняет обмен SOL → SNTOK. Используется существующий mint **SNOWNUMB TOKEN (SNTOK)** из задания 7; имя, символ и описание The Best Token синхронизированы в JSON и onchain metadata.

### Протоколы и выбор интеграции

| Протокол | Возможности и способ интеграции |
| :-- | :-- |
| [Orca Whirlpools](https://docs.orca.so/) | Пулы, позиции ликвидности и swaps; [официальный Rust SDK](https://github.com/orca-so/whirlpools/tree/main/rust-sdk/whirlpool) строит инструкции по данным RPC |
| [Raydium Trade API](https://github.com/raydium-io/raydium-docs-v1/blob/main/sdk-api/trade-api.mdx) | HTTP-котировка маршрута и построение транзакции, которую клиент подписывает и отправляет |

Выбран Orca: Rust SDK позволяет интегрировать и ликвидность собственного Token-2022, и обмен в devnet в существующий Cargo workspace. Raydium рассмотрен как другой способ интеграции swap; клиент использует Orca.

### Пул и позиция

| Объект в devnet | Адрес |
| :-- | :-- |
| SNTOK mint | `DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW` |
| Orca program | `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc` |
| [SNTOK/wSOL pool](https://explorer.solana.com/address/HWvtqsdnTpEKBzPwDPDLtmfgtF3zGv1D7J8mTq5Btd9w?cluster=devnet) | `HWvtqsdnTpEKBzPwDPDLtmfgtF3zGv1D7J8mTq5Btd9w` |
| Position mint | `2PLHgGVJm9g3dN28tuJCBmDLuioqLMte5MdXwFEyTeyk` |

Splash pool использует позицию на полном диапазоне. `setup` вносит до 0.005 SOL и 500 SNTOK; начальная цена задана как 100 000 SNTOK/SOL для этого пула. Комиссия пула — 1%. Цена котировки зависит от состояния и ликвидности пула.

### Команды DeFi-клиента

Из корня проекта в терминале macOS zsh/bash, с уже пополненным devnet-кошельком владельца SNTOK:

```bash
cargo run -p defi-client --locked -- setup --keypair .keys/devnet-payer.json
cargo run -p defi-client --locked -- quote --owner 9KmHj9ZwNcPysvJM3CYAuc7Jp1hzVbEC7QaBAT7uD7N2
cargo run -p defi-client --locked -- swap --keypair .keys/devnet-payer.json
```

- `setup` создаёт отсутствующий пул и одну позицию; повторный запуск с `.keys/defi-position.json` проверяет NFT владельца и использует существующую позицию без нового внесения. Файл сохраняется до отправки: при сбое нужно проверить статус позиции, прежде чем запускать создание заново.
- `quote` читает котировку без файла кошелька при `--owner`. По умолчанию вход — 100 000 lamports (0.0001 SOL), slippage — 100 bps (1%).
- `swap` заново получает котировку, симулирует подписанную транзакцию, подтверждает отправку и сверяет прирост SNTOK с минимальным выходом.
- `--input-lamports <INTEGER>` и `--slippage-bps <1..1000>` задают параметры quote/swap; `--url` задаёт RPC, `--position-keypair` — путь позиции для setup.

Перед операциями клиент проверяет genesis hash devnet, владельца программы пула, его config и mints. Суммы рассчитываются в целых минимальных единицах. Симуляция ограничивает расход setup до 0.18 SOL, swap — входом плюс 100 000 lamports на сетевые расходы. Ключи остаются в игнорируемой `.keys/`, Word-отчёты сохраняются локально.

### Подтверждённый результат

Создание пула и внесение ликвидности прошли симуляцию и подтверждение. Обмен **0.0001 SOL → 9.707785 SNTOK** выполнен при минимальном выходе **9.610707 SNTOK**. Расход SOL с сетевой комиссией — **0.00011 SOL**; комиссия сделки 0.000001 SOL входит в сумму обмена. Баланс изменился с 874 500 до **874 509.707785 SNTOK**; независимое чтение через SPL Token CLI подтвердило результат.

- [Создание пула](https://explorer.solana.com/tx/5f65VLtHLhxHgh9MZpDkHMZJHBNBWnAPtsowxPx3x9Af81UBrb4Hdha39grABUrhjWjjxBs1ruB8LoTgfDpagBK2?cluster=devnet).
- [Открытие позиции и ликвидность](https://explorer.solana.com/tx/5EMGEQRfXf79BPvWjzjE1tSxt2MiUAZwicBwujn5aA81t7Y28PwdxmjXRwPLBS63jcuBHGvf4K5Eh1un3QZKKF2b?cluster=devnet).
- [Обмен SOL → SNTOK](https://explorer.solana.com/tx/JnsUbkhJHCfQL8MrxTTxFa6s8bJDV4XYG5YYdjaooyRP9TqqVe3gqL9AAbdHs6A1D7HRe7prgGXMyh4MmUNVpba?cluster=devnet).

Повторный `setup` подтвердил использование той же позиции без транзакции. Запрос quote к mainnet RPC отклонён проверкой сети до чтения кошелька и отправки. Format, Clippy и три теста клиента прошли:

```bash
cargo fmt --all -- --check
cargo clippy -p defi-client -p token-client --all-targets --locked -- -D warnings
cargo test -p defi-client --locked
cargo build -p defi-client --locked
```

## Задание 7 SPL Token и Token Extensions

Ветка `task/09-solana-defi` сохраняет минимальный Rust RPC-клиент для собственного токена **SNOWNUMB TOKEN (SNTOK)**. Описание токена — The Best Token. Клиент напрямую вызывает существующий Token-2022; отдельный смарт-контракт для стандартного токена не требуется.

- `demo` создаёт новый mint с 6 decimals, `MetadataPointer` на сам mint и `TokenMetadata` с name, symbol и URI.
- Создаёт ATA отправителя и второго кошелька, выпускает 1 000 000 SNTOK через `mint_to_checked` и переводит 125 000 через `transfer_checked`.
- После подтверждения читает RPC и проверяет metadata, владельцев аккаунтов, оба баланса и supply. Итог: 875 000 у отправителя, 125 000 у получателя, supply 1 000 000.
- `show <MINT>` читает публичные данные без файла кошелька.

Исходники находятся в `clients/token-client/src/main.rs`, публичный JSON для URI — в `clients/token-client/metadata.json`. Mint authority и update authority остаются у отправителя; freeze authority отсутствует. Суммы передаются целыми `u64` в минимальных единицах: 1 SNTOK = 1 000 000 единиц. Расширение metadata увеличивает mint, поэтому клиент заранее оплачивает rent за полный размер.

### Запуск токена

Все команды ниже выполняются из корня проекта в терминале macOS zsh/bash. Используйте localnet или devnet и тестовый SOL. Кошельки хранятся только локально в игнорируемой `.keys/`.

Создать отдельные кошельки, сохранив существующие файлы:

```bash
mkdir -p .keys
if [ ! -e .keys/token-payer.json ]; then
  solana-keygen new --no-bip39-passphrase --silent --outfile .keys/token-payer.json
fi
if [ ! -e .keys/token-recipient.json ]; then
  solana-keygen new --no-bip39-passphrase --silent --outfile .keys/token-recipient.json
fi
```

Пополнить отправителя тестовым SOL и выполнить демонстрацию:

```bash
solana airdrop 1 --url devnet --keypair .keys/token-payer.json
cargo run -p token-client --locked -- demo \
  --url https://api.devnet.solana.com \
  --keypair .keys/token-payer.json \
  --recipient "$(solana-keygen pubkey .keys/token-recipient.json)"
```

При ограничении devnet faucet используйте уже пополненный devnet-кошелёк через `--keypair`. Каждый `demo` создаёт **новый** mint. Для повторного чтения используйте адрес из вывода:

```bash
cargo run -p token-client --locked -- show <MINT> --url https://api.devnet.solana.com
```

Для localnet запустите `solana-test-validator --ledger .anchor/token-ledger` в отдельном терминале zsh/bash, используйте `solana airdrop 1 --url localhost --keypair .keys/token-payer.json` и `--url http://127.0.0.1:8899`. Для задания 7 не нужна Anchor-сборка или загрузка своей программы.

Обновить metadata существующего mint из `metadata.json`, сохранив supply и адрес:

```bash
cargo run -p token-client --locked -- update-metadata DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW \
  --url https://api.devnet.solana.com --keypair .keys/devnet-payer.json
```

Имя, символ и описание клиента берутся из одного `metadata.json`. Команда обновляет onchain-поля name, symbol, uri и description, доплачивает rent при увеличении metadata и проверяет сохранность mint.

### Проверки токен-клиента

В терминале macOS zsh/bash:

```bash
cargo fmt -p token-client -- --check
cargo build -p token-client --locked
cargo clippy -p token-client --all-targets --locked -- -D warnings
```

Проверка взаимодействия с сетью — полный `demo` с итоговым `PASS`, затем независимый `show`. `PASS` выводится только после проверки данных RPC. Неизвестные команды, отсутствующий recipient, неверные адреса и пропущенные значения параметров отклоняются до отправки транзакций.

Подтверждённые операции в **devnet**:

- [Обновление metadata](https://explorer.solana.com/tx/56CB2A1RNsK9pfEYDTD3aj5WRKN3SDNkkzZS1uBXpq6M6AM3FqrsNYygrqqrFHrFTWBHrgPjJCkQZmWXaNdixvKX?cluster=devnet).
- [Mint SNOWNUMB TOKEN](https://explorer.solana.com/address/DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW?cluster=devnet).
- [Создание mint и metadata](https://explorer.solana.com/tx/27ouwgjsVDoNr8PCvyW7fmFDkCzEiRM22kzpgjNwMEffBLEcpchCtg2BoophNb3Mq76km2dGbwCH57GZqkG6TWx1?cluster=devnet).
- [Выпуск 1 000 000 SNTOK](https://explorer.solana.com/tx/33Ae8Wr8AQ6iPRd1KmmUhn8uUmkzfCAMfMzC3XjKfF5epazxfgZoPSHVQpqjHXYPUYpzSrDgcSSSjW4mH4fbZitU?cluster=devnet).
- [Перевод 125 000 SNTOK](https://explorer.solana.com/tx/2AzKg6955QUmVhXNVsBtkQSoB9n23nLuKi9LgfMJgGMVtRRju4rRnqgG5Q2nyADNZL7H5wnkZpAYGDCQ6eJQT39M?cluster=devnet).

Официальные источники: [Token-2022 Metadata](https://solana.com/docs/tokens/extensions/metadata), [Token Extensions](https://www.solana-program.com/docs/token-2022/extensions).


## О проекте

Задание №6 реализует onchain-профиль: кошелёк создаёт аккаунт с именем и описанием, обновляет его и читает через RPC-клиент. Данные хранятся в PDA, адрес которого вычисляется по публичному ключу владельца.

В репозитории сохранены Counter с отдельным Rust-клиентом и токен-программа для создания mint и associated token account, выпуска и перевода токенов через `transfer_checked`.

Задание находится в ветке `task/06-onchain-profile`. Нумерация `task/*` независима от заданий онлайн-платформы в ветках `platform/01-tests`, `platform/02-burn` и `platform/03-escrow`.

## Технологии

| Часть проекта | Технологии |
| :-- | :-- |
| Программы | Rust 1.89.0, Anchor 1.1.2, Solana CLI 3.1.10 |
| Dashboard | Rust, tiny_http 0.12.0, Solana RPC 3.1.14, HTML/CSS/JavaScript |
| DeFi-клиент | Rust, Orca Whirlpools SDK/client 8.0.0, Tokio 1.53.1 |
| Token-клиент | Rust, `solana-rpc-client` 3.1.14, Token-2022 interface 2.1.0, metadata interface 0.8.0 |
| Profile-клиент | Rust, Cargo, `solana-rpc-client` 3.1.14 |
| Тесты программ | Rust, LiteSVM 0.10.0 |

## Архитектура

| Каталог | Ответственность |
| :-- | :-- |
| `clients/dashboard-client/` | HTTP API публичных данных и веб-dashboard Solana |
| `clients/defi-client/` | Пул SNTOK/wSOL, позиция ликвидности, quote и swap через Orca |
| `clients/token-client/` | Mint с metadata, выпуск и перевод SNTOK, публичное чтение |
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
