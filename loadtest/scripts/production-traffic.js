import http from "k6/http";
import { check, sleep } from "k6";
import { uuidv4, randomIntBetween } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";
import { randomItem } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";

// -----------------------------------------------------------------------------
// Configuration
// -----------------------------------------------------------------------------

const env = (key, defaultValue) => {
  const value = __ENV[key];
  return value !== undefined && value !== "" ? value : defaultValue;
};

const envBool = (key, defaultValue) => {
  const value = __ENV[key];
  if (value === undefined || value === "") return defaultValue;
  return ["true", "1", "yes"].includes(value.toLowerCase());
};

const envInt = (key, defaultValue) => {
  const value = parseInt(__ENV[key], 10);
  return isNaN(value) ? defaultValue : value;
};

const BASE_URL = env("BASE_URL", "https://127.0.0.1:3001");
const TENANT_ID = env("TENANT_ID", "public");
const LEGACY_PATH_PREFIX = env("LEGACY_PATH_PREFIX", "/cards");
const INSECURE_SKIP_TLS_VERIFY = envBool("INSECURE_SKIP_TLS_VERIFY", true);
const RUN_FOREVER = envBool("RUN_FOREVER", false);
const DURATION = env("DURATION", "5m");
const VUS = envInt("VUS", 5);
const ENABLE_NEGATIVE = envBool("ENABLE_NEGATIVE", true);
const ENABLE_CUSTODIAN = envBool("ENABLE_CUSTODIAN", false);
const KEY1 = env("KEY1", "");
const KEY2 = env("KEY2", "");

const DELAYED_RETRIEVE_ENABLED = envBool("DELAYED_RETRIEVE_ENABLED", true);
const DELAYED_RETRIEVE_DELAY = env("DELAYED_RETRIEVE_DELAY", "30s");
const DELAYED_RETRIEVE_TTL = envInt("DELAYED_RETRIEVE_TTL", 3600);

const WEIGHT_HEALTH = envInt("WEIGHT_HEALTH", 5);
const WEIGHT_HS_PAYMENT_METHOD_DELETE = envInt("WEIGHT_HS_PAYMENT_METHOD_DELETE", 20);
const WEIGHT_HS_CUSTOMER_DELETE_MANY_CARDS = envInt("WEIGHT_HS_CUSTOMER_DELETE_MANY_CARDS", 10);
const WEIGHT_HS_UPDATE_RETRIEVE_DELETE_ADD_SAME_REF = envInt("WEIGHT_HS_UPDATE_RETRIEVE_DELETE_ADD_SAME_REF", 15);
const WEIGHT_HS_METADATA_CHANGED_DELETE_ADD_SAME_REF = envInt("WEIGHT_HS_METADATA_CHANGED_DELETE_ADD_SAME_REF", 10);
const WEIGHT_HS_TOKENIZATION_DELETE_ADD_SAME_REF = envInt("WEIGHT_HS_TOKENIZATION_DELETE_ADD_SAME_REF", 10);
const WEIGHT_HS_WEBHOOK_RETRIEVE_DELETE_ADD_NEW_REF = envInt("WEIGHT_HS_WEBHOOK_RETRIEVE_DELETE_ADD_NEW_REF", 5);
const WEIGHT_HS_NETWORK_TOKEN_DELETE_ONLY = envInt("WEIGHT_HS_NETWORK_TOKEN_DELETE_ONLY", 5);
const WEIGHT_HS_PAYOUT_METADATA_UPDATE = envInt("WEIGHT_HS_PAYOUT_METADATA_UPDATE", 5);
const WEIGHT_LEGACY_FLOW = envInt("WEIGHT_LEGACY_FLOW", 25);
const WEIGHT_LEGACY_DUPLICATE = envInt("WEIGHT_LEGACY_DUPLICATE", 10);
const WEIGHT_LEGACY_METADATA_CHANGED = envInt("WEIGHT_LEGACY_METADATA_CHANGED", 10);
const WEIGHT_DELAYED_RETRIEVE = envInt("WEIGHT_DELAYED_RETRIEVE", 10);
const WEIGHT_V2_FLOW = envInt("WEIGHT_V2_FLOW", 20);
const WEIGHT_V2_UPDATE = envInt("WEIGHT_V2_UPDATE", 10);
const WEIGHT_FINGERPRINT_CREATE = envInt("WEIGHT_FINGERPRINT_CREATE", 10);
const WEIGHT_FINGERPRINT_REUSE = envInt("WEIGHT_FINGERPRINT_REUSE", 10);
const WEIGHT_FINGERPRINT_SUPPLIED_ID = envInt("WEIGHT_FINGERPRINT_SUPPLIED_ID", 5);
const WEIGHT_NEGATIVE = envInt("WEIGHT_NEGATIVE", 5);
const WEIGHT_CUSTODIAN = envInt("WEIGHT_CUSTODIAN", ENABLE_CUSTODIAN ? 2 : 0);

