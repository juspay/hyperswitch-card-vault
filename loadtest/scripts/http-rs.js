import http from "k6/http";
import { uuidv4 } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";
import { check, sleep } from "k6";
import { global_options } from "./settings.js";
export let options = global_options;

export default function () {
  // const data = {
  //   merchant_id: "m1100",
  //   merchant_customer_id: "c11",
  //   card: {
  //     card_number: uuidv4(),
  //     name_on_card: "Max Payne",
  //   },
  // };
  const data = {
    header:
      "eyJlbmMiOiJBMjU2R0NNIiwidHlwIjoiSldUIiwiYWxnIjoiUlNBLU9BRVAtMjU2In0",
    iv: "fRkYjluw9r5jAgwH",
    encryptedPayload:
      "uO3fkLywJSq98XmNzuN7E80hynUChbFjVHoT4J1k5qPA9vCuzp8dGw1Hbqx2Psr9D35dg3Q0E1iYlM_fGpM4v_1RjbLH-OEBXiPHm2Z6iMNOH-IJ7InftJmW25jcIo_XZElj__VENfOudup8ezXORnbwdk7UHHuEGT4GguQCLrbhz_QWJMzeTDmEZVIZ7p7Slbklh5UlcGqguUoIs7yQk_LPpmE2B6yHfV8wr7_T_vWFqp4vNcZT2zg74SQ62l8fLi-XK5u0IZyDz-4IDgvs6ua5s9ZDowRC11p0B_6pGaj-5IoEYyJYCxcbMA5TaDc5h-Xuljzmyw6c0-YbhUJBPDkfU3a6wD6WhNRytKWugwwW5-8nbIVazVuqFCVr8LeRUO4zF3l-7RxJjl0djIzBIMamqihT_C4tAkQRjQy2Ogv3_gv3awcLMXNgAEoC8a0f1osUsMrA_5oSz8R39BGSns2j4OAE85ySRJxYTwm2u-257-10C0ggeOAX_DHHCMSfq0_rUOKZBhoMJmakkECUDFbzCtg4940RIi0VWtZbW5nrZCkcFDgahvk7bSOZ4fqH-yl45MQTeJYusZPnBLm0CF8QVGhRv2zdqx3wvO02VdAsb8n80J5vQeqikvscVaEdQpSsfnBRLjW2ZyW9gX5yrvizdn3DohLVgqvPBuwtH7dEchPLaTbfIIDrnlfvmaiJMITOf9yv4kH2AYRgMV5ehvZK249VJGfHqITaOW3DJLkuj85-M5PwPb60P_CfndFrKP4mcuwXKgF5Tspnuw9sEZKfbR099SmkALWpyFrBXo-350zmPN3JaRaTx2AnuQlwhoW9I4DYooz67BDNOyDU6o4KEZ60_NQim7I-IpDABVjtSrbnBl3PNV3RsOkDmRQyoPUMB5nNC9OnSfZvHU9Q4_0SaWlSLFWMMRF2I54oWUk7dyzZl2gfb_gqe4Xf5qo7ddBramb9Nz82YnsCUyNVdoCaK4q3v3e2qvKQQ_LhPA",
    tag: "0_vJmBJUFJEgFpL7Ck7UAg",
    encryptedKey:
      "vnIziEpSfYwp__VhLLUHdqwa8BofOMvPLbHdIsOsn0i_e4H967oodlZhrFZFydXClzBA3DdOww-h85zcd1-GGfjzXEL8roDIR7rCtsGEgDMqj62QBFbfD1XqKPfuWtE3MFQi0TGoKJWmm2y_D5dwAxb3KPCWdJf_83ZL5xbeI18rgkH7q1wb6YL9XW8z7nWIhUjf71-HnA6mHmlbTKu8-YccIUKR9yZB6XV5ooteiPOXkYJtbe1CWSGHIbBXeFfkQ9EhnQR-a0CK2QS14DmRQEQGXn4eFPYU28iAI9weMNuYz6sa6hazeO635zrAdcV_YOnXCwgCrWPeqXu10tGRIg",
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
