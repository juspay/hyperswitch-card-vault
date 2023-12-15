This API is encrypted using `JWE + JWS` for end-to-end encryption with the application. To test this API, you need to encrypt the json payload using the locker public key and hyperswitch private key for signing.

---


You can follow the below mentioned steps to encrypt and decrypt the payloads as required



> Prerequisites
>
> You need to use the utility provided with the application, to install that utility you can run the following command
> `cargo install --git https://github.com/juspay/hyperswitch-card-vault --bin utils --root .`
>
> You would also need the hyperswitch(tenant) private key and locker public key, while encrypting the payload
> (in the steps below, I am assuming that these keys are saved as `tenant-private-key.pem` and `locker-public-key.pem`)




1. To encrypt the payload call the utility
   `./bin/utils jwe-encrypt --priv tenant-private-key.pem --pub locker-public-key.pem`
   then paste the payload and close the buffer using `ctrl+d`
2. You can use the output of this command as the body for the request.
3. The output will also be JWE + JWS encrypted, you we can use the utility again to decrypt it.
   `./bin/utils jwe-decrypt --priv tenant-private-key.pem --pub locker-public-key.pem`
   then paste the response received and close the buffer using `ctrl+d`
