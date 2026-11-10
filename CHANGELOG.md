# Changelog

## [0.0.2](https://github.com/salasebas/roo-calamine/compare/v0.0.1...v0.0.2) (2026-11-10)


### Features

* read values 2x faster than streaming with an opt-in Roo::Calamine.each_row_values ([#12](https://github.com/salasebas/roo-calamine/issues/12)) ([c4d02de](https://github.com/salasebas/roo-calamine/commit/c4d02de37458641a9e24db0de0d3b5eef7da5927))


### Performance Improvements

* build cell coordinates in Rust for 3-13% faster reads ([#13](https://github.com/salasebas/roo-calamine/issues/13)) ([42a57f5](https://github.com/salasebas/roo-calamine/commit/42a57f5d044000fda04022e8de500d7fbb5deb53))
* build cell coordinates in Rust instead of Coordinate.new ([42a57f5](https://github.com/salasebas/roo-calamine/commit/42a57f5d044000fda04022e8de500d7fbb5deb53))
* build roo cells without running their Ruby constructors ([5f4561a](https://github.com/salasebas/roo-calamine/commit/5f4561a3821fc94f3c63c8eb1d3bfd273f7cffc7))
* make each without options up to 47% faster on large sheets ([#18](https://github.com/salasebas/roo-calamine/issues/18)) ([739f02c](https://github.com/salasebas/roo-calamine/commit/739f02c5fed5f46c074a91776ba62180a75bc0d3))
* make parse(headers: true) 2x faster by grouping the cells hash in one pass ([#14](https://github.com/salasebas/roo-calamine/issues/14)) ([1d15aae](https://github.com/salasebas/roo-calamine/commit/1d15aae5d0ef83413be288157f012e517c35e54f))
* make single-cell reads 2x faster with 3x less memory by building cells lazily ([#17](https://github.com/salasebas/roo-calamine/issues/17)) ([c2417fb](https://github.com/salasebas/roo-calamine/commit/c2417fb3238d76da4fe30bd437ea95b2859bc604))
* stream xlsx 30% faster by building roo cells without their constructors ([#15](https://github.com/salasebas/roo-calamine/issues/15)) ([5f4561a](https://github.com/salasebas/roo-calamine/commit/5f4561a3821fc94f3c63c8eb1d3bfd273f7cffc7))

## 0.0.1 (2026-11-10)


### Features

* make roo read xlsx files 4-12x faster with identical results ([#1](https://github.com/salasebas/roo-calamine/issues/1)) ([dce0aa4](https://github.com/salasebas/roo-calamine/commit/dce0aa4737b6e376ef01eb4f2aa8f602331d4d0b))
