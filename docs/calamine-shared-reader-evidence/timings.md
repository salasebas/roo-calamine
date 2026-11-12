# Public API measurements

Five fresh processes per backend/case; median [minimum, maximum] in ms,
then median peak RSS in MiB. Baseline, candidate and no-fork control run
in one pass with their process positions rotated. All 17 digests match
plain Roo 4.0.0. This pass followed compilation and functional tests.
`cell` and `celltype` include open and 10,000 random queries.

[Environment](environment.json), [fixture hashes](fixtures.json),
[all final samples/oracles](paired.json). [control.json](control.json) is a
projection of those same final samples, not an additional experiment.
Earlier runs are retained without discarding samples:
[initial pair](paired-initial.json), [second pair](paired-two-backends.json),
and [initial control](control-initial.json). The initial control overlapped
compilation and had high variation; it is not used for the final comparison.

Reproduce the final pass with:

```sh
ruby bench/fork_comparison.rb BASELINE_LIB docs/calamine-shared-reader-evidence/paired.json 5 CONTROL_LIB
```

| Fixture / scenario | Main ms / MiB | Shared fork ms / MiB | No-fork control ms / MiB |
|---|---:|---:|---:|
| small.xlsx / open | 0.996 [0.751, 26.548] / 35.7 | 1.006 [0.861, 18.875] / 35.8 | 0.91 [0.758, 1.187] / 35.9 |
| small.xlsx / cell | 7.607 [7.045, 9.421] / 37.2 | 10.101 [7.693, 20.945] / 37.2 | 8.399 [7.056, 9.661] / 37.3 |
| small.xlsx / celltype | 7.907 [7.326, 9.947] / 37.2 | 8.343 [7.37, 9.784] / 37.3 | 7.054 [6.976, 9.234] / 37.2 |
| small.xlsx / stream | 2.655 [2.247, 2.9] / 36.6 | 3.531 [3.097, 17.326] / 37.0 | 2.982 [2.305, 3.162] / 36.4 |
| small.xlsx / stream-max-rows | 2.65 [2.433, 60.294] / 36.7 | 3.145 [2.758, 3.533] / 36.8 | 2.938 [2.65, 3.155] / 36.6 |
| small.xlsx / each | 2.528 [2.182, 70.368] / 36.5 | 2.655 [2.061, 3.009] / 36.5 | 2.196 [1.777, 37.364] / 36.4 |
| small.xlsx / csv | 2.166 [1.812, 2.665] / 36.4 | 2.201 [2.077, 2.952] / 36.5 | 2.0 [1.675, 2.667] / 36.5 |
| mixed-100k.xlsx / cell | 173.386 [173.038, 215.404] / 102.5 | 645.014 [598.277, 684.98] / 102.6 | 179.423 [173.122, 329.833] / 102.5 |
| mixed-100k.xlsx / celltype | 200.166 [168.171, 281.932] / 99.5 | 601.034 [585.798, 680.594] / 99.3 | 179.525 [166.882, 263.579] / 99.2 |
| mixed-100k.xlsx / stream | 630.787 [585.27, 650.202] / 46.4 | 616.784 [582.032, 699.291] / 44.8 | 616.056 [556.158, 630.082] / 46.4 |
| mixed-100k.xlsx / stream-max-rows | 14.78 [13.467, 15.443] / 45.7 | 17.65 [17.39, 19.027] / 43.7 | 15.387 [12.926, 148.247] / 45.8 |
| mixed-100k.xlsx / each | 355.286 [342.733, 428.076] / 175.2 | 830.61 [774.948, 860.006] / 176.7 | 366.07 [348.119, 386.833] / 175.7 |
| mixed-100k.xlsx / csv | 546.45 [542.066, 556.121] / 118.7 | 1016.944 [988.035, 1041.372] / 118.5 | 545.891 [529.566, 575.3] / 118.0 |
| shared-strings-100k.xlsx / cell | 101.764 [94.912, 121.504] / 68.0 | 280.392 [264.046, 328.835] / 68.6 | 104.783 [91.764, 119.512] / 68.3 |
| shared-strings-100k.xlsx / stream | 297.563 [266.828, 305.774] / 47.7 | 297.004 [282.653, 313.628] / 46.1 | 294.429 [274.144, 329.6] / 47.9 |
| unique-strings-100k.xlsx / cell | 385.146 [341.195, 497.835] / 160.0 | 519.197 [487.958, 615.532] / 159.2 | 387.335 [350.884, 474.868] / 158.8 |
| unique-strings-100k.xlsx / stream | 510.252 [481.104, 1471.807] / 249.4 | 482.21 [445.496, 543.298] / 248.5 | 537.338 [492.533, 590.248] / 249.1 |

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