const SCENARIO_WEIGHTS = [
  { name: "health", weight: WEIGHT_HEALTH, fn: healthScenario },
  { name: "hs_payment_method_delete", weight: WEIGHT_HS_PAYMENT_METHOD_DELETE, fn: hsPaymentMethodDeleteScenario },
  { name: "hs_customer_delete_many_cards", weight: WEIGHT_HS_CUSTOMER_DELETE_MANY_CARDS, fn: hsCustomerDeleteManyCardsScenario },
  { name: "hs_update_retrieve_delete_add_same_ref", weight: WEIGHT_HS_UPDATE_RETRIEVE_DELETE_ADD_SAME_REF, fn: hsUpdateRetrieveDeleteAddSameRefScenario },
  { name: "hs_metadata_changed_delete_add_same_ref", weight: WEIGHT_HS_METADATA_CHANGED_DELETE_ADD_SAME_REF, fn: hsMetadataChangedDeleteAddSameRefScenario },
  { name: "hs_tokenization_delete_add_same_ref", weight: WEIGHT_HS_TOKENIZATION_DELETE_ADD_SAME_REF, fn: hsTokenizationDeleteAddSameRefScenario },
  { name: "hs_webhook_retrieve_delete_add_new_ref", weight: WEIGHT_HS_WEBHOOK_RETRIEVE_DELETE_ADD_NEW_REF, fn: hsWebhookRetrieveDeleteAddNewRefScenario },
  { name: "hs_network_token_delete_only", weight: WEIGHT_HS_NETWORK_TOKEN_DELETE_ONLY, fn: hsNetworkTokenDeleteOnlyScenario },
  { name: "hs_payout_metadata_update", weight: WEIGHT_HS_PAYOUT_METADATA_UPDATE, fn: hsPayoutMetadataUpdateScenario },
  { name: "legacy_flow", weight: WEIGHT_LEGACY_FLOW, fn: legacyFlowScenario },
  { name: "legacy_duplicate", weight: WEIGHT_LEGACY_DUPLICATE, fn: legacyDuplicateScenario },
  { name: "legacy_metadata_changed", weight: WEIGHT_LEGACY_METADATA_CHANGED, fn: legacyMetadataChangedScenario },
  { name: "delayed_retrieve", weight: DELAYED_RETRIEVE_ENABLED ? WEIGHT_DELAYED_RETRIEVE : 0, fn: delayedRetrieveScenario },
  { name: "v2_flow", weight: WEIGHT_V2_FLOW, fn: v2FlowScenario },
  { name: "v2_update", weight: WEIGHT_V2_UPDATE, fn: v2UpdateScenario },
  { name: "fingerprint_create", weight: WEIGHT_FINGERPRINT_CREATE, fn: fingerprintCreateScenario },
  { name: "fingerprint_reuse", weight: WEIGHT_FINGERPRINT_REUSE, fn: fingerprintReuseScenario },
  { name: "fingerprint_supplied_id", weight: WEIGHT_FINGERPRINT_SUPPLIED_ID, fn: fingerprintSuppliedIdScenario },
  { name: "negative", weight: ENABLE_NEGATIVE ? WEIGHT_NEGATIVE : 0, fn: negativeScenario },
  { name: "custodian", weight: WEIGHT_CUSTODIAN, fn: custodianScenario },
];

const TOTAL_WEIGHT = SCENARIO_WEIGHTS.reduce((sum, s) => sum + s.weight, 0);

// Parse durations like "30s", "2m", "1h" to seconds for k6 sleep
const parseDurationToSeconds = (duration) => {
  const match = duration.match(/^([0-9]+)([smh])$/);
  if (!match) return 30;
  const value = parseInt(match[1], 10);
  const unit = match[2];
  switch (unit) {
    case "s": return value;
    case "m": return value * 60;
    case "h": return value * 3600;
    default: return value;
  }
};

const DELAYED_RETRIEVE_DELAY_SECONDS = parseDurationToSeconds(DELAYED_RETRIEVE_DELAY);

// -----------------------------------------------------------------------------
// k6 Options
// -----------------------------------------------------------------------------

