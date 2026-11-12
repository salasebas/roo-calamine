# Runtime measurements

Five fresh processes per backend and case, all on baseline 194b149 and the
rebased candidate. Time is median [minimum, maximum] in milliseconds;
memory is median peak RSS in MiB. All 17 oracle hashes match plain roo for
baseline, candidate, and control. No samples or outliers were removed.

| Fixture and call | Baseline ms / MiB | Fork ms / MiB | No-fork control ms / MiB |
|---|---:|---:|---:|
| small.xlsx / open | 0.778 [0.695, 1.01] / 35.8 | 0.778 [0.708, 0.895] / 35.8 | 0.824 [0.729, 0.99] / 35.5 |
| small.xlsx / cell | 6.491 [6.395, 6.987] / 37.0 | 6.482 [6.305, 6.769] / 36.9 | 6.584 [6.528, 6.843] / 37.0 |
| small.xlsx / celltype | 6.23 [6.206, 6.381] / 36.8 | 6.3 [6.052, 6.58] / 36.9 | 6.188 [6.004, 6.644] / 37.0 |
| small.xlsx / stream | 1.802 [1.758, 2.16] / 36.4 | 1.754 [1.689, 2.301] / 36.3 | 1.805 [1.797, 1.899] / 36.4 |
| small.xlsx / stream-max-rows | 1.795 [1.755, 1.859] / 36.3 | 1.751 [1.747, 2.048] / 36.5 | 1.843 [1.77, 1.98] / 36.3 |
| small.xlsx / each | 1.634 [1.589, 1.69] / 36.4 | 1.549 [1.392, 3.929] / 36.3 | 1.573 [1.424, 1.619] / 36.2 |
| small.xlsx / csv | 1.666 [1.625, 1.844] / 36.4 | 1.75 [1.567, 1.817] / 36.3 | 1.771 [1.618, 1.778] / 36.2 |
| mixed-100k.xlsx / cell | 169.444 [164.938, 194.447] / 102.4 | 176.981 [172.735, 180.287] / 102.4 | 171.858 [168.98, 178.553] / 102.5 |
| mixed-100k.xlsx / celltype | 166.631 [160.778, 173.513] / 99.3 | 169.089 [166.061, 191.73] / 99.2 | 170.22 [161.726, 179.157] / 99.3 |
| mixed-100k.xlsx / stream | 547.167 [543.982, 560.765] / 46.2 | 551.046 [545.812, 555.234] / 46.2 | 550.724 [545.204, 562.326] / 46.2 |
| mixed-100k.xlsx / stream-max-rows | 11.398 [11.26, 12.777] / 45.5 | 11.287 [11.224, 11.797] / 45.5 | 12.692 [11.357, 51.166] / 45.4 |
| mixed-100k.xlsx / each | 323.309 [309.56, 393.506] / 175.3 | 329.531 [320.535, 408.859] / 175.2 | 320.757 [313.296, 411.4] / 175.2 |
| mixed-100k.xlsx / csv | 465.672 [463.252, 517.887] / 116.3 | 473.68 [466.657, 499.556] / 116.3 | 462.812 [459.154, 475.032] / 116.1 |
| shared-strings-100k.xlsx / cell | 93.817 [92.989, 96.363] / 67.9 | 96.69 [94.11, 99.253] / 67.9 | 92.988 [91.74, 97.153] / 67.8 |
| shared-strings-100k.xlsx / stream | 242.556 [241.083, 263.235] / 46.6 | 244.156 [239.934, 245.003] / 46.4 | 242.765 [239.91, 245.387] / 46.6 |
| unique-strings-100k.xlsx / cell | 331.532 [315.446, 343.209] / 158.3 | 318.256 [310.91, 331.931] / 157.1 | 348.5 [337.995, 380.026] / 157.6 |
| unique-strings-100k.xlsx / stream | 489.826 [471.213, 501.895] / 248.4 | 491.55 [454.182, 581.199] / 248.7 | 503.878 [493.306, 562.385] / 249.0 |

[Raw samples and oracle hashes](paired.json). Control timings are experimental;
its code reduction is not a claim that it is ready for adoption.

## Pre-generated random queries

Seven fresh processes per backend, identical 10,000 row/column pairs generated
before timing. Values in milliseconds, median [minimum, maximum].

| Path | Open and first read / full array load | First 10k reads | Repeated 10k reads |
|---|---:|---:|---:|
| baseline | 1.348 [1.2, 5.461] | 3.079 [3.005, 3.141] | 2.852 [2.8, 3.272] |
| candidate | 1.327 [1.261, 1.382] | 3.084 [2.939, 3.267] | 2.821 [2.801, 3.002] |
| control | 1.27 [1.225, 2.211] | 3.041 [2.998, 3.282] | 2.848 [2.795, 2.865] |
| calx | 0.664 [0.657, 3.151] | 0.297 [0.282, 0.305] | 0.292 [0.277, 0.299] |

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
| Open and raw records | 1.396 | 1.339 |
| First 10k native cell reads and construction | 1.446 | 1.449 |
| Read values from returned cells | 0.22 | 0.223 |
| Warm native cell/value reads | 1.144 | 1.139 |
| Warm public cell/value reads | 2.735 | 2.74 |

[Raw component samples](components.json). Public dispatch and cell-construction
costs remain after the parser moves into Calamine.
