
## locker phases

### phase 1
working locker
- configuration: pull configuration from toml file and environment variables
- cards api: ability to perform add, retrieve, delete card actions
- jwe + jws: encryption and signing middleware with keys passed in [`configuration`]
- master key to be passed in configuration
- key custodian: add support to decrypt the master key before starting the server

### phase 2
- tenant api for adding and deleting tenants
- add support for *kms* encryption on the environment variables
- adding logging and metrics for locker
- tenant specific jwe public key storage and utilizing key_id for identification
- docker + kubernetes setup for infra deployment

### phase 3 (optional)
- add support for key rotation