export const options = {
  insecureSkipTLSVerify: INSECURE_SKIP_TLS_VERIFY,
  scenarios: {
    production_traffic: {
      executor: "constant-vus",
      vus: VUS,
      duration: RUN_FOREVER ? "1000000h" : DURATION,
      gracefulStop: `${Math.max(DELAYED_RETRIEVE_DELAY_SECONDS + 5, 10)}s`,
    },
  },
  thresholds: {
    // Negative scenarios intentionally exercise expected 4xx responses (expired TTL,
    // invalid card, missing tenant header). Therefore the global HTTP failure
    // rate is not a reliable signal. Track checks pass-rate and latency instead.
    checks: ["rate>0.95"],
    http_req_duration: ["p(95)<2000"],
  },
  tags: {
    test: "production-traffic",
  },
};

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

function headers(extra = {}) {
  return Object.assign(
    {
      "Content-Type": "application/json",
      "x-tenant-id": TENANT_ID,
    },
    extra
  );
}

function fullUrl(path) {
  return `${BASE_URL}${path}`;
}

function legacyUrl(path) {
  const prefix = LEGACY_PATH_PREFIX.endsWith("/")
    ? LEGACY_PATH_PREFIX.slice(0, -1)
    : LEGACY_PATH_PREFIX;
  return fullUrl(`${prefix}${path}`);
}

function logUnexpectedFailure(details) {
  const entry = Object.assign(
    {
      timestamp: new Date().toISOString(),
      base_url: BASE_URL,
      tenant_id: TENANT_ID,
      vu: __VU,
      iteration: __ITER,
    },
    details
  );
  console.error(JSON.stringify(entry));
}

function safeResponseBody(response) {
  try {
    if (response.body === null || response.body === undefined) return null;
    const text = typeof response.body === "string" ? response.body : response.body.toString();
    return text.length > 4096 ? text.substring(0, 4096) + "...[truncated]" : text;
  } catch (e) {
    return "<unable-to-read-body>";
  }
}

function recordFailure(scenario, method, url, status, expectedStatuses, requestBody, response) {
  logUnexpectedFailure({
    scenario,
    method,
    url,
    status,
    expected_statuses: expectedStatuses,
    request_body: requestBody,
    response_body: safeResponseBody(response),
    response_time_ms: response.timings.duration,
    error: "unexpected_status",
  });
}

function doPost(scenario, url, body, expectedStatuses, extraHeaders = {}) {
  const bodyText = typeof body === "string" ? body : JSON.stringify(body);
  const response = http.post(url, bodyText, {
    headers: headers(extraHeaders),
    tags: { scenario },
  });

  const ok = check(response, {
    [`${scenario}: status in [${expectedStatuses.join(",")}]`]: (r) =>
      expectedStatuses.includes(r.status),
  });

  if (!ok) {
    recordFailure(scenario, "POST", url, response.status, expectedStatuses, bodyText, response);
  }

  return response;
}

function assertThat(scenario, label, condition, details = {}) {
  const ok = check(null, { [label]: () => condition });
  if (!ok) {
    logUnexpectedFailure(
      Object.assign({ scenario, label, error: "assertion_failed" }, details)
    );
  }
  return ok;
}

function doGet(scenario, url, expectedStatuses, extraHeaders = {}) {
  const response = http.get(url, {
    headers: headers(extraHeaders),
    tags: { scenario },
  });

  const ok = check(response, {
    [`${scenario}: status in [${expectedStatuses.join(",")}]`]: (r) =>
      expectedStatuses.includes(r.status),
  });

  if (!ok) {
    recordFailure(scenario, "GET", url, response.status, expectedStatuses, null, response);
  }

  return response;
}

function tryParseJson(response) {
  try {
    return response.json();
  } catch (e) {
    return null;
  }
}

const CARD_FIXTURES = [
    { number: "4242424242424242", month: "12", year: "30" },
    { number: "4000056655665556", month: "11", year: "28" },
    { number: "5105105105105100", month: "10", year: "29" },
    { number: "6011111111111117", month: "09", year: "31" },
    { number: "378282246310005", month: "03", year: "29" },
    { number: "371449635398431", month: "04", year: "30" },
    { number: "30569309025904", month: "05", year: "28" },
    { number: "38520000023237", month: "06", year: "29" },
    { number: "6011000990139424", month: "07", year: "31" },
    { number: "3566002020360505", month: "08", year: "30" },
    { number: "5200828282828210", month: "01", year: "28" },
    { number: "5555555555554444", month: "02", year: "29" },
    { number: "4111111111111111", month: "12", year: "30" },
];

function pickCard() {
  return Object.assign({}, randomItem(CARD_FIXTURES));
}

