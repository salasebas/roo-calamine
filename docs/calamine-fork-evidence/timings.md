# Runtime measurements

Five fresh processes per backend and case, all on baseline 21f9da6 and the
rebased candidate. Time is median [minimum, maximum] in milliseconds;
memory is median peak RSS in MiB. All 17 oracle hashes match plain roo for
baseline, candidate, and control. No samples or outliers were removed.

| Fixture and call | Baseline ms / MiB | Fork ms / MiB | No-fork control ms / MiB |
|---|---:|---:|---:|
| small.xlsx / open | 0.741 [0.671, 0.82] / 35.8 | 0.703 [0.604, 0.787] / 35.7 | 0.713 [0.646, 0.742] / 35.7 |
| small.xlsx / cell | 9.005 [8.922, 9.343] / 37.3 | 9.167 [8.981, 9.36] / 37.3 | 9.481 [8.784, 10.823] / 37.2 |
| small.xlsx / celltype | 10.9 [10.446, 11.038] / 37.1 | 10.477 [10.367, 10.987] / 37.2 | 10.648 [10.473, 10.998] / 37.1 |
| small.xlsx / stream | 2.096 [1.964, 2.434] / 36.4 | 2.317 [2.039, 2.678] / 36.4 | 2.458 [2.0, 3.444] / 36.3 |
| small.xlsx / stream-max-rows | 2.007 [1.891, 2.033] / 36.3 | 1.899 [1.804, 1.946] / 36.5 | 1.997 [1.867, 2.012] / 36.4 |
| small.xlsx / each | 1.708 [1.58, 1.829] / 36.5 | 1.767 [1.483, 2.252] / 36.4 | 1.748 [1.66, 1.947] / 36.5 |
| small.xlsx / csv | 2.064 [2.029, 2.458] / 36.4 | 2.099 [1.998, 3.116] / 36.6 | 1.965 [1.831, 2.078] / 36.3 |
| mixed-100k.xlsx / cell | 218.644 [210.753, 221.382] / 106.2 | 224.913 [214.731, 227.711] / 105.0 | 220.547 [217.892, 226.285] / 106.7 |
| mixed-100k.xlsx / celltype | 207.666 [206.938, 221.308] / 117.4 | 218.664 [210.31, 227.322] / 104.7 | 211.8 [209.429, 218.843] / 105.3 |
| mixed-100k.xlsx / stream | 702.22 [686.992, 708.477] / 47.4 | 693.359 [683.374, 721.852] / 47.3 | 694.679 [687.94, 712.578] / 47.0 |
| mixed-100k.xlsx / stream-max-rows | 15.619 [15.315, 17.454] / 46.4 | 15.633 [14.091, 16.738] / 46.2 | 16.626 [14.703, 27.016] / 46.0 |
| mixed-100k.xlsx / each | 324.936 [316.76, 330.469] / 175.3 | 332.11 [323.281, 340.228] / 175.1 | 323.47 [316.094, 332.241] / 175.3 |
| mixed-100k.xlsx / csv | 465.82 [464.22, 471.711] / 116.2 | 471.756 [467.213, 481.126] / 116.3 | 468.548 [465.25, 471.838] / 116.3 |
| shared-strings-100k.xlsx / cell | 100.674 [99.668, 101.597] / 68.6 | 104.011 [102.333, 108.757] / 68.7 | 101.75 [100.818, 109.273] / 70.5 |
| shared-strings-100k.xlsx / stream | 249.902 [246.531, 253.381] / 46.8 | 250.36 [248.99, 250.669] / 46.6 | 250.476 [249.579, 270.058] / 46.7 |
| unique-strings-100k.xlsx / cell | 309.776 [307.867, 311.577] / 157.9 | 311.581 [301.279, 322.673] / 158.1 | 337.768 [334.817, 339.932] / 157.9 |
| unique-strings-100k.xlsx / stream | 461.041 [452.741, 466.57] / 248.3 | 463.392 [449.798, 466.496] / 248.3 | 485.626 [481.65, 487.893] / 249.1 |

[Raw samples and oracle hashes](paired.json). Control timings are experimental;
its code reduction is not a claim that it is ready for adoption.

## Pre-generated random queries

Seven fresh processes per backend, identical 10,000 row/column pairs generated
before timing. Values in milliseconds, median [minimum, maximum].

| Path | Open and first read / full array load | First 10k reads | Repeated 10k reads |
|---|---:|---:|---:|
| baseline | 1.614 [1.448, 2.229] | 3.887 [3.852, 4.289] | 3.392 [3.301, 3.435] |
| candidate | 1.481 [1.308, 2.855] | 3.887 [3.868, 4.003] | 3.326 [3.307, 3.392] |
| control | 1.539 [1.348, 21.221] | 3.899 [3.881, 3.947] | 3.35 [3.309, 3.413] |
| calx | 0.728 [0.691, 5.386] | 0.309 [0.3, 0.329] | 0.3 [0.293, 0.326] |

Calx 0.1.0 loads the full small sheet and queries Ruby arrays. Its value rules
and APIs differ from roo, and it has no matching celltype API. This is a
reference cost, not an achievable floor for a roo-compatible implementation.
[Raw phases and sparse results](probes.json).

## Parse, construction, and dispatch

Seven fresh processes per backend on the same small sheet. Medians in ms.
The first phase opens the workbook, extracts metadata, and builds raw records
without constructing worksheet cells. Native cell reads bypass public coordinate
validation. The first 10k reads include constructing cells not yet requested;
repeated coordinates reuse the resulting objects. Reading Cell#value and
calling the public API are measured separately afterward. These phases are
sequential observations, not an additive subtraction of independently isolated
parser, allocator, or Ruby VM costs.

| Phase | Baseline | Fork |
|---|---:|---:|
| Open and raw records | 1.54 | 1.39 |
| First 10k native cell reads and construction | 1.606 | 1.57 |
| Read values from returned cells | 0.237 | 0.228 |
| Warm native cell/value reads | 1.181 | 1.171 |
| Warm public cell/value reads | 3.293 | 3.315 |

[Raw component samples](components.json). Public dispatch and cell-construction
costs remain after the parser moves into Calamine.
