import http from "k6/http";
import { uuidv4 } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";
import { check, sleep } from "k6";
import { global_options } from "./settings.js";
export let options = global_options;

export default function () {
  const data = {
    merchant_id: "m1100",
    merchant_customer_id: "c11",
    card: {
      card_number: uuidv4(),
      name_on_card: "Max Payne",
    },
  };
  const response = http.post(
    "http://locker_server:8080/data/add",
    JSON.stringify(data),
    {
      headers: { "Content-Type": "application/json" },
    },
  );

  check(response, { "status is 200": (r) => r.status === 200 });
  // check(response, { "returned status OK": (r) => r.json().status === "Ok" });
}