function cardByIndex(index, name, overrides = {}) {
  return Object.assign({}, CARD_FIXTURES[index % CARD_FIXTURES.length], { name }, overrides);
}

function generateEntityPair() {
  return {
    merchantId: `merchant-${uuidv4()}`,
    customerId: `customer-${uuidv4()}`,
  };
}

function pickScenario() {
  if (TOTAL_WEIGHT === 0) return SCENARIO_WEIGHTS[0];
  let roll = Math.random() * TOTAL_WEIGHT;
  for (const scenario of SCENARIO_WEIGHTS) {
    if (scenario.weight <= 0) continue;
    roll -= scenario.weight;
    if (roll <= 0) return scenario;
  }
  return SCENARIO_WEIGHTS[SCENARIO_WEIGHTS.length - 1];
}

// -----------------------------------------------------------------------------
// Scenario implementations
// -----------------------------------------------------------------------------

function healthScenario() {
  doGet("health", fullUrl("/health"), [200]);
  doGet("health_diagnostics", fullUrl("/health/diagnostics"), [200]);
}

function legacyAdd(merchantId, customerId, card, ttl, scenario, requestorCardReference = null) {
  const payload = {
    merchant_id: merchantId,
    merchant_customer_id: customerId,
    requestor_card_reference: requestorCardReference,
    card: {
      card_number: card.number,
      name_on_card: card.name,
      card_exp_month: card.month,
      card_exp_year: card.year,
      card_brand: card.brand,
      nick_name: card.nickName,
    },
    ttl: ttl,
  };
  return doPost(scenario, legacyUrl("/add"), payload, [200]);
}

function legacyRetrieve(merchantId, customerId, cardReference, scenario) {
  return doPost(
    scenario,
    legacyUrl("/retrieve"),
    {
      merchant_id: merchantId,
      merchant_customer_id: customerId,
      card_reference: cardReference,
    },
    [200, 404]
  );
}

function retrieveAndAssertFound(merchantId, customerId, cardReference, scenario) {
  const response = legacyRetrieve(merchantId, customerId, cardReference, scenario);
  assertThat(
    scenario,
    `${scenario}: retrieve response is 200`,
    response.status === 200,
    { status: response.status, response_body: safeResponseBody(response) }
  );
  return response;
}

function legacyDelete(merchantId, customerId, cardReference, scenario) {
  return doPost(
    scenario,
    legacyUrl("/delete"),
    {
      merchant_id: merchantId,
      merchant_customer_id: customerId,
      card_reference: cardReference,
    },
    [200]
  );
}

function getLegacyCardReference(response) {
  const body = tryParseJson(response);
  return body && body.payload && body.payload.card_reference;
}

function getLegacyPayload(response) {
  const body = tryParseJson(response);
  return body && body.payload;
}

function assertLegacyDeleteOk(scenario, response) {
  const body = tryParseJson(response);
  assertThat(
    scenario,
    `${scenario}: delete response status is Ok`,
    body && body.status === "Ok",
    { status: response.status, response_body: safeResponseBody(response) }
  );
}

function deleteAndAssert(merchantId, customerId, cardReference, scenario) {
  const response = legacyDelete(merchantId, customerId, cardReference, scenario);
  assertLegacyDeleteOk(scenario, response);
  return response;
}

function addDeleteAddSameReferenceScenario(scenario, retrieveBeforeDelete = false) {
  const { merchantId, customerId } = generateEntityPair();
  const originalCard = Object.assign(pickCard(), { name: `${scenario} Original` });
  const updatedCard = Object.assign({}, originalCard, {
    name: `${scenario} Updated`,
    year: "35",
    nickName: "updated-card",
  });

  const addRes = legacyAdd(merchantId, customerId, originalCard, 3600, scenario);
  const cardReference = getLegacyCardReference(addRes);

  if (cardReference) {
    if (retrieveBeforeDelete) {
      retrieveAndAssertFound(merchantId, customerId, cardReference, scenario);
    }

    deleteAndAssert(merchantId, customerId, cardReference, scenario);

    const reAdd = legacyAdd(
      merchantId,
      customerId,
      updatedCard,
      3600,
      scenario,
      cardReference
    );
    const reAddRef = getLegacyCardReference(reAdd);
    assertThat(
      scenario,
      `${scenario}: re-add keeps requestor card reference`,
      reAddRef === cardReference,
      { original_ref: cardReference, readd_ref: reAddRef }
    );
  }
}

