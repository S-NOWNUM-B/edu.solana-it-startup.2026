"use strict";

const form = document.querySelector("#wallet-form");
const walletInput = document.querySelector("#wallet");
const refreshButton = document.querySelector("#refresh");
const status = document.querySelector("#status");
const overview = document.querySelector("#overview");
let requestInFlight = false;

function node(tag, text, className) {
  const element = document.createElement(tag);
  if (text !== undefined) element.textContent = text;
  if (className) element.className = className;
  return element;
}

function short(address, size = 7) {
  return `${address.slice(0, size)}…${address.slice(-size)}`;
}

function explorer(kind, address, label = short(address)) {
  const link = node("a", label, "mono");
  link.href = `https://explorer.solana.com/${kind}/${encodeURIComponent(address)}?cluster=devnet`;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  link.title = address;
  link.setAttribute("aria-label", `${kind === "tx" ? "Транзакция" : "Адрес"} ${address} в Explorer`);
  return link;
}

function quantity(value) {
  const [whole, fraction] = String(value).split(".");
  return whole.replace(/\B(?=(\d{3})+(?!\d))/g, "\u202f") + (fraction ? `.${fraction}` : "");
}

function tokenQuantity(amount, decimals) {
  const digits = String(amount).padStart(decimals + 1, "0");
  if (decimals === 0) return quantity(digits);
  const whole = digits.slice(0, -decimals);
  const fraction = digits.slice(-decimals).replace(/0+$/, "");
  return quantity(whole + (fraction ? `.${fraction}` : ""));
}

function emptyRow(target, columns, message, failure = false) {
  const row = node("tr");
  const cell = node("td", message, `empty${failure ? " failure" : ""}`);
  cell.colSpan = columns;
  row.append(cell);
  target.replaceChildren(row);
}

function renderTransactions(target, transactions, errorMessage) {
  if (transactions === null) return emptyRow(target, 3, errorMessage || "RPC не вернул транзакции. Обновите данные.", true);
  if (transactions.length === 0) return emptyRow(target, 3, "Транзакций по этому адресу нет.");
  target.replaceChildren(...transactions.map(transaction => {
    const row = node("tr");
    const result = node("td");
    const label = node("span", transaction.success ? "Успешно" : "Ошибка", `result${transaction.success ? "" : " failed"}`);
    label.title = transaction.confirmation_status;
    result.append(label);
    const signature = node("td");
    signature.append(explorer("tx", transaction.signature));
    row.append(result, signature, node("td", quantity(transaction.slot), "align-right mono value"));
    return row;
  }));
}

