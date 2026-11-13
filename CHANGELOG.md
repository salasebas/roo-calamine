# Changelog

## [0.0.2](https://github.com/salasebas/roo-oxide/compare/v0.0.1...v0.0.2) (2026-11-13)


### Features

* name the native Roo accelerator roo-oxide ([#78](https://github.com/salasebas/roo-oxide/issues/78)) ([6a67945](https://github.com/salasebas/roo-oxide/commit/6a6794599b3fe2ff3c68d11b82b0c78f9b812c1f))
* read values 2x faster than streaming with an opt-in Roo::Calamine.each_row_values ([#12](https://github.com/salasebas/roo-oxide/issues/12)) ([c4d02de](https://github.com/salasebas/roo-oxide/commit/c4d02de37458641a9e24db0de0d3b5eef7da5927))


### Performance Improvements

* build cell coordinates in Rust for 3-13% faster reads ([#13](https://github.com/salasebas/roo-oxide/issues/13)) ([42a57f5](https://github.com/salasebas/roo-oxide/commit/42a57f5d044000fda04022e8de500d7fbb5deb53))
* build cell coordinates in Rust instead of Coordinate.new ([42a57f5](https://github.com/salasebas/roo-oxide/commit/42a57f5d044000fda04022e8de500d7fbb5deb53))
* build roo cells without running their Ruby constructors ([5f4561a](https://github.com/salasebas/roo-oxide/commit/5f4561a3821fc94f3c63c8eb1d3bfd273f7cffc7))
* cut clean row iteration time by 62% ([#70](https://github.com/salasebas/roo-oxide/issues/70)) ([43914e9](https://github.com/salasebas/roo-oxide/commit/43914e90cc018ba07fcc29f92d3e2a866ab3eccb))
* cut each_row_values peak memory by up to 30% by converting rows straight to values ([#23](https://github.com/salasebas/roo-oxide/issues/23)) ([82c2d69](https://github.com/salasebas/roo-oxide/commit/82c2d693b905df0d1aba4b195e930b898b92b34d))
* cut IO workbook streaming memory by 53-58% ([#56](https://github.com/salasebas/roo-oxide/issues/56)) ([735e7b0](https://github.com/salasebas/roo-oxide/commit/735e7b00b6c8e335704492a5fdc5fa92d27312ed))
* cut parse allocations by 200k and small cell lookup time by 20% ([#42](https://github.com/salasebas/roo-oxide/issues/42)) ([fdb6a93](https://github.com/salasebas/roo-oxide/commit/fdb6a9335d53d7c8117bce8d47b1e448dce81cf5))
* cut peak memory 11–19% (65 MiB) on workbooks with many unique strings by extracting sharedStrings.xml natively ([#37](https://github.com/salasebas/roo-oxide/issues/37)) ([7722a73](https://github.com/salasebas/roo-oxide/commit/7722a733eef5ca146d6ea4d5639aaf36fe22d1f8))
* cut peak memory of row, column, and cell reads by up to 26% with 20-byte native records ([#32](https://github.com/salasebas/roo-oxide/issues/32)) ([f165dc5](https://github.com/salasebas/roo-oxide/commit/f165dc5048e5061c9f5a05ba60256f0309670ee9))
* cut row reads by parsing plain cells directly ([fd71931](https://github.com/salasebas/roo-oxide/commit/fd7193170749a7462e8450d131e501317aad5c9b))
* cut small-workbook opening time by 37% ([#60](https://github.com/salasebas/roo-oxide/issues/60)) ([2cd40eb](https://github.com/salasebas/roo-oxide/commit/2cd40eb5e3c63839dcfdd9b28ca76567f21916ac))
* cut temporal CSV and formatted read times by 46–66% ([#71](https://github.com/salasebas/roo-oxide/issues/71)) ([21f9da6](https://github.com/salasebas/roo-oxide/commit/21f9da66f1260eb41469f1b6ef57a98b8d57f871))
* cut unique-string document reads by 25–42% ([#61](https://github.com/salasebas/roo-oxide/issues/61)) ([2660f9a](https://github.com/salasebas/roo-oxide/commit/2660f9a113800fdb6819cd6ecf77193f329227d2))
* cut unique-string peak memory 23–43% by deferring HTML strings ([#44](https://github.com/salasebas/roo-oxide/issues/44)) ([7806c6f](https://github.com/salasebas/roo-oxide/commit/7806c6fe40f9f52e9aa048be186d4a79ec24016a))
* export YAML, XML and Matrix 19–51× faster ([#58](https://github.com/salasebas/roo-oxide/issues/58)) ([e979933](https://github.com/salasebas/roo-oxide/commit/e979933c97338a768bc606d7b374f50621081481))
* hand each_row_streaming the native row array directly ([bf37bc7](https://github.com/salasebas/roo-oxide/commit/bf37bc717c51f43f1b9a8745ff066f42673d55fd))
* make each 8× faster on sparse sheets built whole by indexing their cells by row ([#30](https://github.com/salasebas/roo-oxide/issues/30)) ([f915c14](https://github.com/salasebas/roo-oxide/commit/f915c14ad69325988b7006627652ba736ca9738f))
* make each and parse(headers: true) 2–2.6× faster with half the memory by reading rows from the native records ([#25](https://github.com/salasebas/roo-oxide/issues/25)) ([e5ae0a9](https://github.com/salasebas/roo-oxide/commit/e5ae0a91ab814e5aa6eb2cd70cd4e31fcfc34b87))
* make each without options up to 47% faster on large sheets ([#18](https://github.com/salasebas/roo-oxide/issues/18)) ([739f02c](https://github.com/salasebas/roo-oxide/commit/739f02c5fed5f46c074a91776ba62180a75bc0d3))
* make header iteration as fast as parse ([#57](https://github.com/salasebas/roo-oxide/issues/57)) ([03f8520](https://github.com/salasebas/roo-oxide/commit/03f8520a59ceb938a2f357ab2dad6a8fb0e27811))
* make parse(headers: true) 2x faster by grouping the cells hash in one pass ([#14](https://github.com/salasebas/roo-oxide/issues/14)) ([1d15aae](https://github.com/salasebas/roo-oxide/commit/1d15aae5d0ef83413be288157f012e517c35e54f))
* make single-cell reads 2x faster with 3x less memory by building cells lazily ([#17](https://github.com/salasebas/roo-oxide/issues/17)) ([c2417fb](https://github.com/salasebas/roo-oxide/commit/c2417fb3238d76da4fe30bd437ea95b2859bc604))
* read columns 2.6–8.4× faster with half the memory from the native records ([#31](https://github.com/salasebas/roo-oxide/issues/31)) ([8b0b683](https://github.com/salasebas/roo-oxide/commit/8b0b683935b3ec7f78513f360221d4eb208a5638))
* read hyperlinks and merged ranges natively instead of loading the whole sheet into Nokogiri ([29e5fd6](https://github.com/salasebas/roo-oxide/commit/29e5fd64ce4e1a36d11b5eee7a48fc75fa49e792))
* read row values and random cells 30-40% faster by parsing each tag's attributes once ([#22](https://github.com/salasebas/roo-oxide/issues/22)) ([afd5ad1](https://github.com/salasebas/roo-oxide/commit/afd5ad13b88670eefa4647ce5b41e960d500b04f))
* read sheets with hyperlinks or expanded merged ranges 30-60% faster in up to 20x less memory ([#26](https://github.com/salasebas/roo-oxide/issues/26)) ([29e5fd6](https://github.com/salasebas/roo-oxide/commit/29e5fd64ce4e1a36d11b5eee7a48fc75fa49e792))
* read small workbook rows 8–13% faster with in-memory metadata ([#45](https://github.com/salasebas/roo-oxide/issues/45)) ([eb7c4e9](https://github.com/salasebas/roo-oxide/commit/eb7c4e92ee31f981a6ffeaf32b7528ddba40125e))
* read small worksheets 11% faster natively with lazy ZIP inflation ([#43](https://github.com/salasebas/roo-oxide/issues/43)) ([65bd561](https://github.com/salasebas/roo-oxide/commit/65bd56137023b87c5e588ee6f9817191fe84f646))
* read workbooks full of unique or rich-text strings 4-17x faster by parsing shared strings natively ([#27](https://github.com/salasebas/roo-oxide/issues/27)) ([de00e34](https://github.com/salasebas/roo-oxide/commit/de00e3429d3d6d8b2a4a76e7db6f5ea3d9520f52))
* read worksheets from a copy of the xlsx instead of roo's extracted files ([77c232b](https://github.com/salasebas/roo-oxide/commit/77c232bfd736e85257782c767a48c73b6adec433))
* read worksheets straight from a copy of the xlsx zip, with up to 60% less peak memory on large files ([#28](https://github.com/salasebas/roo-oxide/issues/28)) ([77c232b](https://github.com/salasebas/roo-oxide/commit/77c232bfd736e85257782c767a48c73b6adec433))
* reduce Array row iteration overhead ([#59](https://github.com/salasebas/roo-oxide/issues/59)) ([5ab8230](https://github.com/salasebas/roo-oxide/commit/5ab823065464f9c38dbbe9f6f55b87e4ac5eb76f))
* reduce native cell construction overhead ([c974479](https://github.com/salasebas/roo-oxide/commit/c9744797ca42b46787479195201a6863e32b6856))
* reduce row-read overhead by parsing plain cells directly ([#69](https://github.com/salasebas/roo-oxide/issues/69)) ([fd71931](https://github.com/salasebas/roo-oxide/commit/fd7193170749a7462e8450d131e501317aad5c9b))
* reduce single-cell read latency on small sheets ([#73](https://github.com/salasebas/roo-oxide/issues/73)) ([194b149](https://github.com/salasebas/roo-oxide/commit/194b149d64a2682fa43fc54319a640dfe4eb300e))
* speed up each_row_values 7–9% and each, cell and parse 3–9% by reading common cell attributes on a fast path ([#36](https://github.com/salasebas/roo-oxide/issues/36)) ([d98ff8a](https://github.com/salasebas/roo-oxide/commit/d98ff8ac9ba96e30badab264e5e37a41029fe9cc))
* speed up hyperlink reads and row iteration ([#72](https://github.com/salasebas/roo-oxide/issues/72)) ([4f47c26](https://github.com/salasebas/roo-oxide/commit/4f47c264ae9ead7f83dbc5e3c4596d53f3172b55))
* speed up initial temporal scans 40–42% with 31–33% less memory ([#41](https://github.com/salasebas/roo-oxide/issues/41)) ([c974479](https://github.com/salasebas/roo-oxide/commit/c9744797ca42b46787479195201a6863e32b6856))
* stream rows 7–18% faster by yielding the native row array without copying it ([#35](https://github.com/salasebas/roo-oxide/issues/35)) ([bf37bc7](https://github.com/salasebas/roo-oxide/commit/bf37bc717c51f43f1b9a8745ff066f42673d55fd))
* stream tall sheets ~10% faster by handing Ruby one row at a time ([#24](https://github.com/salasebas/roo-oxide/issues/24)) ([4370a4b](https://github.com/salasebas/roo-oxide/commit/4370a4bac1656b01ff976b3815f9eb382101c83b))
* stream xlsx 30% faster by building roo cells without their constructors ([#15](https://github.com/salasebas/roo-oxide/issues/15)) ([5f4561a](https://github.com/salasebas/roo-oxide/commit/5f4561a3821fc94f3c63c8eb1d3bfd273f7cffc7))
* write to_csv 2–22× faster with byte-identical output by walking the cells once ([#29](https://github.com/salasebas/roo-oxide/issues/29)) ([2094b39](https://github.com/salasebas/roo-oxide/commit/2094b39d9259dd1787b457a4d4c6dff9df7e6cf1))
* write to_csv 2.2–3.9× faster with up to 83% less peak memory from the native records ([#38](https://github.com/salasebas/roo-oxide/issues/38)) ([8293459](https://github.com/salasebas/roo-oxide/commit/829345998667a208d1c76abc26f4242f2c1a2bd8))

## 0.0.1 (2026-11-10)


### Features

* make roo read xlsx files 4-12x faster with identical results ([#1](https://github.com/salasebas/roo-calamine/issues/1)) ([dce0aa4](https://github.com/salasebas/roo-calamine/commit/dce0aa4737b6e376ef01eb4f2aa8f602331d4d0b))