function hsPaymentMethodDeleteScenario() {
  const scenario = "hs_payment_method_delete";
  const { merchantId, customerId } = generateEntityPair();
  const card = Object.assign(pickCard(), { name: "PM Delete" });

  const addRes = legacyAdd(merchantId, customerId, card, 3600, scenario);
  const cardReference = getLegacyCardReference(addRes);

  if (cardReference) {
    deleteAndAssert(merchantId, customerId, cardReference, scenario);
  }
}

function hsCustomerDeleteManyCardsScenario() {
  const scenario = "hs_customer_delete_many_cards";
  const { merchantId, customerId } = generateEntityPair();
  const cardCount = randomIntBetween(2, 5);
  const cardReferences = [];

  for (let i = 0; i < cardCount; i += 1) {
    const card = cardByIndex(i, `Customer Delete ${i}`);
    const addRes = legacyAdd(merchantId, customerId, card, 3600, scenario);
    const cardReference = getLegacyCardReference(addRes);
    if (cardReference) {
      cardReferences.push(cardReference);
    }
  }

  for (const cardReference of cardReferences) {
    deleteAndAssert(merchantId, customerId, cardReference, scenario);
  }
}

function hsUpdateRetrieveDeleteAddSameRefScenario() {
  addDeleteAddSameReferenceScenario("hs_update_retrieve_delete_add_same_ref", true);
}

function hsMetadataChangedDeleteAddSameRefScenario() {
  const scenario = "hs_metadata_changed_delete_add_same_ref";
  const { merchantId, customerId } = generateEntityPair();
  const originalCard = Object.assign(pickCard(), { name: "Metadata Original" });
  const changedCard = Object.assign({}, originalCard, {
    name: "Metadata Changed",
    year: "35",
  });

  const first = legacyAdd(merchantId, customerId, originalCard, 3600, scenario);
  const firstRef = getLegacyCardReference(first);
  const second = legacyAdd(merchantId, customerId, changedCard, 3600, scenario);
  const secondPayload = getLegacyPayload(second);

  if (firstRef && secondPayload) {
    assertThat(
      scenario,
      `${scenario}: metadata changed returns same card_reference`,
      secondPayload.card_reference === firstRef,
      { first_ref: firstRef, second_ref: secondPayload.card_reference }
    );
    assertThat(
      scenario,
      `${scenario}: duplication_check is meta_data_changed`,
      secondPayload.duplication_check === "meta_data_changed",
      { payload: secondPayload }
    );

    deleteAndAssert(merchantId, customerId, firstRef, scenario);
    const reAdd = legacyAdd(merchantId, customerId, changedCard, 3600, scenario, firstRef);
    const reAddRef = getLegacyCardReference(reAdd);
    assertThat(
      scenario,
      `${scenario}: re-add keeps existing card_reference`,
      reAddRef === firstRef,
      { first_ref: firstRef, readd_ref: reAddRef }
    );
  }
}

function hsTokenizationDeleteAddSameRefScenario() {
  addDeleteAddSameReferenceScenario("hs_tokenization_delete_add_same_ref", false);
}

function hsWebhookRetrieveDeleteAddNewRefScenario() {
  const scenario = "hs_webhook_retrieve_delete_add_new_ref";
  const { merchantId, customerId } = generateEntityPair();
  const originalCard = Object.assign(pickCard(), {
    name: "Webhook Token Original",
    brand: "Visa",
  });
  const updatedCard = Object.assign({}, originalCard, {
    name: "Webhook Token Updated",
    month: "10",
    year: "35",
  });

  const addRes = legacyAdd(merchantId, customerId, originalCard, 3600, scenario);
  const cardReference = getLegacyCardReference(addRes);

  if (cardReference) {
    retrieveAndAssertFound(merchantId, customerId, cardReference, scenario);
    deleteAndAssert(merchantId, customerId, cardReference, scenario);
    legacyAdd(merchantId, customerId, updatedCard, 3600, scenario);
  }
}

function hsNetworkTokenDeleteOnlyScenario() {
  const scenario = "hs_network_token_delete_only";
  const { merchantId, customerId } = generateEntityPair();
  const card = Object.assign(pickCard(), { name: "Network Token" });

  const addRes = legacyAdd(merchantId, customerId, card, 3600, scenario);
  const cardReference = getLegacyCardReference(addRes);

  if (cardReference) {
    deleteAndAssert(merchantId, customerId, cardReference, scenario);
  }
}

function hsPayoutMetadataUpdateScenario() {
  addDeleteAddSameReferenceScenario("hs_payout_metadata_update", false);
}

