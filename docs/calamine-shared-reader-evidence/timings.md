# Public API measurements

Five fresh processes per backend/case; median [minimum, maximum] in ms,
then median peak RSS in MiB. All 17 baseline/candidate/control digests match
plain Roo 4.0.0. No samples were discarded. The initial run is retained in [paired-initial.json](paired-initial.json).
The final paired pass followed compilation/tests. Control was measured in a separate
pass after the paired run. `cell` and `celltype` include open and 10,000 random
queries. [Environment](environment.json), [fixture hashes](fixtures.json),
[paired samples/oracles](paired.json), [control samples](control.json).

| Fixture / scenario | Main ms / MiB | Shared fork ms / MiB | No-fork control ms / MiB |
|---|---:|---:|---:|
| small.xlsx / open | 0.805 [0.742, 0.841] / 36.0 | 0.827 [0.755, 0.937] / 36.0 | 1.834 [0.767, 5.369] / 36.0 |
| small.xlsx / cell | 7.0 [6.772, 7.192] / 37.3 | 7.228 [7.104, 7.891] / 37.4 | 7.645 [7.515, 119.704] / 37.2 |
| small.xlsx / celltype | 6.643 [6.377, 6.991] / 37.1 | 6.965 [6.923, 7.022] / 37.3 | 7.069 [6.743, 73.443] / 37.2 |
| small.xlsx / stream | 2.001 [1.763, 2.142] / 36.4 | 2.409 [2.301, 2.549] / 36.5 | 2.536 [2.244, 30.436] / 36.5 |
| small.xlsx / stream-max-rows | 2.025 [1.913, 2.114] / 36.5 | 2.4 [2.133, 3.227] / 36.7 | 3.944 [2.963, 102.632] / 36.8 |
| small.xlsx / each | 1.756 [1.675, 1.853] / 36.3 | 2.018 [1.903, 2.08] / 36.5 | 2.718 [2.226, 3.275] / 36.8 |
| small.xlsx / csv | 1.871 [1.596, 1.983] / 36.4 | 2.281 [2.03, 2.305] / 36.6 | 2.599 [2.181, 13.092] / 36.8 |
| mixed-100k.xlsx / cell | 170.905 [170.185, 173.711] / 102.6 | 589.174 [587.858, 595.35] / 102.5 | 209.136 [185.77, 212.266] / 102.7 |
| mixed-100k.xlsx / celltype | 162.447 [161.566, 163.675] / 99.3 | 567.143 [559.783, 577.908] / 99.5 | 192.492 [177.73, 193.424] / 99.7 |
| mixed-100k.xlsx / stream | 540.314 [537.649, 558.934] / 46.1 | 563.604 [559.314, 567.817] / 43.0 | 711.669 [709.707, 736.75] / 46.9 |
| mixed-100k.xlsx / stream-max-rows | 11.326 [11.243, 11.546] / 45.7 | 14.587 [14.424, 15.246] / 42.9 | 14.539 [13.291, 50.096] / 46.2 |
| mixed-100k.xlsx / each | 300.353 [283.306, 301.867] / 173.1 | 703.51 [701.899, 712.423] / 173.2 | 357.973 [321.304, 1115.135] / 173.3 |
| mixed-100k.xlsx / csv | 458.145 [449.653, 473.598] / 116.5 | 888.17 [870.316, 951.114] / 116.6 | 552.853 [547.806, 565.906] / 117.9 |
| shared-strings-100k.xlsx / cell | 94.428 [90.515, 101.689] / 68.1 | 255.953 [253.929, 260.209] / 67.9 | 110.681 [105.917, 205.047] / 69.7 |
| shared-strings-100k.xlsx / stream | 244.202 [243.282, 251.682] / 46.6 | 250.84 [245.341, 252.762] / 45.3 | 289.187 [266.155, 302.386] / 47.6 |
| unique-strings-100k.xlsx / cell | 315.009 [305.213, 340.032] / 157.2 | 468.657 [442.785, 515.691] / 157.8 | 370.387 [335.866, 1100.067] / 157.6 |
| unique-strings-100k.xlsx / stream | 513.856 [495.58, 575.703] / 249.4 | 508.364 [463.533, 521.148] / 249.9 | 532.873 [489.552, 612.077] / 248.8 |

## Queries after open

Seven processes each. Identical 10,000 coordinates generated before timing.
Calx loads the full small sheet and reads arrays; it has different semantics
and no matching celltype API. It is a reference, not a compatible floor.

| Path | Open/first read or array load | First 10k reads | Repeated 10k reads |
|---|---:|---:|---:|
| baseline | 2.065 [1.433, 2.76] | 3.345 [3.176, 4.143] | 2.958 [2.89, 3.572] |
| candidate | 2.497 [1.751, 3.02] | 3.277 [3.193, 4.138] | 3.068 [2.889, 3.663] |
| calx | 0.719 [0.685, 21.54] | 0.314 [0.309, 0.348] | 0.304 [0.299, 0.358] |

[Warm and sparse samples](probes.json), [Calx samples](calx.json). Sparse
A1/XFD1048576 reads keep a 242-byte Ruby store wrapper and materialize two
cells. The wrapper size excludes native allocations.

## Construction and dispatch

Seven processes each on the small sheet; medians in ms. Coordinates are
generated first. Phases run sequentially, not as isolated additive costs.
The first native queries build cells not yet requested; repeated reads reuse
them. The last phase includes public coordinate validation/dispatch.

| Phase | Main | Shared fork |
|---|---:|---:|
| Open/metadata/raw records | 1.606 | 2.132 |
| First 10k native cell reads/construction | 1.626 | 1.684 |
| Read values from returned cells | 0.24 | 0.239 |
| Warm native cell/value reads | 1.181 | 1.197 |
| Warm public cell/value reads | 2.813 | 2.852 |

[Component samples](components.json). These do not isolate the XML parser,
allocator and Ruby VM from one another.
