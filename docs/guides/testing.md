## Prerequisites for Locker API Testing

A postman collection is included in `docs/collection/Tartarus.postman_collection.json` for testing the Locker API. The collection contains requests for all the endpoints of the Locker API.

Note: The requests in the Postman collection will not work when the `middleware` feature is enabled. This is because the requests need to be JWE (JSON Web Encryption) + JWS (JSON Web Signature) encrypted.

To encrypt and decrypt the requests and responses, a utility is provided in the same crate. Here's how to use this utility:

Encrypting the Request Before Sending

```bash
$ cat request.json | cargo run --bin utils -- jwe-encrypt --priv <CLIENT_PRIVATE_KEY.pem> --pub <LOCKER_PUBLIC_KEY.pem>
```

Decrypting the Response After Receiving

```bash

$ cat response.json | cargo run --bin utils -- jwe-decrypt --priv <CLIENT_PRIVATE_KEY.pem> --pub <LOCKER_PUBLIC_KEY.pem>

```

In the above commands:

- `CLIENT_PRIVATE_KEY.pem` is the private key of the client that is going to use the vault (in general, it would be Hyperswitch).
- `LOCKER_PUBLIC_KEY.pem` is the public key of the locker.

Note: The process of generating the keys is mentioned in the [`setup.md`](./setup.md) of the locker crate.