function legacyFlowScenario() {
  const { merchantId, customerId } = generateEntityPair();
  const card = Object.assign(pickCard(), { name: "John Smith" });

  const addRes = legacyAdd(merchantId, customerId, card, 3600, "legacy_flow");
  const cardReference = getLegacyCardReference(addRes);

  if (cardReference) {
    retrieveAndAssertFound(merchantId, customerId, cardReference, "legacy_flow");
    deleteAndAssert(merchantId, customerId, cardReference, "legacy_flow");
  }
}

function legacyDuplicateScenario() {
  const { merchantId, customerId } = generateEntityPair();
  const card = Object.assign(pickCard(), { name: "Duplicate Customer" });

  const first = legacyAdd(merchantId, customerId, card, 3600, "legacy_duplicate");
  const firstBody = tryParseJson(first);
  const firstRef = firstBody && firstBody.payload && firstBody.payload.card_reference;

  const second = legacyAdd(merchantId, customerId, card, 3600, "legacy_duplicate");
  const secondBody = tryParseJson(second);
  const secondRef = secondBody && secondBody.payload && secondBody.payload.card_reference;

  if (firstRef && secondRef) {
    assertThat(
      "legacy_duplicate",
      "legacy_duplicate: same card_reference on duplicate",
      firstRef === secondRef,
      { first_ref: firstRef, second_ref: secondRef }
    );
    assertThat(
      "legacy_duplicate",
      "legacy_duplicate: duplication_check is duplicated",
      secondBody.payload.duplication_check === "duplicated",
      { payload: secondBody.payload }
    );
  }
}

function legacyMetadataChangedScenario() {
  const { merchantId, customerId } = generateEntityPair();
  const base = pickCard();

  const firstCard = Object.assign({}, base, { name: "Original Name" });
  const first = legacyAdd(merchantId, customerId, firstCard, 3600, "legacy_metadata_changed");
  const firstBody = tryParseJson(first);
  const firstRef = firstBody && firstBody.payload && firstBody.payload.card_reference;

  const secondCard = Object.assign({}, base, { name: "Changed Name", year: "35" });
  const second = legacyAdd(merchantId, customerId, secondCard, 3600, "legacy_metadata_changed");
  const secondBody = tryParseJson(second);

  if (firstRef && secondBody && secondBody.payload) {
    // The service keeps the same card_reference when metadata changes and reports
    // `meta_data_changed` in the duplication_check field.
    assertThat(
      "legacy_metadata_changed",
      "legacy_metadata_changed: same card_reference on metadata change",
      secondBody.payload.card_reference === firstRef,
      { first_ref: firstRef, second_ref: secondBody.payload.card_reference }
    );
    assertThat(
      "legacy_metadata_changed",
      "legacy_metadata_changed: duplication_check is meta_data_changed",
      secondBody.payload.duplication_check === "meta_data_changed",
      { payload: secondBody.payload }
    );
  }
}

function delayedRetrieveScenario() {
  const { merchantId, customerId } = generateEntityPair();
  const card = Object.assign(pickCard(), { name: "Delayed Customer" });

  const addRes = legacyAdd(merchantId, customerId, card, DELAYED_RETRIEVE_TTL, "delayed_retrieve");
  const addBody = tryParseJson(addRes);
  const cardReference = addBody && addBody.payload && addBody.payload.card_reference;

  if (cardReference) {
    sleep(DELAYED_RETRIEVE_DELAY_SECONDS);
    const retrieveRes = legacyRetrieve(merchantId, customerId, cardReference, "delayed_retrieve");
    assertThat(
      "delayed_retrieve",
      "delayed_retrieve: card still retrievable after delay",
      retrieveRes.status === 200,
      { method: "POST", url: legacyUrl("/retrieve"), status: retrieveRes.status }
    );
    deleteAndAssert(merchantId, customerId, cardReference, "delayed_retrieve");
  }
}

function v2Add(entityId, vaultId, data, ttl, scenario, mode = "insert") {
  return doPost(
    scenario,
    fullUrl(`/api/v2/vault/add?mode=${mode}`),
    {
      entity_id: entityId,
      vault_id: vaultId,
      data: data,
      ttl: ttl,
    },
    [200]
  );
}

function v2Retrieve(entityId, vaultId, scenario) {
  return doPost(
    scenario,
    fullUrl("/api/v2/vault/retrieve"),
    { entity_id: entityId, vault_id: vaultId },
    [200, 404]
  );
}

function v2Delete(entityId, vaultId, scenario) {
  return doPost(
    scenario,
    fullUrl("/api/v2/vault/delete"),
    { entity_id: entityId, vault_id: vaultId },
    [200]
  );
}