function renderDashboard(data) {
  document.querySelector("#sol-balance").textContent = data.balance ? quantity(data.balance.sol) : "—";
  document.querySelector("#slot").textContent = data.balance ? quantity(data.balance.slot) : "—";
  document.querySelector("#token-count").textContent = data.tokens === null ? "—" : new Set(data.tokens.map(token => token.mint)).size;
  document.querySelector("#transaction-count").textContent = data.transactions === null ? "—" : data.transactions.length;
  const errorFor = section => data.errors.find(error => error.section === section)?.message;
  const tokens = document.querySelector("#tokens");
  if (data.tokens === null) {
    emptyRow(tokens, 4, errorFor("tokens") || "RPC не вернул токены. Обновите данные.", true);
  } else if (data.tokens.length === 0) {
    emptyRow(tokens, 4, "У кошелька нет токен-счетов SPL Token или Token-2022.");
  } else {
    tokens.replaceChildren(...data.tokens.map(token => {
      const row = node("tr");
      const asset = node("td");
      const name = explorer("address", token.mint, token.name);
      name.className = "asset-name";
      asset.append(name, node("span", token.symbol, "asset-symbol"));
      asset.title = `Токен-счёт: ${token.account}`;
      const mint = node("td");
      mint.append(explorer("address", token.mint));
      row.append(asset, mint, node("td", token.program.startsWith("Tokenz") ? "Token-2022" : "SPL Token", "standard"), node("td", tokenQuantity(token.amount, token.decimals), "align-right mono value"));
      return row;
    }));
  }
  renderTransactions(document.querySelector("#transactions"), data.transactions, errorFor("transactions"));
  const walletLink = document.querySelector("#wallet-explorer");
  walletLink.href = `https://explorer.solana.com/address/${encodeURIComponent(data.wallet)}?cluster=devnet`;
  walletLink.hidden = false;
  document.querySelector("#programs").replaceChildren(...data.programs.map(program => {
    const panel = node("div", undefined, "program-panel");
    const heading = node("div", undefined, "program-heading");
    heading.append(node("h3", program.name), explorer("address", program.address, short(program.address, 5)));
    const table = node("table");
    const head = node("thead");
    const row = node("tr");
    for (const [index, text] of ["Результат", "Подпись", "Слот"].entries()) {
      const cell = node("th", text, index === 2 ? "align-right" : undefined);
      cell.scope = "col";
      row.append(cell);
    }
    head.append(row);
    const body = node("tbody");
    renderTransactions(body, program.transactions, program.error);
    table.append(head, body);
    panel.append(heading, table);
    return panel;
  }));
  status.className = data.errors.length ? "status error" : "status";
  status.textContent = data.errors.length
    ? `Часть данных недоступна: ${data.errors.map(error => ({balance: "баланс", tokens: "токены", transactions: "транзакции", programs: "активность программ"})[error.section] || "RPC").join(", ")}. Обновите данные.`
    : "Данные получены из RPC · confirmed";
  if (data.balance === null && errorFor("balance")) status.textContent += " Баланс недоступен.";
}

function clearDashboard(message) {
  for (const id of ["sol-balance", "slot", "token-count", "transaction-count"]) document.getElementById(id).textContent = "—";
  emptyRow(document.querySelector("#tokens"), 4, message);
  emptyRow(document.querySelector("#transactions"), 3, message);
  document.querySelector("#programs").replaceChildren(node("p", message, "empty"));
  document.querySelector("#wallet-explorer").hidden = true;
}

async function fetchJson(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 65000);
  try {
    const response = await fetch(url, { signal: controller.signal, cache: "no-store" });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || "Не удалось получить данные.");
    return data;
  } finally {
    clearTimeout(timeout);
  }
}

async function refresh() {
  if (requestInFlight) return;
  const wallet = walletInput.value.trim();
  if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(wallet)) {
    walletInput.setCustomValidity("Введите публичный адрес Solana в формате base58.");
    walletInput.reportValidity();
    return;
  }
  walletInput.setCustomValidity("");
  requestInFlight = true;
  refreshButton.disabled = true;
  walletInput.disabled = true;
  overview.setAttribute("aria-busy", "true");
  clearDashboard("Загрузка из RPC…");
  status.className = "status";
  status.textContent = "Обновляю баланс, токены и транзакции…";
  try {
    renderDashboard(await fetchJson(`/api/dashboard?wallet=${encodeURIComponent(wallet)}`));
  } catch (error) {
    clearDashboard("Данные недоступны. Повторите обновление.");
    status.className = "status error";
    status.textContent = error.name === "AbortError" ? "RPC не ответил вовремя. Повторите обновление." : error.message;
  } finally {
    requestInFlight = false;
    refreshButton.disabled = false;
    walletInput.disabled = false;
    overview.setAttribute("aria-busy", "false");
  }
}

form.addEventListener("submit", event => { event.preventDefault(); refresh(); });
walletInput.addEventListener("input", () => walletInput.setCustomValidity(""));

async function initialize() {
  refreshButton.disabled = true;
  try {
    const config = await fetchJson("/api/config");
    walletInput.value = config.wallet;
    document.querySelector("#rpc-label").textContent = config.rpc_url;
    await refresh();
  } catch (error) {
    status.className = "status error";
    status.textContent = error.message;
    clearDashboard("Не удалось подключиться к dashboard API.");
    overview.setAttribute("aria-busy", "false");
  } finally {
    refreshButton.disabled = false;
  }
}

initialize();
