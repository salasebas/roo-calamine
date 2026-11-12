# Runtime measurements

Five fresh processes per backend and case, all on baseline 21f9da6 and the
rebased candidate. Time is median [minimum, maximum] in milliseconds;
memory is median peak RSS in MiB. All 17 oracle hashes match plain roo for
baseline, candidate, and control. No samples or outliers were removed.

| Fixture and call | Baseline ms / MiB | Fork ms / MiB | No-fork control ms / MiB |
|---|---:|---:|---:|
| small.xlsx / open | 0.717 [0.708, 0.94] / 35.7 | 0.783 [0.718, 1.679] / 35.6 | 0.833 [0.72, 0.9] / 35.9 |
| small.xlsx / cell | 8.087 [7.979, 8.292] / 37.2 | 7.921 [7.9, 8.017] / 37.1 | 8.056 [7.964, 9.619] / 37.1 |
| small.xlsx / celltype | 9.961 [9.524, 10.123] / 37.2 | 9.698 [9.579, 9.971] / 37.1 | 9.795 [9.626, 9.808] / 37.2 |
| small.xlsx / stream | 1.916 [1.86, 1.936] / 36.3 | 1.819 [1.754, 1.855] / 36.4 | 1.867 [1.787, 2.161] / 36.2 |
| small.xlsx / stream-max-rows | 2.213 [2.063, 2.47] / 36.3 | 2.078 [2.036, 2.491] / 36.4 | 2.158 [2.133, 2.257] / 36.3 |
| small.xlsx / each | 1.668 [1.475, 1.932] / 36.3 | 1.693 [1.433, 1.893] / 36.5 | 1.832 [1.66, 3.691] / 36.5 |
| small.xlsx / csv | 1.963 [1.812, 3.374] / 36.5 | 1.923 [1.578, 1.961] / 36.3 | 1.812 [1.773, 1.993] / 36.3 |
| mixed-100k.xlsx / cell | 169.976 [168.513, 171.483] / 103.2 | 176.844 [175.617, 193.194] / 103.3 | 170.52 [169.646, 176.77] / 103.3 |
| mixed-100k.xlsx / celltype | 170.232 [169.71, 173.689] / 103.4 | 180.542 [178.477, 189.373] / 103.3 | 173.266 [171.237, 183.03] / 103.3 |
| mixed-100k.xlsx / stream | 555.864 [535.231, 583.92] / 46.1 | 545.736 [542.497, 559.545] / 46.1 | 548.178 [539.471, 566.69] / 46.0 |
| mixed-100k.xlsx / stream-max-rows | 12.459 [11.727, 13.041] / 45.5 | 11.551 [11.316, 11.808] / 45.5 | 11.531 [11.453, 13.331] / 45.4 |
| mixed-100k.xlsx / each | 314.431 [307.752, 325.73] / 175.2 | 317.933 [315.885, 343.959] / 175.2 | 314.682 [310.155, 329.705] / 175.1 |
| mixed-100k.xlsx / csv | 446.912 [445.746, 475.198] / 116.1 | 456.42 [451.867, 469.414] / 116.1 | 449.995 [444.512, 455.03] / 116.1 |
| shared-strings-100k.xlsx / cell | 94.962 [93.95, 95.332] / 68.6 | 96.838 [95.918, 100.56] / 68.4 | 94.13 [92.647, 94.451] / 68.5 |
| shared-strings-100k.xlsx / stream | 237.007 [236.751, 238.324] / 46.5 | 236.753 [234.383, 248.237] / 46.5 | 238.867 [237.751, 240.332] / 46.6 |
| unique-strings-100k.xlsx / cell | 299.146 [297.795, 302.283] / 158.0 | 300.734 [297.696, 307.721] / 158.0 | 320.343 [318.003, 320.376] / 157.7 |
| unique-strings-100k.xlsx / stream | 463.018 [461.76, 478.502] / 248.4 | 459.571 [449.45, 469.355] / 248.4 | 487.916 [479.071, 498.078] / 249.0 |

[Raw samples and oracle hashes](paired.json). Control timings are experimental;
its code reduction is not a claim that it is ready for adoption.

## Pre-generated random queries

Seven fresh processes per backend, identical 10,000 row/column pairs generated
before timing. Values in milliseconds, median [minimum, maximum].

| Path | Open and first read / full array load | First 10k reads | Repeated 10k reads |
|---|---:|---:|---:|
| baseline | 1.691 [1.455, 5.925] | 4.004 [3.853, 4.31] | 3.428 [3.349, 3.75] |
| candidate | 1.454 [1.362, 1.846] | 3.975 [3.874, 4.11] | 3.351 [3.312, 3.91] |
| control | 1.852 [1.349, 3.977] | 4.046 [3.879, 4.08] | 3.362 [3.319, 3.634] |
| calx | 0.743 [0.719, 6.024] | 0.329 [0.312, 0.489] | 0.304 [0.3, 0.321] |

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
| Open and raw records | 1.478 | 1.476 |
| First 10k native cell reads and construction | 1.599 | 1.617 |
| Read values from returned cells | 0.233 | 0.235 |
| Warm native cell/value reads | 1.197 | 1.188 |
| Warm public cell/value reads | 3.352 | 3.323 |

[Raw component samples](components.json). Public dispatch and cell-construction
costs remain after the parser moves into Calamine.
