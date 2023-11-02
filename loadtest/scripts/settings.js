// export const global_options = {
//   stages: [
//     // Ramp-up from 1 to 5 VUs in 5s
//     { duration: "5s", target: 5 },
//     // Stay at rest on 5 VUs for 10s
//     { duration: "10s", target: 5 },
//     // Ramp-down from 5 to 0 VUs for 5s
//     { duration: "5s", target: 0 },
//   ],
// };

// export const global_options = {
//   stages: step_increment("30s", 100, 10),
// };

export const global_options = {
  // stages: [
  //   // { duration: "1m", target: 3 },
  //   { duration: "5m", target: 3 },
  // ],
  scenarios: {
    constant_request_rate: {
      executor: "constant-arrival-rate",
      rate: 100000,
      timeUnit: "1s", // 1000 iterations per second, i.e. 1000 RPS
      duration: "5m",
      preAllocatedVUs: 3, // how large the initial pool of VUs would be
      maxVUs: 5, // if the preAllocatedVUs are not enough, we can initialize more
    },
  },
};

// function step_increment(duration, step_size, count) {
//   let stages = [];
//   let current = 0;
//   for (let i = 0; i < count; ++i) {
//     current = current + step_size;
//     stages.push({ duration: duration, target: current });
//   }
//   return stages;
// }
