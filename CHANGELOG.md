# Changelog

All notable changes to hyperswitch-card-vault will be documented here.

- - -

## 0.6.0 (2024-12-17)

### Features

- **v2:** Adding support for v2 API ([#135](https://github.com/juspay/hyperswitch-card-vault/pull/135)) ([`229c4d3`](https://github.com/juspay/hyperswitch-card-vault/commit/229c4d384629ca01fa5d9eba96ef46ee2108098e))

**Full Changelog:** [`v0.5.1...v0.6.0`](https://github.com/juspay/hyperswitch-card-vault/compare/v0.5.1...v0.6.0)

- - -


## 0.5.1 (2024-12-10)

### Miscellaneous Tasks

- Include `reqwest/rustls-tls` feature to keymanager_mtls ([#134](https://github.com/juspay/hyperswitch-card-vault/pull/134)) ([`5cbb06f`](https://github.com/juspay/hyperswitch-card-vault/commit/5cbb06fc9a983f48d719146d8cb37ce9904eae8b))

**Full Changelog:** [`v0.5.0...v0.5.1`](https://github.com/juspay/hyperswitch-card-vault/compare/v0.5.0...v0.5.1)

- - -


## 0.5.0 (2024-12-10)

### Features

- **caching+fingerprint:** Add support for caching for fingerprint API ([#80](https://github.com/juspay/hyperswitch-card-vault/pull/80)) ([`7deb933`](https://github.com/juspay/hyperswitch-card-vault/commit/7deb9336e3d4c433692fd57a93aacef4aaf6d329))
- **health:** Add deep health check with support for diagnostics ([#64](https://github.com/juspay/hyperswitch-card-vault/pull/64)) ([`07a115b`](https://github.com/juspay/hyperswitch-card-vault/commit/07a115bb363ef7a674143e2d299d0ce3da522966))
- **keymanager:** Add support for sending master key to key manager ([#131](https://github.com/juspay/hyperswitch-card-vault/pull/131)) ([`3e7cf0f`](https://github.com/juspay/hyperswitch-card-vault/commit/3e7cf0fc79002662fe3b85ed518e7841394d6f35))
- **logging:** Add `console-subscriber` to support monitoring on tokio ([#123](https://github.com/juspay/hyperswitch-card-vault/pull/123)) ([`07bc64e`](https://github.com/juspay/hyperswitch-card-vault/commit/07bc64e4910d32f4603fa4e70f4823cbc056c1dd))
- **router:**
  - Handle 4xx errors ([#81](https://github.com/juspay/hyperswitch-card-vault/pull/81)) ([`71fe9d1`](https://github.com/juspay/hyperswitch-card-vault/commit/71fe9d1aee98b3dac6218081bfbf0d086d1be901))
  - Add v2 api for /fingerprint ([#119](https://github.com/juspay/hyperswitch-card-vault/pull/119)) ([`cc083fa`](https://github.com/juspay/hyperswitch-card-vault/commit/cc083fa99b3fd4c924dc5103c6593793d0823536))
- **ttl:** Add ttl to locker entries ([#88](https://github.com/juspay/hyperswitch-card-vault/pull/88)) ([`2a10a09`](https://github.com/juspay/hyperswitch-card-vault/commit/2a10a090c880ca289657bf007d31cc2dee224f60))
- Add support for multi-tenancy ([#97](https://github.com/juspay/hyperswitch-card-vault/pull/97)) ([`0b41de8`](https://github.com/juspay/hyperswitch-card-vault/commit/0b41de8d9880a326a26883ba7be84733ea139c33))
- Integrate a secret manager ([#110](https://github.com/juspay/hyperswitch-card-vault/pull/110)) ([`8849f42`](https://github.com/juspay/hyperswitch-card-vault/commit/8849f429c82c7677dd0a58c6235084ddaae99334))

### Bug Fixes

- Address non-digit character cases in card number validation ([#93](https://github.com/juspay/hyperswitch-card-vault/pull/93)) ([`f25efeb`](https://github.com/juspay/hyperswitch-card-vault/commit/f25efeb5ac44d4ea021dd9535d5b166b7f10826d))

### Refactors

- **ttl:** Add support for accepting ttl in seconds as opposed to datetime ([#89](https://github.com/juspay/hyperswitch-card-vault/pull/89)) ([`4c193a4`](https://github.com/juspay/hyperswitch-card-vault/commit/4c193a4a2372c89bbefee4fe35d19a213dff29fa))
- Move crypto related managers to separate modules ([#95](https://github.com/juspay/hyperswitch-card-vault/pull/95)) ([`d2a153f`](https://github.com/juspay/hyperswitch-card-vault/commit/d2a153f61d3f97baee1f54c1a813d7f55396716f))
- Remove `tenant_id` column from all existing tables ([#105](https://github.com/juspay/hyperswitch-card-vault/pull/105)) ([`1ec3248`](https://github.com/juspay/hyperswitch-card-vault/commit/1ec32480353e7f128d0b5062a460f92ad983b2c4))
- Add db migrations for v2 ([#107](https://github.com/juspay/hyperswitch-card-vault/pull/107)) ([`7f1c0d1`](https://github.com/juspay/hyperswitch-card-vault/commit/7f1c0d1bc8b0790770ce12a35634e587d238b43e))

### Miscellaneous Tasks

- Include postman collection in docs ([#87](https://github.com/juspay/hyperswitch-card-vault/pull/87)) ([`4412bbd`](https://github.com/juspay/hyperswitch-card-vault/commit/4412bbd20a552f3cf6b6740e526fbaf74b64b830))
- Add support for schema as a key in tenant secrets ([#120](https://github.com/juspay/hyperswitch-card-vault/pull/120)) ([`06a0414`](https://github.com/juspay/hyperswitch-card-vault/commit/06a04149f4fc689aedfaf21b3964658f5861138f))
- Include tenancy docs in setup ([#122](https://github.com/juspay/hyperswitch-card-vault/pull/122)) ([`db89f3b`](https://github.com/juspay/hyperswitch-card-vault/commit/db89f3be1e09dbf80a29828da20f5367c6d7fb1b))

### Build System / Dependencies

- **deps:** Bump dependencies to supported versions ([#115](https://github.com/juspay/hyperswitch-card-vault/pull/115)) ([`4aa7441`](https://github.com/juspay/hyperswitch-card-vault/commit/4aa744169191b1f1fe444bb666b6ac1c3f4efa01))
- Bump MSRV to 1.75.0 ([#77](https://github.com/juspay/hyperswitch-card-vault/pull/77)) ([`4e4fb9b`](https://github.com/juspay/hyperswitch-card-vault/commit/4e4fb9bc321b95a13e4aa2955933960ea3a1475b))
- Obtain workspace member package names from cargo_metadata more deterministically ([#84](https://github.com/juspay/hyperswitch-card-vault/pull/84)) ([`2f08c4e`](https://github.com/juspay/hyperswitch-card-vault/commit/2f08c4e00fd29898865ca310a3576ee638ef98ac))

**Full Changelog:** [`v0.4.0...v0.5.0`](https://github.com/juspay/hyperswitch-card-vault/compare/v0.4.0...v0.5.0)

- - -


## 0.4.0 (2024-02-08)

### Features

- **fingerprint:**
  - Add fingerprint table and db interface ([#75](https://github.com/juspay/hyperswitch-card-vault/pull/75)) ([`bf57a3c`](https://github.com/juspay/hyperswitch-card-vault/commit/bf57a3c182786fbf692eb38776730f9a9186af41))
  - Add api for fingerprint ([#76](https://github.com/juspay/hyperswitch-card-vault/pull/76)) ([`48503ff`](https://github.com/juspay/hyperswitch-card-vault/commit/48503ff88181621f6d1d246b6e8e8056e5ec924d))
- **hmac:** Add implementation for `hmac-sha512` ([#74](https://github.com/juspay/hyperswitch-card-vault/pull/74)) ([`e3eea9a`](https://github.com/juspay/hyperswitch-card-vault/commit/e3eea9a32f8165aadcd6075f032875e9d37d9b56))

**Full Changelog:** [`v0.3.0...v0.4.0`](https://github.com/juspay/hyperswitch-card-vault/compare/v0.3.0...v0.4.0)

- - -


## 0.3.0 (2024-02-05)

### Features

- **benches:** Introduce benchmarks for internal components ([#53](https://github.com/juspay/hyperswitch-card-vault/pull/53)) ([`8a7bbc3`](https://github.com/juspay/hyperswitch-card-vault/commit/8a7bbc3f6db41e938e19fd36bb55bd3b7ef585b0))
- **caching:** Implement hash_table and merchant table caching ([#55](https://github.com/juspay/hyperswitch-card-vault/pull/55)) ([`f0d4cc4`](https://github.com/juspay/hyperswitch-card-vault/commit/f0d4cc45868977505fe34023161c31505aabc348))
- **hashicorp-kv:** Add feature to extend key management service at runtime ([#65](https://github.com/juspay/hyperswitch-card-vault/pull/65)) ([`9260782`](https://github.com/juspay/hyperswitch-card-vault/commit/9260782cbe5dfc11e0674cdea7dda267c0710e8f))
- **router:** Add `duplication_check` field in stored card response([#59](https://github.com/juspay/hyperswitch-card-vault/pull/59)) ([`358cdb8`](https://github.com/juspay/hyperswitch-card-vault/commit/358cdb8d89b594a0f273dd976867879357c19ef3))

### Miscellaneous Tasks

- **deps:** Update axum `0.6.20` to `0.7.3` ([#66](https://github.com/juspay/hyperswitch-card-vault/pull/66)) ([`7b8e116`](https://github.com/juspay/hyperswitch-card-vault/commit/7b8e1163a4a2a51fc2bc4404c6608a4d3a572ed4))
- Fix caching issue for conditional merchant creation ([#68](https://github.com/juspay/hyperswitch-card-vault/pull/68)) ([`258b3ac`](https://github.com/juspay/hyperswitch-card-vault/commit/258b3ac416bcdfcc731cfa88967a42ea690347e1))

**Full Changelog:** [`v0.2.0...v0.3.0`](https://github.com/juspay/hyperswitch-card-vault/compare/v0.2.0...v0.3.0)

- - -


## 0.2.0 (2023-12-26)

### Features

- **router:** Use only card number for card duplication check ([#57](https://github.com/juspay/hyperswitch-card-vault/pull/57)) ([`5781603`](https://github.com/juspay/hyperswitch-card-vault/commit/57816033433ee6355a856e0dacd57688847ba1f1))

### Miscellaneous Tasks

- **deps:** Update version of aws dependencies ([#54](https://github.com/juspay/hyperswitch-card-vault/pull/54)) ([`1142449`](https://github.com/juspay/hyperswitch-card-vault/commit/1142449795080293aa2fad780a53e553811de3e6))
- **utils:**
  - Add jwe operations in utils binary ([#60](https://github.com/juspay/hyperswitch-card-vault/pull/60)) ([`68f3455`](https://github.com/juspay/hyperswitch-card-vault/commit/68f34554838bb00141eb0e10256cf6664dd383d6))
  - Fix jwe operations in utils binary ([#61](https://github.com/juspay/hyperswitch-card-vault/pull/61)) ([`94016bb`](https://github.com/juspay/hyperswitch-card-vault/commit/94016bb1983d5d7f7b624e57900e256bf5409bf0))

**Full Changelog:** [`v0.1.3...v0.2.0`](https://github.com/juspay/hyperswitch-card-vault/compare/v0.1.3...v0.2.0)

- - -


## 0.1.3 (2023-11-24)

### Bug Fixes

- **luhn:** Fix the check by reversing ordering ([#51](https://github.com/juspay/hyperswitch-card-vault/pull/51)) ([`61c0164`](https://github.com/juspay/hyperswitch-card-vault/commit/61c01644dda83d4bc56947610b06fc19633a9ba1))

### Miscellaneous Tasks

- Bump the crate version to 0.1.3 ([#52](https://github.com/juspay/hyperswitch-card-vault/pull/52)) ([`bf5fe37`](https://github.com/juspay/hyperswitch-card-vault/commit/bf5fe370b125f1e0b395512f98b416b854caca81))

**Full Changelog:** [`v0.1.2...v0.1.3`](https://github.com/juspay/hyperswitch-card-vault/compare/v0.1.2...v0.1.3)

- - -


## 0.1.2 (2023-11-21)

### Features

- **card+config:** Add cards API and config pulling feature ([`1c9569c`](https://github.com/juspay/hyperswitch-card-vault/commit/1c9569ce163ac862a42d63f14df4e3dce978baaa))
- **cargo:** Add limiting and release build improvements ([`22bdcdd`](https://github.com/juspay/hyperswitch-card-vault/commit/22bdcdd57d2cfaa8d3a39aa72b6147735e48b1b0))
- **db:** Add variable pool size ([#45](https://github.com/juspay/hyperswitch-card-vault/pull/45)) ([`0f6ee81`](https://github.com/juspay/hyperswitch-card-vault/commit/0f6ee8147a4dc3d2566cfce7e2af3778cd5ac7a7))
- **docker:**
  - Add Dockerfile ([`107f53b`](https://github.com/juspay/hyperswitch-card-vault/commit/107f53b31b3bb27029eefbaa924cee0f8159599b))
  - Add docker file and test it ([`031d813`](https://github.com/juspay/hyperswitch-card-vault/commit/031d81359185936b7149fcfe04465018fa22cf61))
- **hash:** Add support for detecting data duplication ([`6a23a7d`](https://github.com/juspay/hyperswitch-card-vault/commit/6a23a7d48f78daed8e4abc26217984c1de72bbd0))
- **key_custodian:** Encrypt master key with 2 custodian keys ([`064dcca`](https://github.com/juspay/hyperswitch-card-vault/commit/064dccaf99451a5fb88f3596090362cef84a9350))
- **kms:**
  - Integrate kms feature ([`ead558d`](https://github.com/juspay/hyperswitch-card-vault/commit/ead558db2268d00c33c731252c0bc0002d7a8b0e))
  - Integrate kms feature ([`00bf1ae`](https://github.com/juspay/hyperswitch-card-vault/commit/00bf1ae4af0b0c35fc2d49428e23ae79d508f13d))
- **loadtest:** Add support for loadtesting ([`fcb0428`](https://github.com/juspay/hyperswitch-card-vault/commit/fcb042839070b40740ddd555ca1e59585526cea4))
- **logging:** Add logging framework ([`427db97`](https://github.com/juspay/hyperswitch-card-vault/commit/427db97b597aa1e7f25322bda4dbb17872ef3dc1))
- **ratelimit:** Add rate limit to delete api ([`845296e`](https://github.com/juspay/hyperswitch-card-vault/commit/845296e0ad3fa23c5d05cdba79d18a416a37e38d))
- **trace:** Add tracing middleware for tracing requests ([`2b00866`](https://github.com/juspay/hyperswitch-card-vault/commit/2b00866b42d9c9a7ab0112b07346f3b0b2f42fad))
- **utils:** Add utils binary for key generation ([`c3edc13`](https://github.com/juspay/hyperswitch-card-vault/commit/c3edc136b8c41c1cbc1f6f0111904ece810c54bc))

### Bug Fixes

- **error:** Rewrite the error framework with custom change contexts ([`af78b58`](https://github.com/juspay/hyperswitch-card-vault/commit/af78b588ad627094d9db24a9e00f43ba5451bc5e))
- **loadtest:** Add jwe to loadtest ([`afcfd8c`](https://github.com/juspay/hyperswitch-card-vault/commit/afcfd8c56d4f39eabd60f5a31230d500cfe90434))
- **validation:** Add key validation and card number validation ([`250ebfa`](https://github.com/juspay/hyperswitch-card-vault/commit/250ebfae1970a1cfa9b776682b5636fb58e89fc9))
- Fix clippy errors in main ([`93d9eb4`](https://github.com/juspay/hyperswitch-card-vault/commit/93d9eb4da975305e5d0f29d243192a2cf115768c))
- Clippy lints ([`083e2f2`](https://github.com/juspay/hyperswitch-card-vault/commit/083e2f2ed22ea683476a5a70d301e1c30c9141ca))

### Refactors

- **kms:** Enable kms feature for configs ([`18fb1fa`](https://github.com/juspay/hyperswitch-card-vault/commit/18fb1fa38ac37b61b2651e7b2a41c1e1c955a84f))
- Address requested changes ([`39b53c6`](https://github.com/juspay/hyperswitch-card-vault/commit/39b53c6d8fcf7062c86e35fddaf094bd23372186))
- Add logs to existing routes ([`6525abe`](https://github.com/juspay/hyperswitch-card-vault/commit/6525abe6814cddbdf6d7c9f7b0d667b4424d1e30))
- Address requested changes ([`3ae7a9c`](https://github.com/juspay/hyperswitch-card-vault/commit/3ae7a9c87650cd11812d57711bc825dc02ec438b))
- Hex decode master_key ([`b85d656`](https://github.com/juspay/hyperswitch-card-vault/commit/b85d65613a3a669aa1a803d588151747de5d83d7))

### Testing

- **crypto:** Add tests for jwe ([`8744683`](https://github.com/juspay/hyperswitch-card-vault/commit/874468364870867542962d09f8518b62c0b72415))

### Documentation

- **openapi:** Add openapi spec to docs ([`9b58830`](https://github.com/juspay/hyperswitch-card-vault/commit/9b58830de33d1a039b549ca9148de4d361a792f0))
- **setup:** Add setup guide for locker ([`6f30ce6`](https://github.com/juspay/hyperswitch-card-vault/commit/6f30ce6f7e9b660d68948d34bd2a2fa70241ee0d))
- Create LICENSE ([#44](https://github.com/juspay/hyperswitch-card-vault/pull/44)) ([`e7f7db4`](https://github.com/juspay/hyperswitch-card-vault/commit/e7f7db47d411a7dd6797bdd0fefe87ac7c48250e))

### Miscellaneous Tasks

- Minor fixes ([`d23284b`](https://github.com/juspay/hyperswitch-card-vault/commit/d23284b98a5628bd5ade8e471b6982cfe2687bfc))
- Fmt check ([`40ce145`](https://github.com/juspay/hyperswitch-card-vault/commit/40ce1453a8fdcb2a21028465d0bf2a0491acc9e4))
- Minor fixes ([`6755d82`](https://github.com/juspay/hyperswitch-card-vault/commit/6755d829ac0edabd0f0407688c58d898adce51ad))
- Remove unnecessary clones from routes ([`b4bdb10`](https://github.com/juspay/hyperswitch-card-vault/commit/b4bdb102fad4a5eeabda60fc484ffe52bee8875d))
- Fix clippy + fmt errors ([`94c93c3`](https://github.com/juspay/hyperswitch-card-vault/commit/94c93c32cd709c78fc3c19b7368ad5e962c77a23))
- Fix dockerfile ([`3794d99`](https://github.com/juspay/hyperswitch-card-vault/commit/3794d99945134ab4f232775d69563c75dd7e822a))
- Fix error message and and custom status code mapping ([`e29650f`](https://github.com/juspay/hyperswitch-card-vault/commit/e29650ffa0fd81e032a353c26dc4e7aec0806488))
- Fix minor bugs after adding stricter linting ([`f8d7ac0`](https://github.com/juspay/hyperswitch-card-vault/commit/f8d7ac0b636819d0883fad3f6aac51108339e720))
- Address comments and fix cargo hack ([`88ca5ee`](https://github.com/juspay/hyperswitch-card-vault/commit/88ca5eecd8fed18f17aec74f632e9ca52b783999))
- Format yaml files ([`678ae44`](https://github.com/juspay/hyperswitch-card-vault/commit/678ae441a7cbea34ac2806dd78d634a81f3027a1))
- Remove commented code ([`f16c841`](https://github.com/juspay/hyperswitch-card-vault/commit/f16c841121d54580de33dd575c55b98c57d6aece))
- Remove redundant keys ([`5898755`](https://github.com/juspay/hyperswitch-card-vault/commit/58987557cf63277fe55ef4c0d63f4052218422e3))
- Remove cargo.toml changes ([`f548350`](https://github.com/juspay/hyperswitch-card-vault/commit/f5483501466f75617a9976e214f5ab1ffca3df30))
- Add formatting for markdown ([`c67b4c1`](https://github.com/juspay/hyperswitch-card-vault/commit/c67b4c17bebff0b51ba3dc263960e643a755e86e))
- Remove commented code and println ([`d2b5873`](https://github.com/juspay/hyperswitch-card-vault/commit/d2b5873968c3a0090043ecd814e3d368f22e9e95))
- Update README.md ([`65cc26d`](https://github.com/juspay/hyperswitch-card-vault/commit/65cc26d0ecda25a2be44fd6bd246493a5e9ea7fd))
- Add semi-colon in migrations to make it work ([`5c10107`](https://github.com/juspay/hyperswitch-card-vault/commit/5c101079f9749d5a7510fdd0b7ff5000b3079245))
- Remove default changes ([`34c376c`](https://github.com/juspay/hyperswitch-card-vault/commit/34c376cb8777d26f051f631262177ca0f6a6bb2c))
- Add env variables in setup.md ([`ef998a2`](https://github.com/juspay/hyperswitch-card-vault/commit/ef998a2d087fbf33600be761c8d20d6ce232c1f3))
- Move allow blocks to functions ([`379ad8a`](https://github.com/juspay/hyperswitch-card-vault/commit/379ad8a9b35f593d12aebc3049b5efd6501d8007))
- Add example config ([`fe8ea20`](https://github.com/juspay/hyperswitch-card-vault/commit/fe8ea20329b9f2f27262c80364c6e9a178345ec9))
- Fix merge conflicts ([`c5c57f6`](https://github.com/juspay/hyperswitch-card-vault/commit/c5c57f687f2a641595bb5d2322417a4bc50ce799))

**Full Changelog:** [`69979a0ae15c8fe3180aea17949fce9fc0ee2335...v0.1.2`](https://github.com/juspay/hyperswitch-card-vault/compare/69979a0ae15c8fe3180aea17949fce9fc0ee2335...v0.1.2)
