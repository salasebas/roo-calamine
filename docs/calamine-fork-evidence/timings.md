
## Public API timings

Five fresh processes per backend and case. Times are median [minimum, maximum]
in milliseconds; memory is median peak RSS in MiB. All 17 digest checks
match plain roo for baseline, candidate, and control.

| Fixture and call | Baseline ms / MiB | Fork ms / MiB | No-fork control ms / MiB |
|---|---:|---:|---:|
| small.xlsx / open | 0.93 [0.87, 10.08] / 36.0 | 0.94 [0.86, 1.21] / 36.0 | 1.05 [0.82, 6.16] / 35.8 |
| small.xlsx / cell | 10.59 [9.3, 11.63] / 37.4 | 10.6 [9.82, 11.14] / 37.3 | 10.9 [9.48, 14.01] / 37.5 |
| small.xlsx / celltype | 13.57 [12.69, 13.86] / 37.1 | 13.22 [12.21, 13.87] / 37.5 | 13.31 [12.46, 16.52] / 37.5 |
| small.xlsx / stream | 2.7 [2.58, 3.19] / 36.6 | 2.35 [2.3, 2.79] / 36.9 | 2.93 [2.52, 3.51] / 36.6 |
| small.xlsx / stream-max-rows | 2.68 [2.32, 3.76] / 36.7 | 2.45 [2.15, 3.22] / 36.7 | 2.63 [2.46, 2.81] / 36.7 |
| small.xlsx / each | 2.26 [2.04, 2.69] / 36.7 | 2.34 [1.87, 2.52] / 36.6 | 2.3 [2.11, 2.44] / 36.6 |
| small.xlsx / csv | 2.6 [2.27, 2.77] / 36.6 | 2.42 [2.2, 2.48] / 36.8 | 2.41 [2.01, 24.24] / 36.5 |
| mixed-100k.xlsx / cell | 200.24 [190.76, 204.45] / 103.9 | 199.64 [193.92, 214.64] / 103.5 | 195.95 [188.82, 198.47] / 103.6 |
| mixed-100k.xlsx / celltype | 193.17 [192.52, 199.33] / 103.5 | 201.35 [199.75, 208.84] / 103.9 | 196.53 [191.94, 204.63] / 103.8 |
| mixed-100k.xlsx / stream | 629.0 [609.15, 655.59] / 46.5 | 627.5 [608.47, 651.98] / 46.4 | 620.73 [610.98, 815.5] / 46.4 |
| mixed-100k.xlsx / stream-max-rows | 13.73 [12.14, 15.85] / 45.6 | 13.19 [11.99, 14.23] / 45.6 | 13.12 [12.23, 14.28] / 45.5 |
| mixed-100k.xlsx / each | 325.99 [325.28, 331.17] / 175.1 | 327.23 [326.44, 338.65] / 175.2 | 325.35 [322.95, 338.81] / 175.3 |
| mixed-100k.xlsx / csv | 467.72 [462.65, 507.16] / 116.2 | 477.3 [469.76, 490.74] / 116.3 | 473.9 [465.52, 525.71] / 116.1 |
| shared-strings-100k.xlsx / cell | 96.78 [96.23, 97.21] / 68.3 | 100.16 [98.57, 101.76] / 68.5 | 96.74 [95.98, 98.27] / 68.3 |
| shared-strings-100k.xlsx / stream | 242.45 [239.28, 247.92] / 46.4 | 245.35 [239.66, 256.53] / 46.4 | 244.28 [241.62, 273.36] / 46.5 |
| unique-strings-100k.xlsx / cell | 362.48 [347.03, 371.72] / 158.8 | 338.18 [331.76, 350.17] / 159.8 | 379.77 [364.93, 400.11] / 159.8 |
| unique-strings-100k.xlsx / stream | 484.85 [474.11, 493.5] / 249.1 | 471.52 [468.75, 491.52] / 248.3 | 515.12 [504.28, 532.82] / 249.1 |

[Raw samples and oracle hashes](paired.json). The separate control is slower
on the unique-string cases in this run; its code reduction is not a claim
that it is ready for adoption.

## Pre-generated random queries

Seven fresh processes per backend, identical 10,000 row/column pairs generated
before timing. Values in milliseconds, median [minimum, maximum].

| Path | Open and first read / full array load | First 10k reads | Repeated 10k reads |
|---|---:|---:|---:|
| Baseline roo API | 1.495 [1.416, 97.336] | 4.040 [3.917, 4.492] | 3.460 [3.352, 3.692] |
| Fork roo API | 1.485 [1.397, 1.587] | 4.046 [3.938, 4.468] | 3.377 [3.354, 3.621] |
| No-fork control | 1.495 [1.454, 1.621] | 4.022 [3.919, 4.290] | 3.472 [3.319, 3.620] |
| Calx arrays | 0.731 [0.705, 2.705] | 0.320 [0.307, 0.336] | 0.309 [0.298, 0.317] |

Calx 0.1.0 loads the full small sheet and queries Ruby arrays. Its value rules
and APIs differ from roo, and it has no matching `celltype` API. This is a
reference cost, not an achievable floor for a roo-compatible implementation.
The 97 ms baseline outlier is retained in the raw data, not removed.
[Raw phases and sparse results](probes.json).

## Parse, construction, and dispatch

Seven fresh processes per backend on the same small sheet. Medians in ms.
The first phase opens the workbook, extracts metadata, and builds raw records
without constructing worksheet cells. Native cell reads bypass public coordinate
validation. The first 10k reads include constructing cells not yet requested;
repeated coordinates reuse the resulting objects. Reading `Cell#value` and
calling the public API are measured separately afterward. These phases are
sequential observations, not an additive subtraction of independently isolated
parser, allocator, or Ruby VM costs.

| Phase | Baseline | Fork |
|---|---:|---:|
| Open and raw records | 1.804 | 1.498 |
| First 10k native cell reads and construction | 1.725 | 1.650 |
| Read values from returned cells | 0.238 | 0.243 |
| Warm native cell/value reads | 1.248 | 1.237 |
| Warm public cell/value reads | 3.675 | 3.579 |

[Raw component samples](components.json). The public dispatch and cell-construction
costs remain after the parser moves into Calamine.