function v2FlowScenario() {
  const entityId = `entity-${uuidv4()}`;
  const vaultId = `vault-${uuidv4()}`;
  const data = { version: 1, card_token: uuidv4() };

  v2Add(entityId, vaultId, data, 3600, "v2_flow", "insert");
  v2Retrieve(entityId, vaultId, "v2_flow");
  v2Delete(entityId, vaultId, "v2_flow");
}

function v2UpdateScenario() {
  const entityId = `entity-${uuidv4()}`;
  const vaultId = `vault-${uuidv4()}`;

  v2Add(entityId, vaultId, { version: 1 }, 3600, "v2_update", "insert");
  v2Add(entityId, vaultId, { version: 2 }, 3600, "v2_update", "upsert");
  const retrieveRes = v2Retrieve(entityId, vaultId, "v2_update");
  const body = tryParseJson(retrieveRes);
  if (body && body.data) {
    assertThat(
      "v2_update",
      "v2_update: upsert overwrote data to version 2",
      body.data.version === 2,
      { retrieved_data: body.data }
    );
  }
  v2Delete(entityId, vaultId, "v2_update");
}

function fingerprintCreate(data, key, scenario, extraHeaders = {}) {
  return doPost(
    scenario,
    legacyUrl("/fingerprint"),
    { data: data, key: key },
    [200],
    extraHeaders
  );
}

function fingerprintCreateScenario() {
  const data = `card-${uuidv4()}`;
  const key = `key-${uuidv4()}`;
  const res = fingerprintCreate(data, key, "fingerprint_create");
  const body = tryParseJson(res);
  if (body && body.fingerprint_id) {
    assertThat(
      "fingerprint_create",
      "fingerprint_create: fingerprint_id returned",
      typeof body.fingerprint_id === "string" && body.fingerprint_id.length > 0,
      { fingerprint_id: body.fingerprint_id }
    );
  }
}

function fingerprintReuseScenario() {
  const data = `card-${uuidv4()}`;
  const key = `key-${uuidv4()}`;

  const first = fingerprintCreate(data, key, "fingerprint_reuse");
  const firstBody = tryParseJson(first);
  const firstId = firstBody && firstBody.fingerprint_id;

  const second = fingerprintCreate(data, key, "fingerprint_reuse");
  const secondBody = tryParseJson(second);
  const secondId = secondBody && secondBody.fingerprint_id;

  if (firstId && secondId) {
    assertThat(
      "fingerprint_reuse",
      "fingerprint_reuse: same fingerprint_id for same data",
      firstId === secondId,
      { first_id: firstId, second_id: secondId }
    );
  }
}

function fingerprintSuppliedIdScenario() {
  const data = `card-${uuidv4()}`;
  const key = `key-${uuidv4()}`;
  const suppliedId = Array.from({ length: 20 }, () =>
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".charAt(
      Math.floor(Math.random() * 62)
    )
  ).join("");

  const res = fingerprintCreate(data, key, "fingerprint_supplied_id", {
    "x-fingerprint-id": suppliedId,
  });
  const body = tryParseJson(res);
  if (body && body.fingerprint_id) {
    assertThat(
      "fingerprint_supplied_id",
      "fingerprint_supplied_id: returned caller-supplied id",
      body.fingerprint_id === suppliedId,
      { supplied_id: suppliedId, returned_id: body.fingerprint_id }
    );
  }
}

function negativeScenario() {
  const scenario = "negative";
  const { merchantId, customerId } = generateEntityPair();

  // Invalid card number (fails Luhn validation)
  const invalidCard = {
    card_number: "1234567890123456",
    name_on_card: "Bad Card",
    card_exp_month: "12",
    card_exp_year: "30",
  };
  doPost(
    scenario,
    legacyUrl("/add"),
    {
      merchant_id: merchantId,
      merchant_customer_id: customerId,
      card: invalidCard,
      ttl: 3600,
    },
    [400]
  );

  // Missing tenant header -> 400
  const noTenantResponse = http.get(fullUrl("/health/diagnostics"), {
    headers: { "Content-Type": "application/json" },
    tags: { scenario },
  });
  assertThat(
    scenario,
    "negative: missing tenant header returns 400",
    noTenantResponse.status === 400,
    { method: "GET", url: fullUrl("/health/diagnostics"), status: noTenantResponse.status }
  );
  if (noTenantResponse.status !== 400) {
    recordFailure(
      scenario,
      "GET",
      fullUrl("/health/diagnostics"),
      noTenantResponse.status,
      [400],
      null,
      noTenantResponse
    );
  }

  // Expired TTL: add with ttl=1s, wait, retrieve expects 404
  const shortTtlCard = Object.assign(pickCard(), { name: "Short TTL" });
  const addRes = legacyAdd(merchantId, customerId, shortTtlCard, 1, scenario);
  const addBody = tryParseJson(addRes);
  const ref = addBody && addBody.payload && addBody.payload.card_reference;
  if (ref) {
    sleep(2);
    const retrieveRes = legacyRetrieve(merchantId, customerId, ref, scenario);
    assertThat(
      scenario,
      "negative: expired card retrieve returns 404",
      retrieveRes.status === 404,
      { method: "POST", url: legacyUrl("/retrieve"), status: retrieveRes.status }
    );
  }

  // Retrieve non-existent reference -> 404
  legacyRetrieve(merchantId, customerId, `missing-ref-${uuidv4()}`, scenario);
}

