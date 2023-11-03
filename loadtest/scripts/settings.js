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
  stages: [
    { duration: "1s", target: 3 },
    { duration: "5m", target: 3 },
  ],
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
