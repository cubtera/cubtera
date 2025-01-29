# Changelog

## [1.0.14](https://github.com/cubtera/cubtera/compare/v1.0.13...v1.0.14) (2025-01-29)


### Bug Fixes

* **dim:** add configuration for inventory filename separator + refactoring ([01bfb44](https://github.com/cubtera/cubtera/commit/01bfb44272a44a65291f5acd5bed18a6e0667d01))
* **tf_switch:** refactor lock/wait during tf bin download in parallel runs ([a22e2b3](https://github.com/cubtera/cubtera/commit/a22e2b30d40a110540e2b7705375f6d611d259bf))

## [1.0.13](https://github.com/cubtera/cubtera/compare/v1.0.12...v1.0.13) (2025-01-16)


### Bug Fixes

* add kids env var for TF units ([a4d9944](https://github.com/cubtera/cubtera/commit/a4d9944fc92c7a5e3b887bec428cd3af019c127b))
* add runner cmd as env_var propagation into inlet/outlet processes ([69c66bd](https://github.com/cubtera/cubtera/commit/69c66bde3282ef3d9605b00227dedf9db4077184))
* add runner failure with exit code if outlet/inlet fails ([d446781](https://github.com/cubtera/cubtera/commit/d446781f757002d33ea7d322e5b3a38a3e82646e))
* **dependabot:** update config requirement from 0.14 to 0.15 ([#34](https://github.com/cubtera/cubtera/issues/34)) ([4150b6e](https://github.com/cubtera/cubtera/commit/4150b6e670b3508b94f99a8520930aeb47f92c09))
* **dependabot:** update jsonschema requirement from 0.26 to 0.28 ([#33](https://github.com/cubtera/cubtera/issues/33)) ([4d283fe](https://github.com/cubtera/cubtera/commit/4d283fed3bb9296700c3a62fa3bf77219b955465))
* git methods relative path logic bug ([91f3ec8](https://github.com/cubtera/cubtera/commit/91f3ec8cd95cff0c29e0d6de18d719406b32a1ff))
* jsonschema crate version breaking change ([5b2060e](https://github.com/cubtera/cubtera/commit/5b2060ea58838d07bb30585f23f062a2581b20da))
* unit and dLog code refactoring ([87b12f0](https://github.com/cubtera/cubtera/commit/87b12f053414972f43ce610cfb3b75e78b5a9304))
* upgrade git crate version ([0f8505e](https://github.com/cubtera/cubtera/commit/0f8505e726561189bdc117e39bb4d01391b62a87))

## [1.0.12](https://github.com/cubtera/cubtera/compare/v1.0.11...v1.0.12) (2024-12-09)


### Bug Fixes

* add env vars for all tf commands ([05afdf0](https://github.com/cubtera/cubtera/commit/05afdf0f008b553a4c90b1ab3c21f7f1e2f61f7a))

## [1.0.11](https://github.com/cubtera/cubtera/compare/v1.0.10...v1.0.11) (2024-11-26)


### Bug Fixes

* add some cli colors ([6100edd](https://github.com/cubtera/cubtera/commit/6100edd0c95d34b687f9ed963daec2570f077fec))
* dlog extension ([#30](https://github.com/cubtera/cubtera/issues/30)) ([1daa49c](https://github.com/cubtera/cubtera/commit/1daa49c62a99902e61ed588ee54707b77ee154dc))

## [1.0.10](https://github.com/cubtera/cubtera/compare/v1.0.9...v1.0.10) (2024-10-21)


### Bug Fixes

* docker images build ([f085413](https://github.com/cubtera/cubtera/commit/f0854133584cbdda67c61821158d151108759385))
* remove unit_manifest.json processing ([c78d0e1](https://github.com/cubtera/cubtera/commit/c78d0e193fa7139bcc5d6861f21906746ef2a707))
* temp remove default local state config (for migration) ([d0625e9](https://github.com/cubtera/cubtera/commit/d0625e9b0584fb1dee639dc0f98a3f337dd7627d))

## [1.0.9](https://github.com/cubtera/cubtera/compare/v1.0.8...v1.0.9) (2024-10-20)


### Bug Fixes

* add build/push for api docker image ([6a69dd6](https://github.com/cubtera/cubtera/commit/6a69dd6525fc79e8692873ce3f77ab6e9cab3865))
* api async support new mongodb driver ([fa7df4c](https://github.com/cubtera/cubtera/commit/fa7df4cc15b149db4d2b5bebd829c32c73da8a17))
* s3 tf state vars broken string ([c25ef95](https://github.com/cubtera/cubtera/commit/c25ef952055b7cde67d44044f7ecb461d1778cd8))
* upgrade mongodb crate version to 3.1.0 with refactoring ([205efc2](https://github.com/cubtera/cubtera/commit/205efc238ed4bfdfcaab22662dbeba8ef3a3bcdf))

## [1.0.8](https://github.com/cubtera/cubtera/compare/v1.0.7...v1.0.8) (2024-10-15)


### Bug Fixes

* fix previous release ([ae54ef1](https://github.com/cubtera/cubtera/commit/ae54ef105ecad2144fa5617663b0b8f33b0b1005))

## [1.0.7](https://github.com/cubtera/cubtera/compare/v1.0.6...v1.0.7) (2024-10-14)


### Bug Fixes

* **dim:** fix only null meta vars for optional dims ([e42476a](https://github.com/cubtera/cubtera/commit/e42476aff2c88160182679d753133f995b152974))

## [1.0.6](https://github.com/cubtera/cubtera/compare/v1.0.5...v1.0.6) (2024-10-10)


### Bug Fixes

* add more internal tf vars available ([3d3713f](https://github.com/cubtera/cubtera/commit/3d3713f39ad889e1278902baba257009de69f4e8))
* add some modules related readme files ([3dedb7c](https://github.com/cubtera/cubtera/commit/3dedb7cb613d48baf43da8ef5096cee425a2da8e))
* add tf runner multi-backend logic ([2581324](https://github.com/cubtera/cubtera/commit/2581324eedb516d6f48c23530f61ee2546734087))
* **cfg:** remove deprecated params ([ad1b06a](https://github.com/cubtera/cubtera/commit/ad1b06a5698511374df86d8ff59eeab2e497876b))
* change errors description ([cd49b87](https://github.com/cubtera/cubtera/commit/cd49b87da4eb7cf64c2c26307344efa2ad4e9344))
* **lint:** remove redundant reference ([b6e91f8](https://github.com/cubtera/cubtera/commit/b6e91f8b343bb3ca3256e444bfccaecc55bf5b9f))

## [1.0.5](https://github.com/cubtera/cubtera/compare/v1.0.4...v1.0.5) (2024-10-02)


### Bug Fixes

* deprecated methods for json schema validator ([f0db236](https://github.com/cubtera/cubtera/commit/f0db236a24ec46e9bef0cd25233b85eca026d64a))
* remove release-please failed configuration ([aeade6f](https://github.com/cubtera/cubtera/commit/aeade6f783a525ac30fda78be4c8d81817e84b22))
* rollback release-please ([25f3160](https://github.com/cubtera/cubtera/commit/25f3160c2aa57aa1c2a09892ddd8215754b381cd))

## [1.0.4](https://github.com/cubtera/cubtera/compare/v1.0.3...v1.0.4) (2024-09-18)


### Bug Fixes

* add pr test action ([7801e7c](https://github.com/cubtera/cubtera/commit/7801e7cda522f118f3f0aced913a28d47bc9191e))
* add pr test action ([13b35cf](https://github.com/cubtera/cubtera/commit/13b35cf046ad19ba23ab77b422eb8ffb60fc2152))
* build action ([040ec89](https://github.com/cubtera/cubtera/commit/040ec89a8432ab7a1317341ebe57b2d8aa3020c2))
* test action ([d4796c9](https://github.com/cubtera/cubtera/commit/d4796c9c4e1ecbf1953285e15e89878fea8de8e8))
* test pr action ([720407f](https://github.com/cubtera/cubtera/commit/720407fd6adf82ee0dc5bf7a84a48c6f31e35f06))

## [1.0.3](https://github.com/cubtera/cubtera/compare/v1.0.2...v1.0.3) (2024-09-17)


### Bug Fixes

* add cache workaround for gh action ([77a78c1](https://github.com/cubtera/cubtera/commit/77a78c14ddcfa9e3f31473be999f2148d0cb7714))
* change sha256 value for release ([614fa33](https://github.com/cubtera/cubtera/commit/614fa3372c9a5b2d399d49a366f57ed69094d40d))
* remove redundant debug output ([67feec5](https://github.com/cubtera/cubtera/commit/67feec5c123beca3542287603fccb23c98a3b4dc))

## [1.0.2](https://github.com/cubtera/cubtera/compare/v1.0.1...v1.0.2) (2024-09-13)


### Miscellaneous Chores

* release 1.0.2 ([0ccb9b3](https://github.com/cubtera/cubtera/commit/0ccb9b301faa257cf40d572b201a73c5e0473897))

## [1.0.1](https://github.com/cubtera/cubtera/compare/v1.0.0...v1.0.1) (2024-09-13)


### Bug Fixes

* github action ([f054448](https://github.com/cubtera/cubtera/commit/f05444815150169a0a87830ead74ffb0d7fc7581))

## [1.0.0](https://github.com/cubtera/cubtera/compare/v0.1.2...v1.0.0) (2024-09-13)


### Features

* **ci:** test GH action for releases ([a63fac2](https://github.com/cubtera/cubtera/commit/a63fac2d45462d7e2801f1924e84cf884492dffd))


### Bug Fixes

* **ci:** add github action logic for hombrew distribution ([aca57aa](https://github.com/cubtera/cubtera/commit/aca57aaf499edb841cd032b1290883d1bb185012))
* **ci:** update gh action version ([67044db](https://github.com/cubtera/cubtera/commit/67044dba3c29975a37e846a01a095aae399f432c))
* **ci:** update gh action version ([e1abcdd](https://github.com/cubtera/cubtera/commit/e1abcdd760df634418801a3a4444f4d0da52d9f1))


### Miscellaneous Chores

* release 1.0.0 ([322e746](https://github.com/cubtera/cubtera/commit/322e7469579b0c8e1a8c0ad496cb627b06f9d44b))

## [0.1.2](https://github.com/cubtera/cubtera/compare/v0.1.1...v0.1.2) (2024-09-13)


### Bug Fixes

* **ci:** add github action logic for hombrew distribution ([aca57aa](https://github.com/cubtera/cubtera/commit/aca57aaf499edb841cd032b1290883d1bb185012))

## 4.0.0 (2024-09-12)


### Features

* **ci:** test GH action for releases ([a63fac2](https://github.com/cubtera/cubtera/commit/a63fac2d45462d7e2801f1924e84cf884492dffd))


### Bug Fixes

* **ci:** update gh action version ([e1abcdd](https://github.com/cubtera/cubtera/commit/e1abcdd760df634418801a3a4444f4d0da52d9f1))

## Changelog