function custodianScenario() {
  if (!ENABLE_CUSTODIAN || !KEY1 || !KEY2) {
    return;
  }
  doPost("custodian_key1", fullUrl("/custodian/key1"), { key: KEY1 }, [200]);
  doPost("custodian_key2", fullUrl("/custodian/key2"), { key: KEY2 }, [200]);
  doPost("custodian_decrypt", fullUrl("/custodian/decrypt"), {}, [200]);
}

// -----------------------------------------------------------------------------
// Entry point
// -----------------------------------------------------------------------------

export default function () {
  const scenario = pickScenario();
  scenario.fn();

  // Small randomized think-time between iterations to mimic real users.
  sleep(randomIntBetween(50, 500) / 1000);
}

// -----------------------------------------------------------------------------
// Summary output
// -----------------------------------------------------------------------------

export function handleSummary(data) {
  // `data.root_group.checks` is an array of { passes, fails, ... } entries.
  const checkEntries = Array.isArray(data.root_group.checks) ? data.root_group.checks : [];
  const checksPassed = checkEntries.reduce((sum, c) => sum + (c.passes || 0), 0);
  const checksFailed = checkEntries.reduce((sum, c) => sum + (c.fails || 0), 0);
  const totalChecks = checksPassed + checksFailed;
  const checkPassRate = totalChecks > 0 ? (checksPassed / totalChecks) * 100 : 100;
  const httpReqDuration = data.metrics.http_req_duration
    ? data.metrics.http_req_duration.values
    : {};
  const httpReqFailedRate = data.metrics.http_req_failed
    ? data.metrics.http_req_failed.values.rate * 100
    : 0;

  const humanSummary = [
    "=== Production Traffic Test Summary ===",
    `Base URL:        ${BASE_URL}`,
    `Tenant ID:       ${TENANT_ID}`,
    `Legacy prefix:   ${LEGACY_PATH_PREFIX}`,
    `Duration:        ${RUN_FOREVER ? "until-interrupted" : DURATION}`,
    `VUs:             ${VUS}`,
    `HTTP requests:   ${data.metrics.http_reqs ? data.metrics.http_reqs.values.count : 0}`,
    `Checks passed:   ${checksPassed}`,
    `Checks failed:   ${checksFailed}`,
    `Check pass rate: ${checkPassRate.toFixed(2)}%`,
    `HTTP fail rate:  ${httpReqFailedRate.toFixed(2)}% (includes expected 4xx)`,
    `p95 latency:     ${httpReqDuration["p(95)"] ? httpReqDuration["p(95)"].toFixed(2) : "n/a"} ms`,
    `p99 latency:     ${httpReqDuration["p(99)"] ? httpReqDuration["p(99)"].toFixed(2) : "n/a"} ms`,
    "=======================================",
  ].join("\n");

  return {
    stdout: humanSummary + "\n",
    "./results/summary.json": JSON.stringify(
      {
        config: {
          base_url: BASE_URL,
          tenant_id: TENANT_ID,
          legacy_path_prefix: LEGACY_PATH_PREFIX,
          duration: RUN_FOREVER ? "until-interrupted" : DURATION,
          vus: VUS,
          enable_negative: ENABLE_NEGATIVE,
          enable_custodian: ENABLE_CUSTODIAN,
          delayed_retrieve_delay: DELAYED_RETRIEVE_DELAY,
          weights: SCENARIO_WEIGHTS.map((s) => ({ name: s.name, weight: s.weight })),
        },
        summary: {
          checks_passed: checksPassed,
          checks_failed: checksFailed,
          check_pass_rate_percent: checkPassRate,
          http_req_failed_rate_percent: httpReqFailedRate,
          http_req_duration_p95_ms: httpReqDuration["p(95)"] || null,
          http_req_duration_p99_ms: httpReqDuration["p(99)"] || null,
        },
      },
      null,
      2
    ),
  };
}
