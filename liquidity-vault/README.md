# Liquidity Vault (недели 8–13)

DeFi-vault на Anchor 1.1.2 и Rust. Пользователи вносят SNTOK в общее хранилище и получают share-токены — квитанцию о своей доле. Админ обменивает токены vault через собственный CLMM-пул Raydium и размещает их в позиции с выбранным диапазоном цен, где они получают комиссии с обменов. Токенами распоряжается только программа от имени PDA.

**Program ID (devnet):** [`3XvGNievhhJ9fX5SR46uK4TeBpuCYVV548WpzE5CnmAK`](https://explorer.solana.com/address/3XvGNievhhJ9fX5SR46uK4TeBpuCYVV548WpzE5CnmAK?cluster=devnet)

| Неделя | Тема | Подробности |
| :-- | :-- | :-- |
| 8 | каркас vault | [раздел](#неделя-8) |
| 9 | депозит и share-токены | [раздел](#неделя-9) |
| 10 | CPI и вывод | [раздел](#неделя-10) |
| 11 | AMM и концентрированная ликвидность | [раздел](#неделя-11) |
| 12 | swap через Raydium | [раздел](#неделя-12) |
| 13 | позиция в CLMM-пуле | [раздел](#неделя-13) |

## Структура и запуск

| Путь | Что внутри |
| :-- | :-- |
| [`programs/liquidity-vault/`](programs/liquidity-vault/) | Anchor-программа vault и тесты LiteSVM |
| [`clients/vault-client/`](clients/vault-client/) | Rust-клиент: команды vault, пула Raydium и позиции |

Папка — отдельный Cargo workspace. Все команды выполняются в терминале macOS zsh/bash из `liquidity-vault/`. Нужны Rust 1.89.0 и Solana CLI 3.1.10: SBF собирается `cargo-build-sbf` этой версии, сборка 4.x не исполняется в LiteSVM 0.10.

```bash
# Собрать программу, проверить формат, типы и тесты
~/.local/share/solana/install/releases/3.1.10/solana-release/bin/cargo-build-sbf --manifest-path programs/liquidity-vault/Cargo.toml
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

```bash
# Список команд клиента
cargo run -p vault-client --locked -- --help
```

Клиент пишет только в devnet или localnet (проверка genesis), перед отправкой выполняет simulation и сверяет результат через RPC. По умолчанию используется кошелёк `../.keys/devnet-payer.json`; ключи и seed-фразы в репозиторий не попадают.


---

## Неделя 8

**Каркас vault**

Начинает **Liquidity Vault**: программа [`programs/liquidity-vault/`](programs/liquidity-vault/) принимает SNTOK пользователей в общее хранилище и выпускает share-токены — долю в нём. Rust-клиент [`clients/vault-client/`](clients/vault-client/) проверяет сеть по genesis (только devnet или localnet), выполняет simulation и читает результат через RPC.

**Program ID (devnet):** [`3XvGNievhhJ9fX5SR46uK4TeBpuCYVV548WpzE5CnmAK`](https://explorer.solana.com/address/3XvGNievhhJ9fX5SR46uK4TeBpuCYVV548WpzE5CnmAK?cluster=devnet). В devnet развёрнута сборка с инструкциями недель 8–13 под одним program ID; транзакции каждой недели вызывают только её инструкции.

### Схема аккаунтов

```mermaid
flowchart TD
    Admin([Админ]) -- initialize_vault --> VS
    User([Пользователь]) -- create_user_card --> UC
    subgraph program [liquidity_vault]
        VS["VaultState PDA<br/>vault + token_mint"]
        UC["UserCard PDA<br/>user_card + vault + user"]
    end
    subgraph token [Token-2022 Program]
        VT["Хранилище SNTOK PDA<br/>vault_tokens + vault"]
        SM["Share mint PDA<br/>share_mint + vault"]
    end
    VS -. authority .-> VT
    VS -. mint authority .-> SM
    UC -. vault .-> VS
```

| Аккаунт | Seeds | Владелец аккаунта | Кто распоряжается | Что хранит |
| :-- | :-- | :-- | :-- | :-- |
| `VaultState` | `["vault", token_mint]` | программа vault | только программа | admin, token mint, token program, хранилище, share mint, `total_deposited`, bump |
| Хранилище | `["vault_tokens", vault]` | Token-2022 Program | `VaultState` (authority) | внесённые SNTOK |
| Share mint | `["share_mint", vault]` | Token-2022 Program | `VaultState` (mint authority) | share-токены, 6 decimals как у SNTOK |
| `UserCard` | `["user_card", vault, user]` | программа vault | пользователь создаёт свою | vault, owner, `deposited`, `created_at`, bump |

Хранилищем и share mint распоряжается `VaultState` — PDA без приватного ключа. Подписать за него может только программа, поэтому ни админ, ни пользователь не заберут токены и не выпустят share-токены своей подписью. Все инструкции работают через `token_interface`: vault принимает и SPL Token, и Token-2022.

### Запуск vault

В терминале macOS zsh/bash из папки `liquidity-vault/`. SBF собирается `cargo-build-sbf` из Solana 3.1.10: сборка 4.x не исполняется в LiteSVM 0.10.

```bash
# Собрать программу и проверить её в LiteSVM
~/.local/share/solana/install/releases/3.1.10/solana-release/bin/cargo-build-sbf --manifest-path programs/liquidity-vault/Cargo.toml
cargo test -p liquidity-vault -p vault-client --locked
```

```bash
# Создать vault для SNTOK, свою карточку и прочитать состояние
cargo run -p vault-client --locked -- init
cargo run -p vault-client --locked -- card
cargo run -p vault-client --locked -- show
```

По умолчанию клиент использует devnet, кошелёк `../.keys/devnet-payer.json` и mint SNTOK; `--url`, `--keypair` и `--mint` меняют их. Повторный `init` или `card` не создаёт дубликат, а показывает существующие аккаунты.

### Аккаунты в devnet

| Что | Ссылка |
| :-- | :-- |
| Программа | [3XvG…mAK](https://explorer.solana.com/address/3XvGNievhhJ9fX5SR46uK4TeBpuCYVV548WpzE5CnmAK?cluster=devnet) |
| VaultState | [6k3E…eACp](https://explorer.solana.com/address/6k3EuoscnFuHwswqPxjhR28h9mAc75HkenhmcZmyeACp?cluster=devnet) |
| Хранилище SNTOK | [54zx…P9No](https://explorer.solana.com/address/54zxzWPstT9rmqtJmbbyyVYVBwpDQZxGbJwbyWK8P9No?cluster=devnet) |
| Share mint | [7wZX…Rjfe](https://explorer.solana.com/address/7wZXDcPC4LX7cg2cxtYCViGnMrBB7zHBHvaUPzyRRjfe?cluster=devnet) |
| Карточка пользователя | [98QR…qwWQ](https://explorer.solana.com/address/98QRYEXEsayEdn3BXHi5tRWwH1HwG9DZRzxmAXmuqwWQ?cluster=devnet) |
| Транзакция `initialize_vault` | [AzAh…VfX](https://explorer.solana.com/tx/AzAhUFPe6BZvxYg9ay15AQpkmhK4rWkG6zFQcduH9tG73HvrAek2dycDTFSZuvQm7di6XuaMjSnU7u5ZCkarVfX?cluster=devnet) |
| Транзакция `create_user_card` | [4pMT…pzs5](https://explorer.solana.com/tx/4pMTngMDzSsdckNZd7SrqCwsk65GatBam9vWrmYN3VGqgJJARhLEZTf9Ke3MUmgBgvmYTRZjNceLyFz5FtKvpzs5?cluster=devnet) |

### Проверьте себя

- **Почему share-токен выпускает программа, а не администратор?** Mint authority — `VaultState` PDA. Программа выпускает share-токены только по правилам депозита; иначе админ напечатал бы себе долю и забрал чужие токены.
- **Что такое PDA и почему у неё нет приватного ключа?** Адрес выводится из seeds и program ID так, что он не лежит на кривой Ed25519. Ключа нет, подпись даёт только программа через `invoke_signed`.
- **Как найти карточку конкретного пользователя?** Вычислить PDA `["user_card", vault, user]`; список пользователей хранить не нужно.

---

## Неделя 9

**Депозит и share-токены**

Добавляет `deposit(amount)`. Пользователь вносит SNTOK и получает share-токены — долю в общем хранилище:

1. **Расчёт доли.** Первый вкладчик получает share-токены 1:1. Дальше `shares = amount × все_shares ÷ токены_в_хранилище` в u128 с округлением вниз, в пользу vault.
2. **Перевод в хранилище.** CPI `transfer_checked` в Token-2022; подписывает пользователь, потому что двигаются его токены.
3. **Выпуск share-токенов.** CPI `mint_to`; подписывает программа от имени `VaultState` через seeds `["vault", token_mint, bump]`.
4. **Учёт.** Растут `VaultState.total_deposited` и `UserCard.deposited`.

Ноль запрещён (`ZeroAmount`). Вклад, за который положено 0 share, отклоняется (`ZeroShares`). Нужна своя карточка; списать можно только со своего токен-аккаунта. ATA для share-токенов создаётся при первом депозите. Пример методички закреплён тестом: в хранилище 1 100 токенов при 1 000 share, вклад 110 даёт 100 share.

```bash
# Внести 1000 SNTOK и проверить выпущенные доли через RPC
cargo run -p vault-client --locked -- deposit --amount 1000
```

Клиент заранее считает долю той же функцией `shares_for_deposit`, а после подтверждения сверяет полученные share-токены и баланс хранилища.

### Депозит в devnet

[Транзакция депозита 1000 SNTOK](https://explorer.solana.com/tx/3TqaKdzaifrmAUC2V2tYTsm129YCctqt4GG6EP4Qqp1iZYq8r2xHeipwX8hN76RH476N43k8ZVmCr6FNEjTXNmLU?cluster=devnet): хранилище было пустым, пользователь получил 1000 share.

```text
liquidity_vault · Deposit                         invoke [1]
├─ Associated Token · Create                      invoke [2]  ATA для share-токенов
│  ├─ Token-2022 · GetAccountDataSize             invoke [3]
│  ├─ System · CreateAccount                      invoke [3]
│  ├─ Token-2022 · InitializeImmutableOwner       invoke [3]
│  └─ Token-2022 · InitializeAccount3             invoke [3]
├─ Token-2022 · TransferChecked 1000 SNTOK        invoke [2]  подпись пользователя
└─ Token-2022 · MintTo 1000 share                 invoke [2]  подпись vault PDA
```

### Проверьте себя

- **Почему поздний вкладчик может получить share-токенов меньше, чем внёс токенов?** Если vault заработал, на каждый share приходится больше токенов. Иначе новый вкладчик забрал бы часть дохода ранних.
- **Почему перевод подписывает пользователь, а выпуск share-токенов — программа?** Переводятся токены пользователя, разрешить это может только он. Mint authority share-токена — PDA без ключа, за неё подписывает программа.
- **Какие программы участвовали в транзакции?** liquidity_vault, Associated Token, Token-2022 и System (см. дерево выше).

---

## Неделя 10

**Cpi и вывод**

Добавляет `withdraw(shares)`. **CPI** (Cross-Program Invocation) — вызов одной программы из другой: vault сам не двигает токены, а просит Token-2022 Program. Подпись бывает двух видов:

| Что двигается | Кто подписывает | Как в коде |
| :-- | :-- | :-- |
| Токены пользователя: перевод в vault, сжигание его share | пользователь, подпись передаётся по цепочке | `CpiContext::new` |
| Токены vault: выпуск share, выдача из хранилища | программа от имени `VaultState` PDA | `CpiContext::new_with_signer` с seeds |

`withdraw` проверяет `shares > 0` и что у пользователя их хватает (`InsufficientShares`). Затем считает `amount = shares × токены_в_хранилище ÷ все_shares`, сжигает share-токены (CPI `burn`, подпись пользователя) и переводит токены из хранилища (CPI `transfer_checked`, подпись vault PDA). `deposited` в карточке и `total_deposited` уменьшаются пропорционально возвращённой доле. Токены уходят только на собственный токен-аккаунт пользователя.

```bash
# Вернуть 400 share-токенов и получить свою долю
cargo run -p vault-client --locked -- withdraw --amount 400
```

### Вывод в devnet

[Транзакция вывода 400 share](https://explorer.solana.com/tx/2R2r5kfThUMBBAdd2Hvo8H3WUAQRyozogtKwFjHAARtS97zU9iUAAapPC2D3bDEYGHREB5TAxWExNbQ4RBPG5RXC?cluster=devnet): в хранилище было 1000 SNTOK при 1000 share. Пользователь вернул 400 share и получил 400 SNTOK; в vault осталось 600 SNTOK и 600 share.

CPI-вызовы в блоке **Inner Instructions** Explorer:

```text
liquidity_vault · Withdraw                    invoke [1]
├─ Token-2022 · Burn 400 share                invoke [2]  подпись пользователя
└─ Token-2022 · TransferChecked 400 SNTOK     invoke [2]  подпись vault PDA
```

`invoke [2]` в логах — второй уровень вызова, то есть CPI из программы vault.

### Проверьте себя

- **Что такое CPI и зачем он нужен?** Вызов другой программы из своей. Vault не пишет перевод токенов сам, а пользуется Token Program; так же дальше он вызывает Raydium.
- **Когда подписывает пользователь, а когда программа?** Пользователь — когда двигаются его токены. Программа от имени PDA — когда двигаются токены vault или выпускаются share.
- **Почему администратор не может забрать токены из vault своей подписью?** Authority хранилища — PDA, а не админ. В программе нет инструкции, которая переводит токены админу.

---

## Неделя 11

**Amm и концентрированная ликвидность**

**AMM** — биржа без книги заявок: в пуле лежат два токена, цену задаёт формула. В простом пуле `x × y = k`, и ликвидность размазана по всем ценам от нуля до бесконечности. В **CLMM** провайдер выбирает диапазон: пока цена внутри, его ликвидность участвует в обменах и получает комиссии; вышла за диапазон — перестаёт. Цены разбиты на **тики**, позиция в Raydium оформляется как NFT.

Создаёт второй токен, CLMM-пул Raydium и позицию с кошелька. Вместо TypeScript-скриптов `raydium-sdk-V2-demo` инструкции `create_pool` и `open_position_with_token22_nft` собираются в Rust-клиенте ([`raydium.rs`](clients/vault-client/src/raydium.rs)): порядок аккаунтов, seeds PDA и discriminator'ы взяты из исходников [raydium-clmm](https://github.com/raydium-io/raydium-clmm). Так клиент остаётся в Cargo workspace и не зависит от крейта Raydium, который закреплён на другой версии Anchor.

```bash
# Второй токен: 1 000 000 на кошелёк
cargo run -p vault-client --locked -- create-token
# Пул SNTOK / второй токен: 1 SNTOK = 2 второго токена, комиссия 0.25%
cargo run -p vault-client --locked -- create-pool --price 2
# Позиция ±10% вокруг цены: 10 000 второго токена, SNTOK по пропорции
cargo run -p vault-client --locked -- open-position --range 10 --amount 10000
# Цена, тик, ликвидность и позиция
cargo run -p vault-client --locked -- pool --position KNEY8omEeoCD2Kmx8qy7DZ3WUw46nXg8YtjhNiezLFa
```

### Адреса в devnet

| Что | Адрес |
| :-- | :-- |
| Токен vault — SNTOK (Token-2022) | [DhjR…otozW](https://explorer.solana.com/address/DhjRbgdts2DNEvbfEVQkuGkpwwioEWW87LdHV11otozW?cluster=devnet) |
| Второй токен (SPL Token, 6 decimals) | [YkBa…soFm](https://explorer.solana.com/address/YkBaLgL7tK1ffq3vrb5qXqeJZ1rk6LpHSqZvgDXsoFm?cluster=devnet) |
| Программа Raydium CLMM (devnet) | [DRay…9pfH](https://explorer.solana.com/address/DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH?cluster=devnet) |
| AMM config: комиссия 0.25%, шаг тиков 60 | [CD4a…Q1qu](https://explorer.solana.com/address/CD4aJtX11cqTCAc83nxSPkkh5JW2yjD6uwHeovjqQ1qu?cluster=devnet) |
| **Пул SNTOK / второй токен** | [HQoP…XrXYo](https://explorer.solana.com/address/HQoPzSGhYFrrbAcKXd1FcoWDNvAMtanhZ6ArAVXzrXYo?cluster=devnet) |
| **NFT позиции** | [KNEY…zLFa](https://explorer.solana.com/address/KNEY8omEeoCD2Kmx8qy7DZ3WUw46nXg8YtjhNiezLFa?cluster=devnet) |
| Создание второго токена | [транзакция](https://explorer.solana.com/tx/x5Degu45udo9vkgB7zhSviUBUejf1HMBPTfbSj5HAwUY7aPw1rNy7ciJP7P1TZ4PqoFsDmVAAyQJfy5uuVb9RbP?cluster=devnet) |
| Создание пула | [транзакция](https://explorer.solana.com/tx/5EnN3mNMndUAuRTbqc3VdFy6exWkJrrtihvHkCaDTewDZ2AcXvHxmR7p81V7wYjehe8oLhyRPrHbH6uvCoK1kHNE?cluster=devnet) |
| Открытие позиции | [транзакция](https://explorer.solana.com/tx/aSciKCdHb3f7q34V2jidLkiv7nHSdoQQrMRs88vyxtpYpXR2NPsrhSRJWDyxdJfABW85WRvCMBx9bLrrBJFWuCo?cluster=devnet) |

Остальные аккаунты пула (хранилища, observation, bitmap, tick-массивы) — PDA программы Raydium; клиент вычисляет их из адреса пула.

### Цена и диапазон

Raydium упорядочивает mint'ы по байтам адреса: в пуле **token0 — второй токен, token1 — SNTOK**, и цена пула хранится как «SNTOK за 1 второй токен».

| | |
| :-- | :-- |
| Текущая цена | **2 второго токена за 1 SNTOK** (0.5 SNTOK за второй токен, тик −6932) |
| Диапазон позиции | **1.8112 … 2.2344** второго токена за SNTOK (тики −8040 … −5940) |
| В позиции | 10 000 второго токена + 5 570.530016 SNTOK |

**Почему ±10%.** Правило методички: отступить от цены на величину обычного колебания. Узкий диапазон ±1–2% дал бы больше комиссий с той же суммы, но цена быстро выходила бы за границы. Широкий ±50% спокойнее, но по доходу почти не отличается от простого пула. ±10% держит ликвидность рядом с ценой, и обмены недель 12–13 не выбивают из неё позицию. Границы немного несимметричны (−9.4% / +11.7%), потому что округляются к шагу тиков 60 и считаются в обратной цене пула.

### Проверьте себя

- **Чем концентрированная ликвидность отличается от простого пула?** В простом пуле деньги распределены по всем ценам и почти все лежат без дела. В CLMM они работают только в выбранном диапазоне, поэтому с той же суммы получается больше комиссий.
- **Что происходит с позицией, когда цена выходит за диапазон?** Она перестаёт участвовать в обменах и зарабатывать и целиком превращается в один токен: выше верхней границы — в дорожающий токен уже продан, ниже нижней — позиция скупила дешевеющий.
- **Почему узкий диапазон зарабатывает больше, но требует больше внимания?** Та же ликвидность сосредоточена на меньшем числе тиков, и доля комиссий с каждого обмена больше. Но цена чаще выходит за границы, и позицию приходится переставлять.

---

## Неделя 12

**Swap через raydium**

Учит vault обменивать токены через свой CLMM-пул. Технически это тот же CPI, что на неделе 10: программа vault вызывает Raydium `swap_v2` и подписывает обмен от имени `VaultState` PDA.

### Стратегия и второе хранилище

| Аккаунт | Seeds | Владелец | Что хранит |
| :-- | :-- | :-- | :-- |
| `Strategy` | `["strategy", vault]` | программа vault | пул Raydium, второй токен, второе хранилище, NFT открытой позиции vault |
| Второе хранилище | `["second_tokens", vault]` | Token Program, authority — `VaultState` | второй токен пула |

`init_strategy` вызывает только админ, один раз. Программа проверяет, что пул принадлежит Raydium CLMM, имеет discriminator `PoolState` и содержит SNTOK и второй токен (`PoolMintMismatch`). Отдельный аккаунт оставляет `VaultState` в прежнем layout: vault в devnet уже создан.

`swap(amount_in, minimum_amount_out, sell_vault_token)`:

- только админ (`has_one = admin`) и только через пул стратегии (`WrongPool`);
- токены входа и выхода — хранилища vault, админ не может направить обмен на свой кошелёк;
- CPI `swap_v2` собирается вручную ([`raydium.rs`](programs/liquidity-vault/src/raydium.rs)) и подписывается seeds vault PDA; bitmap-расширение и tick-массивы пула передаются как remaining accounts;
- `minimum_amount_out` защищает от slippage: Raydium отменяет обмен с ошибкой 6018 `TooLittleOutputReceived`, а программа дополнительно сверяет полученную сумму после CPI.

Ожидаемую сумму клиент берёт из simulation той же инструкции с нулевым минимумом: RPC возвращает состояние выходного хранилища после обмена. Затем минимум ставится на 1% ниже ожидаемого (`--slippage-bps 100`).

```bash
# Подключить пул и создать второе хранилище
cargo run -p vault-client --locked -- strategy
# Vault продаёт 100 SNTOK, минимум = ожидаемое - 1%
cargo run -p vault-client --locked -- vault-swap --direction sell --amount 100
# Минимум вдвое выше ожидаемого: транзакция уходит без preflight и отклоняется Raydium
cargo run -p vault-client --locked -- vault-swap --direction sell --amount 50 --reject
# Обратный обмен: 50 второго токена -> SNTOK
cargo run -p vault-client --locked -- vault-swap --direction buy --amount 50
```

### Результаты в devnet

| Что | Ссылка |
| :-- | :-- |
| Strategy | [D1Ve…LraZu](https://explorer.solana.com/address/D1VehnSHMWzR2P1hZbWkqX3DyzjZYR1aivBD1xSLraZu?cluster=devnet) |
| Второе хранилище | [6PCJ…JWhc](https://explorer.solana.com/address/6PCJ1kToMrcCMSskDXujdxzGMVJwyv91G4DMfaFqJWhc?cluster=devnet) |
| `init_strategy` | [транзакция](https://explorer.solana.com/tx/4kwe6fUA6foz8N7pgmBX6vKhEXsG9CdZCkQiso9JitVMEsV63SkS2m6oTj6Ym8yrvTqPok1ZqNr9gDX72V6dcWCU?cluster=devnet) |
| **Успешный swap: 100 SNTOK → 199.307636 второго токена** (минимум 197.314559) | [транзакция](https://explorer.solana.com/tx/68z7FPPShbdAL8bCCQe5UcRt2ody7mW3DWdKNHfSxwunBFnVfqdN5jjG4jtD7K5mKLvtfxndk5VdVYki1YFw8ka?cluster=devnet) |
| **Отклонённый из-за slippage:** ожидалось 99.509753, минимум 199.019506 | [транзакция](https://explorer.solana.com/tx/5dU3WPW6BdxgLYv5GBvv9CkyMNCtGebWiSLJhCdPPnKei72bR1rfRWFAPCUErh6ZN2cetJNS5zP8wEHJvekDnLut?cluster=devnet) — ошибка 6018 (`0x1782`), балансы vault не изменились |
| Обратный swap: 50 второго токена → 24.979627 SNTOK | [транзакция](https://explorer.solana.com/tx/Dhq8iVUEASBtfpFAQeJHEGTfYKwfZRY1JmhsLDzkNePiwHmmRqxifvZTgGCSFbcrByidMk8R2ZK7h6u8vYmA4Fg?cluster=devnet) |

**Как изменилась цена пула после продажи 100 SNTOK:**

| | Цена | В vault |
| :-- | :-- | :-- |
| До | 2.000000 второго токена за SNTOK | 600 SNTOK + 0 второго токена |
| После | **1.996145** (−0.193%) | 500 SNTOK + 199.307636 второго токена |

Vault продал SNTOK в пул, SNTOK в пуле стало больше, и он подешевел. Получено 199.31, а не 200: часть забрала комиссия 0.25%, часть — сдвиг цены по ходу обмена. После обратного обмена цена поднялась до 1.997109.

Share-курс считается по балансу основного хранилища SNTOK. Упрощение методички: перед выводом пользователей админ меняет второй токен обратно в SNTOK.

### Проверьте себя

- **Что такое slippage и как от него защищаются?** Разница между ожидаемой и фактической ценой обмена: пока транзакция идёт в сеть, другие меняют цену. Защита — минимальная сумма на выходе; если пул отдаёт меньше, обмен отменяется целиком.
- **Почему цена сдвигается сильнее, если около текущей цены мало ликвидности?** Обмен проходит по тикам, пока не наберёт сумму. Чем меньше ликвидности рядом, тем больше тиков он пройдёт и тем дальше уйдёт цена.
- **Почему обмен подписывает программа, а не администратор?** Токены лежат в хранилищах vault, их authority — PDA. Админ только разрешает действие, а отдать токены может программа, и только через пул стратегии.

---

## Неделя 13

**Позиция в clmm-пуле**

Делает vault провайдером ликвидности: он кладёт оба токена в выбранный диапазон пула, **NFT позиции принадлежит vault PDA**, комиссии с обменов приходят в хранилища vault. Адрес NFT открытой позиции хранится в `Strategy`, поэтому у vault одна позиция за раз.

| Инструкция | Кто | CPI в Raydium |
| :-- | :-- | :-- |
| `open_position(tick_lower, tick_upper, …, amount_0_max, amount_1_max, base_0)` | только админ | `open_position_with_token22_nft` без ликвидности: ренту платит админ, NFT получает vault; затем `increase_liquidity_v2` подписью vault PDA из хранилищ vault |
| `collect_fees` | только админ | `decrease_liquidity_v2` с ликвидностью 0: позиция не меняется, Raydium выплачивает накопленные комиссии |
| `close_position(amount_0_min, amount_1_min)` | только админ | `decrease_liquidity_v2` на всю ликвидность с минимумами против slippage, затем `close_position`: NFT сжигается, рента возвращается vault |

**Почему открытие в два шага.** В `open_position` Raydium один и тот же аккаунт платит за новые аккаунты и отдаёт токены. Vault — PDA с данными и не может быть payer в System Program. Поэтому позиция открывается пустой (Raydium разрешает это при `base_flag = None`), а ликвидность добавляет сам vault.

**Защита.** Токен-аккаунты позиции обязаны быть хранилищами vault (`NotVaultStorage`): админ не может направить токены или комиссии себе. Пул — только пул стратегии (`WrongPool`), позиция — только записанная в стратегии (`WrongPosition`), вторая позиция поверх открытой запрещена (`PositionAlreadyOpen`). Минимумы закрытия клиент берёт из simulation и снижает на 1%.

```bash
# Позиция vault ±5% вокруг цены: 100 второго токена, SNTOK по пропорции
cargo run -p vault-client --locked -- position-open --range 5 --amount 100
# Обмены с кошелька напрямую в пуле — как сторонний трейдер
cargo run -p vault-client --locked -- trade --direction sell --amount 40
cargo run -p vault-client --locked -- trade --direction buy --amount 80
# Забрать комиссии, посмотреть позицию
cargo run -p vault-client --locked -- position-collect
cargo run -p vault-client --locked -- position
# Ребалансировка: закрыть и открыть заново вокруг текущей цены
cargo run -p vault-client --locked -- rebalance --range 5 --amount 45
# Закрыть позицию
cargo run -p vault-client --locked -- position-close
```

### Сценарий в devnet

| Шаг | Что произошло | Транзакция |
| :-- | :-- | :-- |
| 1. Открыть позицию | цена 1.997109 второго токена за SNTOK, диапазон **±5%: 1.900217 … 2.104258**; vault отдал **100 второго токена + 52.58567 SNTOK** | [open](https://explorer.solana.com/tx/3Cyj9pgCiEmSByUDXHZpGb4ajCVrNh2YzzDgbcFkSMRGNgXZnRMvFyVVnTUUNXPZNk6CW8Lp4q8JQHh5tjcGYA5m?cluster=devnet) |
| 2. Обмены с другого кошелька | продажа 40 SNTOK и покупка SNTOK на 80 второго токена | [sell](https://explorer.solana.com/tx/4d81N4Q4KfwGABeSmAfxDhPHXkAa8DbEck76Ng4da8TjHyXBKYou5ADnVfNpLb5ioxso2oKEaf63kEPivBajhvcb?cluster=devnet), [buy](https://explorer.solana.com/tx/5kzpswowEoK7XU1yCFmMcc2JLkzCdMKyfLJCnfWVkTEDg8ETHNUBWTgpb6T1Fqd8BckwUCms83FA5jeqSumqhKUT?cluster=devnet) |
| 3. Собрать комиссии | **+0.003247 второго токена, +0.001623 SNTOK** | [collect_fees](https://explorer.solana.com/tx/2DxhRWmoaAFx6sRL9ZCGRZjRFGTzUBCGJHPWgzfUVZtjMiV6FyrPPMturpbXsLVHWRT4uSNeYpeVTiyWwpZLAZdF?cluster=devnet) |
| 4. Цена ушла из диапазона | продажа 3000 SNTOK: цена **1.888194** ниже границы 1.900217, позиция перестала зарабатывать | [sell 3000](https://explorer.solana.com/tx/5pR37ipWseUaZmHTPe4fUAkZMzcKj2s3zc2oMmbXG8PfAPW1YgwSxhcRm78npofb8jxdDKaiiVB7HB6Tc9qduTse?cluster=devnet) |
| 5. Ребалансировка: закрыть | вернулось **104.026845 SNTOK + 0 второго токена** | [close](https://explorer.solana.com/tx/yzzL4razgTQAU1yPvqXGZaN5UoqoM9aYGJecsV63ZUx5B3hEWkypRAFPrncTU9kgdDJzDvrJSJUJDZMuKsvNcfj?cluster=devnet) |
| 6. Ребалансировка: открыть | новый диапазон ±5% вокруг 1.888194: **1.789562 … 1.993647**; vault отдал 45 второго токена + 24.136873 SNTOK | [open](https://explorer.solana.com/tx/PHnvwdgqR53yeZQLBbfQxStDNpexQpJ2PY9vxmoh5kR3QVwz8JJc4K4Yzubv4433SjHDsgA73Z5MLgXxfMTdMLc?cluster=devnet) |
| 7. Закрыть | вернулось **44.999999 второго токена + 24.136872 SNTOK** (цена не менялась; 1 минимальная единица ушла на округление в пользу пула) | [close](https://explorer.solana.com/tx/5zVAMvh5qtqYr93a2TEb4j3w9JB5PeNnKucZDirierJH9zmbcEuvaPiWJz433xmNUDJhWfWqrZvc6nLWsHLz6xzr?cluster=devnet) |

После сценария позиций у vault нет: в хранилищах 576.422424 SNTOK и 49.310882 второго токена.

- **Комиссии маленькие**: ликвидность позиции vault (2.88·10⁹) — около 2% ликвидности позиции недели 11 (1.46·10¹¹), а комиссия обмена делится между позициями пропорционально ликвидности.
- **После выхода цены ниже диапазона позиция целиком стала SNTOK**: пока цена падала, трейдеры продавали в неё SNTOK и забирали второй токен. По цене 1.888194 полученные 104.03 SNTOK стоят ≈196.42 второго токена, а просто хранение 100 второго токена + 52.59 SNTOK стоило бы ≈199.29. Разница ≈1.44% — **impermanent loss**, поэтому позицию переставляют вовремя.

Упрощение методички: пока средства в позиции, пользователи их не выводят. Перед выводом админ закрывает позицию и меняет второй токен обратно в SNTOK (`vault-swap --direction buy`).

### Проверьте себя

- **Почему соотношение токенов в позиции зависит от того, где цена внутри диапазона?** Ликвидность — лестница заявок: выше текущей цены позиция держит SNTOK на продажу, ниже — второй токен на покупку. У нижней границы почти всё в SNTOK, у верхней — во втором токене, посередине — примерно поровну по стоимости.
- **Что будет, если цена уйдёт выше верхней границы?** Весь SNTOK будет продан, позиция станет целиком вторым токеном и перестанет зарабатывать, пока цена не вернётся или позицию не переставят.
- **Зачем нужна ребалансировка и кто её делает?** Чтобы позиция снова была вокруг цены и получала комиссии. Делает админ vault: закрывает старую позицию и открывает новую; в Kamino и Meteora это автоматизировано.
